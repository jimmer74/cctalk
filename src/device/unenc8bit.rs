use crate::{
    device::{Currancy, counter::EC},
    errors::EventError,
};

use super::{
    BILL_EVENT_BUFF_LEN, CcTalkHeader, CctalkDevice, CctalkTransmissionError, DEFAULT_TIMEOUT_MS,
    DelayNs, Init, InitStatus, MASTER_ADDR, Msg8, PhantomData, Read, UnInit, Unenc8Bit, Write,
    hString, hVec,
};

/* =====================================================================================
 *
 *
 *                  Only for initialised 8bit devices
 *
 *
 * =====================================================================================
 * */
impl<U, D> CctalkDevice<Unenc8Bit, Init, U, D>
where
    D: DelayNs,
    U: Read + Write,
{
    /*===================================================================================
     *
     *
     *
     *                                  public methods
     *
     *
     *
     *===================================================================================
     * */

    pub fn get_int_events(&mut self) -> Result<hVec<IntEvent, 5>, CctalkTransmissionError> {
        let mut out_events: hVec<IntEvent, 5> = hVec::new();

        let in_events = self.read_events(Some(20))?;

        for event in in_events {
            _ = out_events.push(IntEvent::from(event))
        }

        Ok(out_events)
    }

    pub fn get_buffered_credits(&mut self) -> Result<hVec<f32, 5>, CctalkTransmissionError> {
        let mut credits: hVec<f32, 5> = hVec::new();

        let events = self.read_events(Some(20))?;

        for event in events {
            _ = credits.push(self.currancy.unwrap().slot_to_currancy(event.a));
        }

        Ok(credits)
    }

    pub fn action_escrow(&mut self, accept: bool) -> Result<(), CctalkTransmissionError> {
        let mut cmd_byte = 0x00;

        if accept {
            cmd_byte |= 0x01;
        }

        let msg = Msg8::new(
            self.info.addr,
            MASTER_ADDR,
            CcTalkHeader::RouteBill,
            hVec::from_array([cmd_byte]),
        );

        let res = self.transfer(msg, 20)?;
        let res =
            Msg8::try_from(res).map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        if !res.is_ack() {
            return Err(CctalkTransmissionError::CctalkFailedToAck(res.header()));
        };

        Ok(())
    }

    /*==============================================================================
     *
     *
     *
     *                          private helper fn's
     *
     *
     *
     *==============================================================================
     * */
    fn read_buff_events(
        &mut self,
        timeout_ms: Option<u32>,
    ) -> Result<[u8; BILL_EVENT_BUFF_LEN], CctalkTransmissionError> {
        // CcTalkHeader::ReadBufferedBillEvents returns a history of bill events:
        //
        // Received 11-byte data-packet (from Cctalk spec part 2):
        //
        // [ event counter ]
        // [ result 1A ] [ result 1B ]
        // [ result 2A ] [ result 2B ]
        // [ result 3A ] [ result 3B ]
        // [ result 4A ] [ result 4B ]
        // [ result 5A ] [ result 5B ]
        //
        // Can only extract last 5 events, so frequent polling is required so that more
        // than 5 events haven't occured since last poll.
        //

        //get response to event poll - if not an ack, return an error
        let res = self.header_only(
            self.info.addr,
            CcTalkHeader::ReadBufferedBillEvents,
            timeout_ms,
        )?;
        let header = res.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };

        //extract the 11-byte data-packet from response and return
        let dat = res.data()[0..BILL_EVENT_BUFF_LEN]
            .try_into()
            .expect("error buff events ret value not 11 bytes long");

        Ok(dat)
    }

    fn process_events(&mut self, data: [u8; BILL_EVENT_BUFF_LEN]) -> hVec<RawResult, 5> {
        let mut events = hVec::new();

        //store old events counter
        self.events.last.set(self.events.curr.get());

        //1st byte of packet is new event counter
        self.events.curr.set(data[0]);

        //diff between old and new is number of events we need from queue
        let mut num_events = self.events.curr.diff(&self.events.last);

        //if no events have occured since last poll, return an empty list
        if num_events == 0 {
            return hVec::new();
        }

        //if more than 5 events have occured since last poll, we have lost
        //some events and the maximum number we can extract from queue is 5,
        if num_events > 5 {
            // println!("missed {} events", num_events - 5);
            num_events = 5;
        }

        //iterate over the events in queue and extract and store the a and b data for each one
        for i in 0..num_events as usize {
            let a = data[i + 1];
            let b = data[i + 2];
            _ = events.push(RawResult { a, b });

            //we have one less event to process so increase last event counter
            self.events.last.increase();
        }

        events
    }

    fn read_events(
        &mut self,
        timeout_ms: Option<u32>,
    ) -> Result<hVec<RawResult, 5>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        //get the 11-byte event buffer
        let event_buf = self.read_buff_events(timeout_ms)?;

        //convert the 11-byte buffer into a len-5 (a,b) event queue
        let events = self.process_events(event_buf);

        //for debugging purposes
        #[cfg(feature = "std")]
        if !events.is_empty() {
            std::println!("{:?}", events);
        }
        Ok(events)
    }
}

#[derive(Debug)]
pub enum IntEvent {
    StackedNote(u8),
    NoteEscrow(u8),
    Barcode,
    BarcodeEscrow,
    DevError(EventError),
}

impl Default for IntEvent {
    fn default() -> Self {
        Self::DevError(EventError::UnknownError)
    }
}

/*
 *
 * Slightly complicated schema:
 *
 *
 * (a,b) => (0,20) Barcode Detected
 * (a,b) => (255, 1) Ticket held in escrow
 * (a,b) (1..=254, 0) Note Accepted (a is note slot number (need to look up in currency table))
 * (a,b) => (1..=254, 1) Note held in Escrow
 * (a,b) => (0, 1..=21) Error Codes Various
 *
 *
 * */

impl From<RawResult> for IntEvent {
    fn from(value: RawResult) -> Self {
        let a = value.a;
        let b = value.b;

        match (a, b) {
            //note slot 1->254, error slot: Escrow Hold
            (1..255, 1) => Self::NoteEscrow(a),
            //note slot 1->254, error slot: Note stacked/Accepted
            (1..255, 0) => Self::StackedNote(a),
            //barcode/ticket slot 255, error slot: Ticket stacked/Accepted
            (255, 0) => Self::Barcode,
            //barcodde/ticket slot: 255, error slot: Escrow Hold
            (255, 1) => Self::BarcodeEscrow,
            //anything else (a must be zero and b must be an error code)
            (_, _) => Self::DevError(EventError::from(b)),
        }
    }
}

#[derive(Default, Debug)]
struct RawResult {
    #[allow(unused)]
    a: u8,
    b: u8,
}

impl RawResult {
    #[allow(unused)]
    fn new(a: u8, b: u8) -> Self {
        Self { a, b }
    }

    #[allow(unused)]
    fn is_escrow(&self) -> bool {
        self.b == 1
    }
}

/* =====================================================================================
 *
 *
 *          8bit devices that have been probed, but not initialised
 *
 *
 * =====================================================================================
 */
impl<U, D> CctalkDevice<Unenc8Bit, UnInit, U, D>
where
    U: Read + Write,
    D: DelayNs,
{
    pub fn init(
        mut self,
        escrow: bool,
    ) -> Result<CctalkDevice<Unenc8Bit, Init, U, D>, CctalkTransmissionError> {
        /*
         *
         *      Get currency
         *
         * */
        self.currancy = Some(self.get_currency()?);
        // println!(
        //     "currency_sf: {:?}, currency_country: {}",
        //     self.currancy.unwrap().sf,
        //     String::from_utf8_lossy(&self.currancy.unwrap().cc)
        // );

        /*
         *
         *      Modify Bill Operating Mode
         *
         * */

        let mut cmd_byte = 0x00;

        if self.info.model == hString::<255>::try_from("NV9").unwrap() {
            cmd_byte |= 0b00000001;
        }
        if escrow {
            cmd_byte |= 1 << 1;
        }

        let msg = Msg8::new(
            self.info.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyBillOperatingMode,
            hVec::from_array([cmd_byte]),
        );

        let rx_bytes = self.transfer(msg, 80)?;
        let _msg =
            Msg8::try_from(rx_bytes).map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
        // println!("mod bill op mode: {}", msg.header());

        /*
         *
         *      Modify Inhibit Status for all (occupied) slots
         *
         * */

        let _header = self.inhibit_all_slots(false)?;
        // println!("uninhibited all slots, result: {}", header);

        /*
         *
         *      Modify Master Inhibit Status
         *
         * */

        let _ack = self.set_master_inhibit(false)?;
        // println!("mod master inhibit: {}", ack);

        /*
         *
         * Get/Set event counter
         *
         */

        let ec = self.get_event_counter()?;
        self.events.curr = ec;
        self.events.last = self.events.curr.clone();

        // println!("Device init at addr: {} complete!\n", self.info.addr);

        Ok(CctalkDevice {
            info: self.info,
            currancy: self.currancy,
            cctalk: self.cctalk,
            chksum: self.chksum,
            encrypted: self.encrypted,
            events: self.events,
            _enc_state: PhantomData,
            _init_state: PhantomData,
        })
    }

    pub fn set_master_inhibit(
        &mut self,
        value: bool,
    ) -> Result<CcTalkHeader, CctalkTransmissionError> {
        let inhibit = match value {
            true => 0x00,
            false => 0x01,
        };

        let payload: hVec<u8, 255> = hVec::from_array([inhibit]);
        let msg = Msg8::new(
            self.info.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyMasterInhibitStatus,
            payload,
        );

        let rx_bytes = self.transfer(msg, 80)?;
        let msg =
            Msg8::try_from(rx_bytes).map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        Ok(msg.header())
    }

    fn get_event_counter(&mut self) -> Result<EC, CctalkTransmissionError> {
        let payload = hVec::new();
        let msg = Msg8::new(
            self.info.addr,
            MASTER_ADDR,
            CcTalkHeader::ReadBufferedBillEvents,
            payload,
        );

        let rx_bytes = self.transfer(msg, 80)?;
        let msg =
            Msg8::try_from(rx_bytes).map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
        let data_bytes = msg.data();

        let ev_cnt = data_bytes[0];

        Ok(EC::new(ev_cnt))
    }

    fn inhibit_all_slots(&mut self, value: bool) -> Result<CcTalkHeader, CctalkTransmissionError> {
        let tx_array = if value { [0x00, 0x00] } else { [0xFF, 0xFF] };

        let payload: hVec<u8, 255> = hVec::from_array(tx_array);
        let msg = Msg8::new(
            self.info.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyInhibitStatus,
            payload,
        );

        let rx_bytes = self.transfer(msg, 80)?;
        let msg =
            Msg8::try_from(rx_bytes).map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        Ok(msg.header())
    }

    #[allow(unused)]
    fn uninhibit_slots(&mut self, slots: &[u8]) -> Result<CcTalkHeader, CctalkTransmissionError> {
        let mut tx_u16 = 0u16;

        for (i, _) in slots.iter().enumerate() {
            tx_u16 |= 1 << i;
        }

        let tx_array: [u8; 2] = [(tx_u16 >> 8) as u8, (tx_u16 & 0xFF) as u8];

        let payload: hVec<u8, 255> = hVec::from_array(tx_array);
        let msg = Msg8::new(
            self.info.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyInhibitStatus,
            payload,
        );

        let rx_bytes = self.transfer(msg, 80)?;
        let msg =
            Msg8::try_from(rx_bytes).map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        // println!("mod slots {:#b} uninhibited: {}", tx_u16, msg.header());
        Ok(msg.header())
    }
    fn get_currency(&mut self) -> Result<Currancy, CctalkTransmissionError> {
        let mut currancy = Currancy::default();
        let mut country_code: [u8; 2] = [0u8; 2];
        let mut slot_amt: [u8; 4] = [0u8; 4];

        /*
         *
         *      Request Slots and Country Code
         *
         * */
        for i in 1..=16 {
            let mut payload: hVec<u8, 255> = hVec::new();
            _ = payload.push(i);
            let msg = Msg8::new(
                self.info.addr,
                MASTER_ADDR,
                CcTalkHeader::RequestBillId,
                payload,
            );

            let rx_bytes = self.transfer(msg, 80)?;
            let msg = Msg8::try_from(rx_bytes)
                .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
            let dat = msg.data().clone();
            if *dat != [0, 0, 0, 0, 0, 0, 0] {
                for (j, _) in dat.iter().enumerate() {
                    if j < 2 {
                        country_code[j] = dat[j];
                    } else if j >= 2 && j < 6 {
                        slot_amt[j - 2] = dat[j];
                    }
                }

                let slot_str = unsafe {
                    hString::<255>::from_utf8_unchecked(hVec::from_slice(&slot_amt[..]).unwrap()) //ugly!
                };
                // println!("slot_str: {}", slot_str);
                let slot_amt: u16 = slot_str.parse().unwrap();
                // println!("slot_data: {:?}, slot_amt: £{}", dat, slot_amt);
                currancy.slots[i as usize] = slot_amt as u8;
                if currancy.cc == [0x00, 0x00] {
                    currancy.cc = country_code;
                }
            }
        }

        /*
         *
         *      Request Country Scaling Factor
         *
         * */

        let payload: hVec<u8, 255> = hVec::from_array(country_code);
        let msg = Msg8::new(
            self.info.addr,
            MASTER_ADDR,
            CcTalkHeader::RequestCountryScalingFactor,
            payload,
        );

        let rx_bytes = self.transfer(msg, 80)?;

        let msg =
            Msg8::try_from(rx_bytes).map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        for (i, dat) in msg.data().iter().enumerate() {
            currancy.sf[i] = *dat;
        }

        /*
         *
         *      Request Currency Revision
         *
         * */
        let payload: hVec<u8, 255> = hVec::new();
        let msg = Msg8::new(
            self.info.addr,
            MASTER_ADDR,
            CcTalkHeader::RequestCurrencyRevision,
            payload,
        );

        match self.transfer(msg, 80) {
            Ok(bytes) => {
                let msg = Msg8::try_from(bytes).ok();
                if let Some(msg) = msg {
                    for (i, dat) in msg.data().iter().enumerate() {
                        currancy.rev[i] = *dat;
                    }
                }
            }
            Err(_e) => {
                // println!("error curr revision: {}", e)
            }
        }

        Ok(currancy)
    }
}

/* =====================================================================================
 *
 *
 *              Any 8bit device, in any state of initialisation
 *
 *
 * =====================================================================================
 */
impl<I, U, D> CctalkDevice<Unenc8Bit, I, U, D>
where
    I: InitStatus,
    U: Read + Write,
    D: DelayNs,
{
    pub fn header_only(
        &mut self,
        addr: u8,
        header: CcTalkHeader,
        timeout_ms: Option<u32>,
    ) -> Result<Msg8, CctalkTransmissionError> {
        let msg = Msg8::new(addr, MASTER_ADDR, header, hVec::new());

        let rx_bytes = self.transfer(msg, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;

        let rx_msg =
            Msg8::try_from(rx_bytes).map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        if !rx_msg.is_ack() {
            return Err(CctalkTransmissionError::CctalkFailedToAck(rx_msg.header()));
        }
        Ok(rx_msg)
    }
}
