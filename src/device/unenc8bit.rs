use crate::{
    device::{Currancy, EC, counter::EventCounter},
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
impl<U, D> CctalkDevice<Unenc8Bit, Init, U, D> {
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
    // '

    fn read_buff_events(
        &mut self,
        timeout_ms: Option<u32>,
    ) -> Result<[u8; BILL_EVENT_BUFF_LEN], CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let res = self.header_only(self.addr, CcTalkHeader::ReadBufferedBillEvents, timeout_ms)?;
        let header = res.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };

        let dat = res.data()[0..BILL_EVENT_BUFF_LEN]
            .try_into()
            .expect("error buff events ret value not 11 bytes long");

        Ok(dat)
    }

    fn process_events(&mut self, data: [u8; BILL_EVENT_BUFF_LEN]) -> hVec<EventResult, 5> {
        let mut events = hVec::new();

        self.last_event.set(self.event_counter.get());

        self.event_counter.set(data[0]);

        let mut num_events = self.event_counter.diff(&self.last_event);

        if num_events == 0 {
            return hVec::new();
        }

        if num_events > 5 {
            // let missed_events = num_events - 5;
            // println!("missed {} events", missed_events);
            num_events = 5;
        }

        for i in 0..num_events as usize {
            let a = data[i + 1];
            let b = data[i + 2];
            match EventResult::new(a, b) {
                Ok(ev) => {
                    _ = events.push(ev);
                }
                Err(_e) => {
                    // eprintln!("error: {}", e);
                }
            }
            self.last_event.increase();
        }

        events
    }

    pub fn read_events(
        &mut self,
        timeout_ms: Option<u32>,
    ) -> Result<hVec<EventResult, 5>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let event_buf = self.read_buff_events(timeout_ms)?;

        let events = self.process_events(event_buf);

        Ok(events)
    }

    pub fn get_buffered_credits(&mut self) -> Result<hVec<f32, 5>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let mut credits: hVec<f32, 5> = hVec::new();

        let events = self.read_events(Some(20))?;

        for event in events {
            _ = credits.push(self.currancy.unwrap().slot_to_currancy(event.a));
        }

        Ok(credits)
    }
}

#[derive(Default, Debug)]
pub struct EventResult {
    #[allow(unused)]
    a: u8,
    b: u8,
}

impl EventResult {
    fn new(a: u8, b: u8) -> Result<Self, EventError> {
        if a == 0 {
            return Err(EventError::from(b));
        } else {
            Ok(Self { a, b })
        }
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
    pub fn init(mut self) -> Result<CctalkDevice<Unenc8Bit, Init, U, D>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
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

        let payload: hVec<u8, 255> = if self.model == hString::<255>::try_from("NV10").unwrap() {
            hVec::from_array([0x00])
        } else if self.model == hString::<255>::try_from("NV9").unwrap() {
            hVec::from_array([0x01])
        } else {
            hVec::from_array([0x01])
        };
        let msg = Msg8::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyBillOperatingMode,
            payload,
        );
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, 80)?;
        let _msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
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
        self.event_counter = ec.clone();
        self.last_event = ec;

        // println!("Device init at addr: {} complete!\n", self.addr);

        Ok(CctalkDevice {
            addr: self.addr,
            kind: self.kind,
            manu: self.manu,
            model: self.model,
            currancy: self.currancy,
            cctalk: self.cctalk,
            chksum: self.chksum,
            encrypted: self.encrypted,
            event_counter: self.event_counter,
            last_event: self.last_event,
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
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyMasterInhibitStatus,
            payload,
        );
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, 80)?;
        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        Ok(msg.header())
    }

    fn get_event_counter(&mut self) -> Result<EC, CctalkTransmissionError> {
        let payload = hVec::new();
        let msg = Msg8::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ReadBufferedBillEvents,
            payload,
        );
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, 80)?;
        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
        let data_bytes = msg.data();

        let ev_cnt = data_bytes[0];

        Ok(EventCounter::new(ev_cnt))
    }

    fn inhibit_all_slots(&mut self, value: bool) -> Result<CcTalkHeader, CctalkTransmissionError> {
        let tx_array = if value { [0x00, 0x00] } else { [0xFF, 0xFF] };

        let payload: hVec<u8, 255> = hVec::from_array(tx_array);
        let msg = Msg8::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyInhibitStatus,
            payload,
        );

        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, 80)?;
        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

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
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyInhibitStatus,
            payload,
        );

        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, 80)?;
        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

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
            let msg = Msg8::new(self.addr, MASTER_ADDR, CcTalkHeader::RequestBillId, payload);
            let tx_bytes = msg
                .try_to_bytes()
                .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

            let rx_bytes = self.transfer(tx_bytes, 80)?;
            let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
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
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::RequestCountryScalingFactor,
            payload,
        );
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, 80)?;

        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

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
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::RequestCurrencyRevision,
            payload,
        );

        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        match self.transfer(tx_bytes, 80) {
            Ok(bytes) => {
                let msg = Msg8::try_from_bytes(bytes.as_slice()).ok();
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
    ) -> Result<Msg8, CctalkTransmissionError>
    where
        I: InitStatus,
    {
        let msg = Msg8::new(addr, MASTER_ADDR, header, hVec::new());
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;

        let rx_msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
        let header = rx_msg.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };
        Ok(rx_msg)
    }
}
