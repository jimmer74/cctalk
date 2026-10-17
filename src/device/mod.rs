use crate::{
    device::{
        currency::Currancy,
        event::{IntEvent, RawResult},
    },
    message::MessageType,
};
use core::marker::PhantomData;
mod counter;
pub mod currency;
pub mod event;
pub mod uninit;
type EncKey = CcTalkEncryptionKeys;
// type Chksum = CctalkEncStatus;
type DevKind = CctalkDeviceKind;
use crate::{
    DEFAULT_TIMEOUT_MS, MASTER_ADDR,
    device::counter::{EC, ECs},
    errors::CctalkTransmissionError,
    headers::CcTalkHeader,
    interface::SharedCctalk,
    message::{Cctalk8BitChksumMessage, CctalkCRC16ChksumMessage, CctalkMessage},
};
use embedded_hal::delay::DelayNs;
use embedded_hal_nb::serial::{Read, Write};
use heapless::String as hString;
use heapless::Vec as hVec;

const BILL_EVENT_BUFF_LEN: usize = 11;
const REQ_ENC_SUPPORT_BYTES: [u8; 6] = [170, 85, 0, 0, 85, 170];

#[derive(Debug, Default, Clone)]
pub struct DevInfo {
    pub addr: u8,
    pub kind: DevKind,
    pub manu: hString<255>,
    pub model: hString<255>,
}

#[derive(Debug)]
pub struct CctalkDevice<I, U, D> {
    pub info: DevInfo,
    cctalk: SharedCctalk<U, D>,
    currancy: Option<Currancy>,
    r#type: CctalkEncStatus,
    encrypted: Option<EncKey>,
    events: ECs,

    _init_state: PhantomData<I>,
}

#[derive(Default, Debug, Clone)]
pub enum CctalkDeviceCRC {
    Crc16xmodem,
    #[default]
    Simple8bit,
}

//Initialisation state
pub trait InitStatus {}

#[derive(Default, Debug)]
pub struct Unprobed;
#[derive(Default, Debug)]
pub struct UnInit;
#[derive(Default, Debug)]
pub struct Init;

impl InitStatus for Init {}
impl InitStatus for UnInit {}
impl InitStatus for Unprobed {}

//INFO: transfer fn's orginally had a write flush in
//them, which produced weird inconsistent results when
//reading the echo
//WARN: do not ever put a write flush back!!!!

/* =====================================================================================
 *
 *
 *          devices that have been probed, but not initialised
 *
 *
 * =====================================================================================
 */
impl<U, D> CctalkDevice<UnInit, U, D>
where
    U: Read + Write,
    D: DelayNs,
{
    pub fn init(
        mut self,
        escrow: bool,
    ) -> Result<CctalkDevice<Init, U, D>, CctalkTransmissionError> {
        /*
         *
         *      Get currency
         *
         * */
        // #[cfg(feature = "std")]
        // std::println!("fetching currency");
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

        let msg = CctalkMessage::new(
            self.r#type,
            Some(MASTER_ADDR),
            self.info.addr,
            CcTalkHeader::ModifyBillOperatingMode,
            hVec::from_array([cmd_byte]),
        );

        // #[cfg(feature = "std")]
        // std::println!("setting bill operating mode");
        let rx_bytes = self.transfer(msg, 80)?;
        let _msg = CctalkMessage::from_bytes(self.r#type, rx_bytes)?;
        // println!("mod bill op mode: {}", msg.header());

        /*
         *
         *      Modify Inhibit Status for all (occupied) slots
         *
         * */

        // #[cfg(feature = "std")]
        // std::println!("uninibiting all slots");
        let _header = self.inhibit_all_slots(false)?;
        // println!("uninhibited all slots, result: {}", header);

        /*
         *
         *      Modify Master Inhibit Status
         *
         * */

        // #[cfg(feature = "std")]
        // std::println!("Unsetting master inhibit");
        let _ack = self.set_master_inhibit(false)?;
        // println!("mod master inhibit: {}", ack);

        /*
         *
         * Get/Set event counter
         *
         */

        // #[cfg(feature = "std")]
        // std::println!("Getting/setting event counter");
        let ec = self.get_event_counter()?;
        self.events.curr = ec;
        self.events.last = self.events.curr.clone();

        // #[cfg(feature = "std")]
        // std::println!("Device init at addr: {} complete!\n", self.info.addr);

        Ok(CctalkDevice {
            info: self.info,
            currancy: self.currancy,
            cctalk: self.cctalk,
            r#type: self.r#type,
            encrypted: self.encrypted,
            events: self.events,
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

        let src = if self.r#type == CctalkEncStatus::Unenc8Bit {
            Some(MASTER_ADDR)
        } else {
            None
        };
        let msg = CctalkMessage::new(
            self.r#type,
            src,
            self.info.addr,
            CcTalkHeader::ModifyMasterInhibitStatus,
            payload,
        );

        let rx_bytes = self.transfer(msg, 80)?;

        match self.r#type {
            CctalkEncStatus::Unenc16bit => {
                let msg = CctalkCRC16ChksumMessage::try_from(rx_bytes)
                    .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
                Ok(msg.header())
            }
            _ => {
                let msg = Cctalk8BitChksumMessage::try_from(rx_bytes)
                    .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
                Ok(msg.header())
            }
        }
    }
    fn get_own_addr(&self) -> Option<u8> {
        if self.r#type == CctalkEncStatus::Unenc8Bit {
            Some(MASTER_ADDR)
        } else {
            None
        }
    }
    fn get_event_counter(&mut self) -> Result<EC, CctalkTransmissionError> {
        let payload = hVec::new();
        let own_addr = self.get_own_addr();
        let msg = CctalkMessage::new(
            self.r#type,
            own_addr,
            self.info.addr,
            CcTalkHeader::ReadBufferedBillEvents,
            payload,
        );

        let rx_bytes = self.transfer(msg, 80)?;
        let msg = CctalkMessage::from_bytes(self.r#type, rx_bytes)?;
        let data_bytes = msg.data();

        let ev_cnt = data_bytes[0];

        Ok(EC::new(ev_cnt))
    }

    fn inhibit_all_slots(&mut self, value: bool) -> Result<CcTalkHeader, CctalkTransmissionError> {
        let tx_array = if value { [0x00, 0x00] } else { [0xFF, 0xFF] };

        let payload: hVec<u8, 255> = hVec::from_array(tx_array);
        let msg = CctalkMessage::new(
            self.r#type,
            Some(MASTER_ADDR),
            self.info.addr,
            CcTalkHeader::ModifyInhibitStatus,
            payload,
        );

        let rx_bytes = self.transfer(msg, 80)?;
        let msg = CctalkMessage::from_bytes(self.r#type, rx_bytes)?;

        Ok(msg.header())
    }

    //WARN: This doesn't work for initialisation. Note acceptor (NV9/10) won't go into
    //acceptance mode unless all slots are uninhibited - whether used or not!

    #[allow(unused)]
    fn uninhibit_slots(&mut self, slots: &[u8]) -> Result<CcTalkHeader, CctalkTransmissionError> {
        let mut tx_u16 = 0u16;

        for (i, _) in slots.iter().enumerate() {
            tx_u16 |= 1 << i;
        }

        let tx_array: [u8; 2] = [(tx_u16 >> 8) as u8, (tx_u16 & 0xFF) as u8];

        let payload: hVec<u8, 255> = hVec::from_array(tx_array);
        let msg = CctalkMessage::new(
            self.r#type,
            Some(MASTER_ADDR),
            self.info.addr,
            CcTalkHeader::ModifyInhibitStatus,
            payload,
        );

        let rx_bytes = self.transfer(msg, 80)?;
        let msg = CctalkMessage::from_bytes(self.r#type, rx_bytes)?;

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

            let msg = CctalkMessage::new(
                self.r#type,
                Some(MASTER_ADDR),
                self.info.addr,
                CcTalkHeader::RequestBillId,
                payload,
            );

            let rx_bytes = self.transfer(msg, 80)?;
            let msg = CctalkMessage::from_bytes(self.r#type, rx_bytes)?;
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
        let msg = CctalkMessage::new(
            self.r#type,
            Some(MASTER_ADDR),
            self.info.addr,
            CcTalkHeader::RequestCountryScalingFactor,
            payload,
        );

        let rx_bytes = self.transfer(msg, 80)?;

        let msg = CctalkMessage::from_bytes(self.r#type, rx_bytes)?;

        for (i, dat) in msg.data().iter().enumerate() {
            currancy.sf[i] = *dat;
        }

        /*
         *
         *      Request Currency Revision
         *
         * */
        let payload: hVec<u8, 255> = hVec::new();
        let msg = CctalkMessage::new(
            self.r#type,
            Some(MASTER_ADDR),
            self.info.addr,
            CcTalkHeader::RequestCurrencyRevision,
            payload,
        );

        match self.transfer(msg, 80) {
            Ok(bytes) => {
                let msg = CctalkMessage::from_bytes(self.r#type, bytes).ok();
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
 *                  Only for initialised devices
 *
 *
 * =====================================================================================
 * */
impl<U, D> CctalkDevice<Init, U, D>
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

        let msg = CctalkMessage::new(
            self.r#type,
            Some(MASTER_ADDR),
            self.info.addr,
            CcTalkHeader::RouteBill,
            hVec::from_array([cmd_byte]),
        );

        let res = self.transfer(msg, 20)?;
        let res = CctalkMessage::from_bytes(self.r#type, res)?;

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
        //extract the 11-byte data-packet from response and return

        let res = self.header_only(CcTalkHeader::ReadBufferedBillEvents, timeout_ms)?;
        let dat = res.data().as_slice()[0..BILL_EVENT_BUFF_LEN]
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
        for i in 1..=num_events as usize {
            let a = data[i];
            let b = data[i + 1];
            _ = events.push(RawResult { a, b });

            //we have one less event to process so increase last event counter
            self.events.last.increase();
        }
        #[cfg(feature = "std")]
        std::println!("events: {:?}", events);
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
        // #[cfg(feature = "std")]
        // if !events.is_empty() {
        //     std::println!("{:?}", events);
        // }
        Ok(events)
    }
}
impl<I, U, D> CctalkDevice<I, U, D>
where
    I: InitStatus,
    U: Read + Write,
    D: DelayNs,
{
    pub fn transfer<T: Into<hVec<u8, 260>>>(
        &mut self,
        bytes: T, // hVec<u8, 260>,
        timeout_ms: u32,
    ) -> Result<hVec<u8, 260>, CctalkTransmissionError> {
        let bytes = bytes.into();
        let mut cctalk = self.cctalk.lock();

        _ = cctalk.write_bytes(bytes.clone())?;

        if cctalk.echo {
            match cctalk.read_bytes_exact(timeout_ms, bytes.len()) {
                Ok(rx_bytes) => {
                    if rx_bytes != bytes {
                        // println!("echo does not match");
                        return Err(CctalkTransmissionError::FailedToReciveEcho);
                    }
                }
                Err(_e) => {
                    // println!("read error: {}", e);
                    return Err(CctalkTransmissionError::FailedToReciveEcho);
                }
            }
        }

        let rx_bytes = cctalk.read_bytes(timeout_ms)?;

        Ok(rx_bytes)
    }

    pub fn header_only(
        &mut self,
        header: CcTalkHeader,
        timeout_ms: Option<u32>,
    ) -> Result<CctalkMessage, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let msg = CctalkMessage::new(
            self.r#type,
            Some(MASTER_ADDR),
            self.info.addr,
            header,
            hVec::new(),
        );

        let rx_bytes = self.transfer(msg, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;
        let msg = CctalkMessage::from_bytes(self.r#type, rx_bytes)?;

        if msg.header() != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };

        Ok(msg)
    }
}
#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum CctalkEncStatus {
    #[default]
    UnInit,
    Unenc8Bit,
    Unenc16bit,
    BNV,
    DES, // CctalkEncSupport(CcTalkEncryptionStatus),
}
#[allow(unused)]
#[derive(Default, Debug, Clone)]
pub struct CcTalkEncryptionKeys {
    proto_level: u8,
    command_level: u8,
    proto_key_size: u8,
    com_key_size: u8,
    com_block_size: u8,
    trusted_mode: u8,
    bnv214365: [u8; 3],
    des: [u8; 8],
}

#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub enum CctalkDeviceKind {
    Coinmech,      // 002, 011-017
    Hopper,        // 003-010
    NoteAcceptor,  //040-0470
    TicketPrinter, //110
    #[default]
    Unknown, //all others
}

impl core::fmt::Display for CctalkDeviceKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self {
            CctalkDeviceKind::Coinmech => "Coinmech",
            CctalkDeviceKind::Hopper => "Hopper",
            CctalkDeviceKind::NoteAcceptor => "Note Acceptor",
            CctalkDeviceKind::TicketPrinter => "Ticket Printer",
            CctalkDeviceKind::Unknown => "Unknown/Unimplemented Device",
        };

        write!(f, "{}", msg)
    }
}

#[rustfmt::skip]
impl From<&[u8]> for CctalkDeviceKind {
    fn from(value: &[u8]) -> Self {
        match value {
            [66,105,108,108,32,86,97,108,105,100,97,116,111,114] => Self::NoteAcceptor,
            [67, 111, 105, 110, 32, 65, 99, 99, 101, 112, 116, 111, 114] => Self::Coinmech,
            [80, 97, 121, 111, 117, 116] => Self::Hopper,
            [80, 114, 105, 110, 116, 101, 114] => Self::TicketPrinter,
            _ => Self::Unknown,
        }
    }
}

impl From<u8> for CctalkDeviceKind {
    fn from(value: u8) -> Self {
        match value {
            002 | 011..=017 => Self::Coinmech,
            003..=010 => Self::Hopper,
            040..=047 => Self::NoteAcceptor,
            110 => Self::TicketPrinter,
            _ => Self::Unknown,
        }
    }
}

mod tests {

    #[test]
    fn test_cctalkdevicekind_from_u8() {
        use super::CctalkDeviceKind;
        use heapless::Vec as hVec;
        let input: [u8; 5] = [110, 018, 002, 042, 007];
        let expected: hVec<CctalkDeviceKind, 5> = hVec::from_array([
            CctalkDeviceKind::TicketPrinter,
            CctalkDeviceKind::Unknown,
            CctalkDeviceKind::Coinmech,
            CctalkDeviceKind::NoteAcceptor,
            CctalkDeviceKind::Hopper,
        ]);
        let mut result: hVec<CctalkDeviceKind, 5> = hVec::new();

        for addr in input {
            _ = result.push(CctalkDeviceKind::from(addr));
        }

        assert_eq!(result, expected);
    }
}

//INFO:
// Cctalk Device Address Ranges (from spec):
//
// Coin Acceptor 2, 11 to 17 a.k.a Coin Validator
// Payout 3, 4 to 10 a.k.a Hopper
// Reel 30, 31 to 34
// Bill Validator 40, 41 to 47 a.k.a Note Acceptor
// Card Reader 50
// Changer 55 Money-in, money-out recyclers. Also used for coin singulators and sorters.
// Display 60 e.g. LCD panels, alpha-numeric displays
// Keypad 70 Remote keyboard
// Dongle 80, 85 to 89 Security device, interface box or interface hub
// Meter 90 Electro-mechanical counter replacement
// Bootloader 99 Bootloader firmware and diagnostics when no application code is loaded.
// Power 100 Power switching hub or intelligent power supply
// Printer 110 Ticket printer for coupons and barcodes
// RNG 120 Random Number Generator
// Hopper Scale 130 Hopper with weigh scale
// Coin Feeder 140 Motorised coin feeder or singulator
// Debug 240, 241 to 255 This address range may be used when developing new peripherals
