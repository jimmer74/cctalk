use core::marker::PhantomData;
use std::ops::Deref;

use super::interface::Cctalk;
use crate::{
    DEFAULT_TIMEOUT_MS, MASTER_ADDR,
    errors::CctalkTransmissionError,
    headers::CcTalkHeader::{self, RequestEquipmentCategory, RequestManufacturerId, SimplePoll},
    message::{Cctalk8BitChksumMessage, CctalkCRC16ChksumMessage},
};
use embedded_hal::delay::DelayNs;
use heapless::Vec as hVec;

use embedded_hal_nb::serial::{Read, Write};

const BILL_EVENT_BUFF_LEN: usize = 11;

#[derive(Debug, Clone, Default)]
pub struct CctalkDevice<EncType, InitType> {
    pub addr: u8,
    pub kind: CctalkDeviceKind,
    pub manu: String,
    pub model: String,
    #[allow(unused)]
    chksum: CctalkDeviceCRC,
    #[allow(unused)]
    encrypted: CctalkEncKey,
    #[allow(unused)]
    event_counter: EventCounter,
    last_event: EventCounter,
    _enc_state: PhantomData<EncType>,
    _init_state: PhantomData<InitType>,
}

#[derive(Default, Debug, Clone)]
pub enum CctalkDeviceCRC {
    Crc16xmodem,
    #[default]
    Simple8bit,
}

#[derive(Debug)]
pub struct NoChksum;

impl Default for NoChksum {
    fn default() -> Self {
        Self {}
    }
}
#[derive(Default, Debug)]
pub struct Unenc8Bit;
#[derive(Default, Debug)]
pub struct Unenc16Bit;
#[derive(Default, Debug)]
pub struct Bnv16Bit;
#[derive(Default, Debug)]
pub struct Des16Bit;

pub trait InitStatus {}
#[derive(Default)]
pub struct Unprobed;
#[derive(Default, Debug)]
pub struct UnInit;
#[derive(Default, Debug)]
pub struct Init;

impl InitStatus for Init {}
impl InitStatus for UnInit {}
impl InitStatus for Unprobed {}

#[derive(Default, Debug, Clone)]
struct EventCounter(u8);

#[allow(unused)]
impl EventCounter {
    fn increase(&mut self) {
        if self.0 == 255 {
            self.0 = 1;
        } else {
            self.0 += 1;
        }
    }
    fn set(&mut self, value: u8) {
        self.0 = value;
    }
    fn get(&self) -> u8 {
        self.0
    }
}

impl Deref for EventCounter {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl CctalkDevice<NoChksum, Unprobed> {
    pub fn new(addr: u8) -> CctalkDevice<NoChksum, Unprobed>
    where
        NoChksum: Default,
    {
        Self {
            addr,
            _enc_state: PhantomData,
            _init_state: PhantomData,
            ..Default::default()
        }
    }

    pub fn probe<UART, DELAY>(
        &self,
        cctalk: &mut Cctalk<UART, DELAY>,
    ) -> Result<CctalkDevice<Unenc8Bit, UnInit>, CctalkTransmissionError>
    where
        DELAY: DelayNs,
        UART: Read + Write,
    {
        let addr = self.addr;
        #[allow(unused_assignments)]
        let mut res = cctalk.header_only(self.addr, SimplePoll, None)?;

        //Device type
        res = cctalk.header_only(self.addr, RequestEquipmentCategory, None)?;

        let kind = CctalkDeviceKind::from(res.data().as_slice());

        if kind == CctalkDeviceKind::Unknown {
            return Err(CctalkTransmissionError::CctalkDeviceTypeUnknown);
        }

        //Encryption Key/Status
        let encrypted = CctalkEncKey::CctalkUnEncrypted; //cctalk.retrieve_enc_key(self.addr, Some(200))?;

        //Manufacturer
        res = cctalk.header_only(self.addr, RequestManufacturerId, None)?;
        let manu = String::from_utf8(res.data().to_vec()).unwrap();

        //Model
        res = cctalk.header_only(self.addr, CcTalkHeader::RequestProductCode, None)?;
        let model = String::from_utf8(res.data().to_vec()).unwrap();

        Ok(CctalkDevice {
            addr: addr,
            kind: kind,
            manu: manu,
            model: model,
            event_counter: EventCounter::default(),
            last_event: EventCounter::default(),
            chksum: CctalkDeviceCRC::Simple8bit,
            encrypted: encrypted,
            _enc_state: PhantomData,
            _init_state: PhantomData,
        })
    }

    pub fn probe16<UART, DELAY>(
        &mut self,
        cctalk: &mut Cctalk<UART, DELAY>,
    ) -> Result<CctalkDevice<Unenc16Bit, UnInit>, CctalkTransmissionError>
    where
        DELAY: DelayNs,
        UART: Read + Write,
    {
        let addr = self.addr;
        #[allow(unused_assignments)]
        let mut res = cctalk.header_only_16(addr, SimplePoll, None)?;

        //Device type
        res = cctalk.header_only_16(addr, RequestEquipmentCategory, None)?;

        let kind = CctalkDeviceKind::from(res.data().as_slice());

        if kind == CctalkDeviceKind::Unknown {
            return Err(CctalkTransmissionError::CctalkDeviceTypeUnknown);
        }

        //Encryption Key/Status
        let encrypted = cctalk.retrieve_enc_key(addr, Some(200))?;

        //Manufacturer
        res = cctalk.header_only_16(addr, RequestManufacturerId, None)?;
        let manu = String::from_utf8(res.data().to_vec()).unwrap();

        //Model
        res = cctalk.header_only_16(addr, CcTalkHeader::RequestProductCode, None)?;
        let model = String::from_utf8(res.data().to_vec()).unwrap();

        Ok(CctalkDevice {
            addr: addr,
            kind: kind,
            manu: manu,
            model: model,
            chksum: CctalkDeviceCRC::Simple8bit,
            encrypted: encrypted,
            _enc_state: PhantomData,
            _init_state: PhantomData,
            event_counter: EventCounter::default(),
            last_event: EventCounter::default(),
        })
    }
}

impl CctalkDevice<Unenc8Bit, Init> {
    pub fn read_buff_events<UART, DELAY>(
        &mut self,
        cctalk: &mut Cctalk<UART, DELAY>,
        timeout_ms: Option<u32>,
    ) -> Result<[u8; BILL_EVENT_BUFF_LEN], CctalkTransmissionError>
    where
        DELAY: DelayNs,
        UART: Read + Write,
    {
        let res = self.header_only(
            cctalk,
            self.addr,
            CcTalkHeader::ReadBufferedBillEvents,
            timeout_ms,
        )?;
        let header = res.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };

        let dat = res.data()[0..BILL_EVENT_BUFF_LEN]
            .try_into()
            .expect("error buff events ret value not 11 bytes long");

        Ok(dat)
    }
}

impl<I> CctalkDevice<Unenc8Bit, I>
where
    I: InitStatus,
{
    pub fn header_only<UART, DELAY>(
        &mut self,
        cctalk: &mut Cctalk<UART, DELAY>,
        addr: u8,
        header: CcTalkHeader,
        timeout_ms: Option<u32>,
    ) -> Result<Cctalk8BitChksumMessage, CctalkTransmissionError>
    where
        DELAY: DelayNs,
        UART: Read + Write,
    {
        let msg = Cctalk8BitChksumMessage::new(addr, MASTER_ADDR, header, hVec::new());
        let res = cctalk.transfer(msg, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;
        let header = res.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };
        Ok(res)
    }

    pub fn transfer<UART, DELAY>(
        &mut self,
        cctalk: &mut Cctalk<UART, DELAY>,
        msg: Cctalk8BitChksumMessage,
        timeout_ms: u32,
    ) -> Result<Cctalk8BitChksumMessage, CctalkTransmissionError>
    where
        DELAY: DelayNs,
        UART: Read + Write,
    {
        let msg_bytes = cctalk.write_msg(msg.clone())?;
        if cctalk.echo {
            match cctalk.read_msg_exact(timeout_ms, msg_bytes.len()) {
                Ok(rx_msg) => {
                    // println!("echo matches - discarding!");
                    if rx_msg != msg {
                        println!("echo does not match");
                        return Err(CctalkTransmissionError::FailedToReciveEcho);
                    }
                }
                Err(e) => {
                    println!("read error: {}", e);
                    return Err(CctalkTransmissionError::FailedToReciveEcho);
                }
            }
        }

        let msg = cctalk.read_msg(timeout_ms)?;

        let _chksum = msg
            .chksum_valid()
            .map_err(|_e| CctalkTransmissionError::RxDataMalformedChksum)?;

        Ok(msg)
    }

    pub fn init<UART, DELAY>(
        mut self,
        cctalk: &mut Cctalk<UART, DELAY>,
    ) -> Result<CctalkDevice<Unenc8Bit, Init>, CctalkTransmissionError>
    where
        DELAY: DelayNs,
        UART: Read + Write,
    {
        /*
         *
         *      Get all note slots
         *
         * */

        let mut country_code: [u8; 2] = [0x00, 0x00];
        for i in 1..=16 {
            let mut payload: hVec<u8, 255> = hVec::new();
            _ = payload.push(i);
            let msg = Cctalk8BitChksumMessage::new(
                self.addr,
                MASTER_ADDR,
                CcTalkHeader::RequestBillId,
                payload,
            );
            let msg = self.transfer(cctalk, msg, 80).ok();
            let dat = msg.clone().unwrap().data().clone();
            if *dat != [0, 0, 0, 0, 0, 0, 0] {
                println!(
                    "Note Slot {}: {:?}/{:?}",
                    i,
                    msg.clone().unwrap().data(),
                    msg.clone().unwrap().data_str()
                );

                for (j, _) in country_code.clone().iter().enumerate() {
                    if j < 2 {
                        country_code[j] = dat[j];
                    }
                }
            } else {
                println!("Note slot {} is unoccupied", i);
            }
        }
        println!(
            "Country code: {:?} / {}",
            country_code,
            String::from_utf8_lossy(&country_code[..])
        );

        /*
         *
         *      Request Country Scaling Factor
         *
         * */

        let payload: hVec<u8, 255> = hVec::from_array(country_code);
        let msg = Cctalk8BitChksumMessage::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::RequestCountryScalingFactor,
            payload,
        );

        let msg = self.transfer(cctalk, msg, 80).ok();
        let dat = msg.clone().unwrap().data().clone();

        println!("RCSF: {:?}", dat);

        /*
         *
         *      Request Currency Revision
         *
         * */
        let payload: hVec<u8, 255> = hVec::new(); //hVec::from_array(country_code);
        let msg = Cctalk8BitChksumMessage::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::RequestCurrencyRevision,
            payload,
        );

        match self.transfer(cctalk, msg, 80) {
            Ok(msg) => {
                println!("Currency Rev: {:?}", String::from_utf8_lossy(msg.data()));
            }
            Err(e) => {
                println!("error curr revision: {}", e)
            }
        }

        /*
         *
         *      Modify Bill Operating Mode
         *
         * */

        let payload: hVec<u8, 255> = if self.model == String::from("NV10") {
            hVec::from_array([0x00])
        } else if self.model == String::from("NV9") {
            hVec::from_array([0x01])
        } else {
            hVec::from_array([0x01])
        };
        let msg = Cctalk8BitChksumMessage::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyBillOperatingMode,
            payload,
        );
        let msg = self.transfer(cctalk, msg, 80).ok();
        println!("mod bill op mode: {}", msg.unwrap().header());

        /*
         *
         *      Modify Inhibit Status
         *
         * */
        let payload: hVec<u8, 255> = hVec::from_array([0xFF, 0xFF]);
        let msg = Cctalk8BitChksumMessage::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyInhibitStatus,
            payload,
        );

        let msg = self.transfer(cctalk, msg, 80).ok();
        println!("mod master inhibit: {}", msg.unwrap().header());

        /*
         *
         *      Modify Master Inhibit Status
         *
         * */

        let payload: hVec<u8, 255> = hVec::from_array([0x01]); //hVec::from_array(country_code);
        let msg = Cctalk8BitChksumMessage::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyMasterInhibitStatus,
            payload,
        );

        let msg = self.transfer(cctalk, msg, 80).ok();
        println!("mod master inhibit: {}", msg.unwrap().header());

        Ok(CctalkDevice {
            addr: self.addr,
            kind: self.kind,
            manu: self.manu,
            model: self.model,
            chksum: self.chksum,
            encrypted: self.encrypted,
            event_counter: self.event_counter,
            last_event: self.last_event,
            _enc_state: PhantomData,
            _init_state: PhantomData,
        })
    }
}

#[allow(unused)]
impl<I> CctalkDevice<Unenc16Bit, I>
where
    I: InitStatus,
{
    fn header_only<UART, DELAY /*, ENCRYPTION*/>(
        &mut self,
        cctalk: &mut Cctalk<UART, DELAY>,
        addr: u8,
        header: CcTalkHeader,
        timeout_ms: Option<u32>,
    ) -> Result<Cctalk8BitChksumMessage, CctalkTransmissionError>
    where
        DELAY: DelayNs,
        UART: Read + Write,
    {
        let msg = CctalkCRC16ChksumMessage::new(addr, header, hVec::new());
        let msg = Cctalk8BitChksumMessage::from(msg);
        let res = cctalk.transfer(msg, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;
        let header = res.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };
        Ok(res)
    }
}

#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub enum CctalkEncKey {
    #[default]
    CctalkUnEncrypted,
    CctalkDESKey(hVec<u8, 255>),
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
