use core::marker::PhantomData;

mod counter;
pub mod unenc16bit;
pub mod unenc8bit;
pub mod uninit;

type EncKey = CctalkEncKey;
type Chksum = CctalkDeviceCRC;
type EC = EventCounter;
type DevKind = CctalkDeviceKind;
use crate::{
    DEFAULT_TIMEOUT_MS, MASTER_ADDR,
    device::counter::EventCounter,
    errors::CctalkTransmissionError,
    headers::CcTalkHeader::{self, RequestEquipmentCategory, RequestManufacturerId, SimplePoll},
    interface::SharedCctalk,
    message::{Msg8, Msg16},
};
use heapless::Vec as hVec;

use embedded_hal::delay::DelayNs;
use embedded_hal_nb::serial::{Read, Write};

const BILL_EVENT_BUFF_LEN: usize = 11;
const REQ_ENC_SUPPORT_BYTES: [u8; 6] = [170, 85, 0, 0, 85, 170];

#[derive(Debug)]
pub struct CctalkDevice<E, I, U, D> {
    pub addr: u8,
    pub kind: DevKind,
    pub manu: String,
    pub model: String,
    currancy: Option<Currancy>,
    cctalk: SharedCctalk<U, D>,
    chksum: Chksum,
    encrypted: EncKey,
    event_counter: EC,
    last_event: EC,
    _enc_state: PhantomData<E>,
    _init_state: PhantomData<I>,
}

//TODO:
// #[derive(Debug, Clone, Default)]
// struct Dev {
// kind: DevKind,
// manufacturer: CctalkManu,
// model: String
//
// }

#[derive(Debug, Clone, Copy)]
struct Currancy {
    slots: CurrancySlots,
    sf: ScalingFactor,
    cc: [u8; 2],
    rev: [u8; 256],
}

impl Default for Currancy {
    fn default() -> Self {
        Self {
            slots: CurrancySlots::default(),
            sf: ScalingFactor::default(),
            cc: [0u8; 2],
            rev: [0u8; 256],
        }
    }
}

#[allow(unused)]
impl Currancy {
    pub fn slot_to_currancy(&self, slot: u8) -> f32 {
        (self.slots[slot as usize] as f32) * (self.sf[0] as f32) * (2 ^ self.sf[1]) as f32
            / (self.sf[2] as f32)
    }

    fn is_occupied(&self, index: usize) -> bool {
        self.slots[index] != 0
    }

    fn num_slots(&self) -> usize {
        let num_slots = self.slots.iter().filter(|f| *f != &0u8).count();
        num_slots
    }
}

type CurrancySlots = [u8; 16];
type ScalingFactor = [u8; 3];

#[derive(Default, Debug, Clone)]
pub enum CctalkDeviceCRC {
    Crc16xmodem,
    #[default]
    Simple8bit,
}

//Encryption
pub trait EncStatus {}

#[derive(Default, Debug)]
pub struct NoChksum;
#[derive(Default, Debug)]
pub struct Unenc8Bit;
#[derive(Default, Debug)]
pub struct Unenc16Bit;
#[derive(Default, Debug)]
pub struct Bnv16Bit;
#[derive(Default, Debug)]
pub struct Des16Bit;

impl EncStatus for NoChksum {}
impl EncStatus for Unenc8Bit {}
impl EncStatus for Unenc16Bit {}
impl EncStatus for Bnv16Bit {}
impl EncStatus for Des16Bit {}

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

#[derive(Default, Debug, Clone)]
pub enum CctalkEncKey {
    #[default]
    CctalkUnEncrypted,
    CctalkEncSupport(CcTalkEncryptionStatus),
}

#[allow(unused)]
#[derive(Default, Debug, Clone)]
pub struct CcTalkEncryptionStatus {
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
