use core::marker::PhantomData;
use std::ops::Deref;

use crate::{
    DEFAULT_TIMEOUT_MS, MASTER_ADDR,
    errors::CctalkTransmissionError,
    headers::CcTalkHeader::{self, RequestEquipmentCategory, RequestManufacturerId, SimplePoll},
    interface::SharedCctalk,
    message::{Msg8, Msg16},
};
use heapless::Vec as hVec;

use embedded_hal::delay::DelayNs;
use embedded_hal_nb::serial::{Read, Write};

const BILL_EVENT_BUFF_LEN: usize = 11;

#[derive(Debug)]
pub struct CctalkDevice<EncType, InitType, U, D> {
    pub addr: u8,
    pub kind: CctalkDeviceKind,
    pub manu: String,
    pub model: String,
    cctalk: SharedCctalk<U, D>,
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

//Encryption
pub trait EncStatus {}

pub struct NoChksum;
pub struct Unenc8Bit;
pub struct Unenc16Bit;
pub struct Bnv16Bit;
pub struct Des16Bit;

impl EncStatus for NoChksum {}
impl EncStatus for Unenc8Bit {}
impl EncStatus for Unenc16Bit {}
impl EncStatus for Bnv16Bit {}
impl EncStatus for Des16Bit {}

//Initialisation state
pub trait InitStatus {}

pub struct Unprobed;
pub struct UnInit;
pub struct Init;

impl InitStatus for Init {}
impl InitStatus for UnInit {}
impl InitStatus for Unprobed {}

// #[derive(Default, Debug, Clone)]
// struct EventCounter(u8);

//Device Type
pub struct CoinMech;
pub struct NoteAcc;
pub struct Hopper;
pub struct UnknownDev;

trait CctalkDevType {}
impl CctalkDevType for CoinMech {}
impl CctalkDevType for NoteAcc {}
impl CctalkDevType for Hopper {}
impl CctalkDevType for UnknownDev {}

//INFO: transfer fn's orginally had a write flush in
//them, which produced weird inconsistent results when
//reading the echo
//WARN: do not ever put a write flush back!!!!

impl<U, D> CctalkDevice<NoChksum, Unprobed, U, D> {
    pub fn new(addr: u8, cctalk: SharedCctalk<U, D>) -> CctalkDevice<NoChksum, Unprobed, U, D>
    where
        U: Read + Write,
        D: DelayNs,
    {
        Self {
            addr,
            cctalk: cctalk,
            _enc_state: PhantomData,
            _init_state: PhantomData,
            kind: Default::default(),
            manu: Default::default(),
            model: Default::default(),
            chksum: Default::default(),
            encrypted: Default::default(),
            event_counter: Default::default(),
            last_event: Default::default(),
        }
    }

    pub fn probe(mut self) -> Result<CctalkDevice<Unenc8Bit, UnInit, U, D>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let addr = self.addr;
        let mut res = self.header_only(self.addr, SimplePoll, None)?;
        _ = res;

        //Device type
        res = self.header_only(self.addr, RequestEquipmentCategory, None)?;

        let kind = CctalkDeviceKind::from(res.data().as_slice());

        if kind == CctalkDeviceKind::Unknown {
            return Err(CctalkTransmissionError::CctalkDeviceTypeUnknown);
        }

        //Encryption Key/Status
        let encrypted = CctalkEncKey::CctalkUnEncrypted; //cctalk.retrieve_enc_key(self.addr, Some(200))?;

        //Manufacturer
        res = self.header_only(self.addr, RequestManufacturerId, None)?;
        let manu = String::from_utf8(res.data().to_vec()).unwrap();

        //Model
        res = self.header_only(self.addr, CcTalkHeader::RequestProductCode, None)?;
        let model = String::from_utf8(res.data().to_vec()).unwrap();

        Ok(CctalkDevice {
            addr: addr,
            kind: kind,
            manu: manu,
            model: model,
            cctalk: self.cctalk,
            event_counter: EventCounter::default(),
            last_event: EventCounter::default(),
            chksum: CctalkDeviceCRC::Simple8bit,
            encrypted: encrypted,
            _enc_state: PhantomData,
            _init_state: PhantomData,
        })
    }

    pub fn header_only(
        &mut self,
        addr: u8,
        header: CcTalkHeader,
        timeout_ms: Option<u32>,
    ) -> Result<Msg8, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let msg = Msg8::new(addr, MASTER_ADDR, header, hVec::new());
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;
        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let header = msg.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };
        Ok(msg)
    }

    pub fn probe16(
        mut self,
    ) -> Result<CctalkDevice<Unenc16Bit, UnInit, U, D>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let addr = self.addr;
        #[allow(unused_assignments)]
        let mut res = self.header_only(addr, SimplePoll, None)?;

        //Device type
        res = self.header_only(addr, RequestEquipmentCategory, None)?;

        let kind = CctalkDeviceKind::from(res.data().as_slice());

        if kind == CctalkDeviceKind::Unknown {
            return Err(CctalkTransmissionError::CctalkDeviceTypeUnknown);
        }

        //Encryption Key/Status
        let encrypted = self.retrieve_enc_key(addr, Some(200))?;

        //Manufacturer
        res = self.header_only(addr, RequestManufacturerId, None)?;
        let manu = String::from_utf8(res.data().to_vec()).unwrap();

        //Model
        res = self.header_only(addr, CcTalkHeader::RequestProductCode, None)?;
        let model = String::from_utf8(res.data().to_vec()).unwrap();

        Ok(CctalkDevice {
            addr: addr,
            kind: kind,
            manu: manu,
            model: model,
            cctalk: self.cctalk,
            chksum: CctalkDeviceCRC::Simple8bit,
            encrypted: encrypted,
            _enc_state: PhantomData,
            _init_state: PhantomData,
            event_counter: EventCounter::default(),
            last_event: EventCounter::default(),
        })
    }

    fn retrieve_enc_key(
        &mut self,
        addr: u8,
        timeout_ms: Option<u32>,
    ) -> Result<CctalkEncKey, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let msg = Msg16::new(addr, CcTalkHeader::RequestEncryptionKey, hVec::new());

        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        match self.transfer(tx_bytes, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)) {
            //possibly encrypted (or encryption aware and set to [00,00,00])
            Ok(rx_bytes) => {
                let msg = Msg16::try_from_bytes(rx_bytes.as_slice()).ok();
                if let Some(res) = msg {
                    println!("encryption key: {:?}", res.data());

                    let key = res.data();
                    if key.is_empty() {
                        return Ok(CctalkEncKey::CctalkUnEncrypted);
                    } else {
                        return Ok(CctalkEncKey::CctalkDESKey(key.clone()));
                    }
                } else {
                    return Ok(CctalkEncKey::CctalkUnEncrypted);
                }
            }
            // Err(CctalkTransmissionError::CctalkMessageError(e)) => {
            //     println!("device doesn't support/predates encryption: {}", e);
            //     return Ok(CctalkEncKey::CctalkUnEncrypted);
            // }
            Err(e) => {
                println!("Encryption key error: {}", e);
                return Err(e);
            }
        }
    }
}
impl<E, I, U, D> CctalkDevice<E, I, U, D>
where
    E: EncStatus,
    I: InitStatus,
    U: Read + Write,
    D: DelayNs,
{
    pub fn transfer(
        &mut self,
        bytes: hVec<u8, 260>,
        timeout_ms: u32,
    ) -> Result<hVec<u8, 260>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let mut cctalk = self.cctalk.lock().unwrap();

        _ = cctalk.write_bytes(bytes.clone())?;

        if cctalk.echo {
            match cctalk.read_bytes_exact(timeout_ms, bytes.len()) {
                Ok(rx_bytes) => {
                    // println!("echo matches - discarding!");
                    if rx_bytes != bytes {
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

        let rx_bytes = cctalk.read_bytes(timeout_ms)?;

        Ok(rx_bytes)
    }
}
impl<U, D> CctalkDevice<Unenc8Bit, Init, U, D> {
    pub fn read_buff_events(
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
}

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

    pub fn init(
        mut self,
        // cctalk: &mut Cctalk<UART, DELAY>,
    ) -> Result<CctalkDevice<Unenc8Bit, Init, U, D>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
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
            let msg = Msg8::new(self.addr, MASTER_ADDR, CcTalkHeader::RequestBillId, payload);
            let tx_bytes = msg
                .try_to_bytes()
                .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

            let rx_bytes = self.transfer(tx_bytes, 80)?;
            let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
                .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
            let dat = msg.data().clone();
            if *dat != [0, 0, 0, 0, 0, 0, 0] {
                println!("Note Slot {}: {:?}/{:?}", i, msg.data(), msg.data_str());

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

        let dat = msg.data();

        println!("RCSF: {:?}", dat);

        /*
         *
         *      Request Currency Revision
         *
         * */
        let payload: hVec<u8, 255> = hVec::new(); //hVec::from_array(country_code);
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
                    println!("Currency Rev: {:?}", String::from_utf8_lossy(msg.data()));
                } else {
                    println!("Error Curr Rev failed to recieve: {:?}", bytes);
                }
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
        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
        println!("mod bill op mode: {}", msg.header());

        /*
         *
         *      Modify Inhibit Status
         *
         * */
        let payload: hVec<u8, 255> = hVec::from_array([0xFF, 0xFF]);
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

        println!("mod master inhibit: {}", msg.header());

        /*
         *
         *      Modify Master Inhibit Status
         *
         * */

        let payload: hVec<u8, 255> = hVec::from_array([0x01]); //hVec::from_array(country_code);
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
        println!("mod master inhibit: {}", msg.header());

        // drop(cctalk);

        Ok(CctalkDevice {
            addr: self.addr,
            kind: self.kind,
            manu: self.manu,
            model: self.model,
            cctalk: self.cctalk,
            chksum: self.chksum,
            encrypted: self.encrypted,
            event_counter: self.event_counter,
            last_event: self.last_event,
            _enc_state: PhantomData,
            _init_state: PhantomData,
        })
    }
}

impl<I, U, D> CctalkDevice<Unenc16Bit, I, U, D>
where
    I: InitStatus,
    U: Read + Write,
    D: DelayNs,
{
    #[allow(unused)]
    fn header_only(
        &mut self,
        addr: u8,
        header: CcTalkHeader,
        timeout_ms: Option<u32>,
    ) -> Result<Msg16, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let msg = Msg16::new(addr, header, hVec::new());
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;

        let msg = Msg16::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let header = msg.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };
        Ok(msg)
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
