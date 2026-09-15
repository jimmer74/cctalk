use super::interface::Cctalk;
use crate::{
    errors::CctalkTransmissionError,
    headers::CcTalkHeader::{self, RequestEquipmentCategory, RequestManufacturerId, SimplePoll},
    message::CctalkMessage,
};
use embedded_hal::delay::DelayNs;
use heapless::Vec as hVec;

use embedded_hal_nb::serial::{Read, Write};
#[derive(Debug, Default)]
pub struct CctalkDevice {
    addr: u8,
    kind: Option<CctalkDeviceKind>,
    manu: String,
    model: String,
}

impl CctalkDevice {
    pub fn new(addr: u8) -> CctalkDevice {
        Self {
            addr,
            ..Default::default()
        }
    }

    pub fn probe<UART, DELAY>(
        &mut self,
        cctalk: &mut Cctalk<UART, DELAY>,
    ) -> Result<(), CctalkTransmissionError>
    where
        DELAY: DelayNs,
        UART: Read + Write,
    {
        //Simple Poll
        let mut res = cctalk.header_only(self.addr, SimplePoll, None)?;
        _ = res;

        //Device type
        //TODO: Bail early if info not avail
        res = cctalk.header_only(self.addr, RequestEquipmentCategory, None)?;

        self.kind = match CctalkDeviceKind::from(res.data().as_slice()) {
            CctalkDeviceKind::Unknown => None,
            k => Some(k),
        };

        if self.kind.is_none() {
            return Err(CctalkTransmissionError::CctalkDeviceTypeUnknown);
        }

        //Manufacturer
        res = cctalk.header_only(self.addr, RequestManufacturerId, None)?;
        self.manu = String::from_utf8(res.data().to_vec()).unwrap();

        //Model
        res = cctalk.header_only(self.addr, CcTalkHeader::RequestProductCode, None)?;
        self.model = String::from_utf8(res.data().to_vec()).unwrap();

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum CctalkDeviceKind {
    Coinmech,      // 002, 011-017
    Hopper,        // 003-010
    NoteAcceptor,  //040-0470
    TicketPrinter, //110
    Unknown,       //all others
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

impl From<&[u8]> for CctalkDeviceKind {
    fn from(value: &[u8]) -> Self {
        match value {
            [66, 105, 108, 108, 32, 65, 99, 99, 101, 112, 116, 111, 114] => Self::NoteAcceptor,
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
    use super::*;
    use crate::message::CctalkMessage;
    use embedded_hal::delay::DelayNs;
    use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};
    use heapless::Vec as hVec;
    use nb::Error::WouldBlock;

    #[derive(Debug)]
    struct MockDelay {
        pub total_ms_delayed: u32,
    }
    impl DelayNs for MockDelay {
        fn delay_ns(&mut self, ns: u32) {
            self.total_ms_delayed += ns / 1_000_000;
        }
        fn delay_ms(&mut self, ms: u32) {
            self.total_ms_delayed += ms;
        }
    }

    #[test]
    fn test_cctalkdevicekind_from_u8() {
        let input: [u8; 5] = [110, 018, 002, 042, 007];
        let expected: hVec<CctalkDeviceKind, 5> = hVec::from_array([
            self::CctalkDeviceKind::TicketPrinter,
            self::CctalkDeviceKind::Unknown,
            self::CctalkDeviceKind::Coinmech,
            self::CctalkDeviceKind::NoteAcceptor,
            self::CctalkDeviceKind::Hopper,
        ]);
        let mut result: hVec<CctalkDeviceKind, 5> = hVec::new();

        for addr in input {
            _ = result.push(CctalkDeviceKind::from(addr));
        }

        assert_eq!(result, expected);
    }
}
// Coin Acceptor 2 11 to 17 a.k.a Coin Validator
// Payout 3 4 to 10 a.k.a Hopper
// Reel 30 31 to 34
// Bill Validator 40 41 to 47 a.k.a Note Acceptor
// Card Reader 50
// Changer 55 Money-in, money-out recyclers. Also
// used for coin singulators and sorters.
// Display 60 e.g. LCD panels,
// alpha-numeric displays
// Keypad 70 Remote keyboard
// Dongle 80 85 to 89 Security device, interface box or
// interface hub
// Meter 90 Electro-mechanical counter
// replacement
// Bootloader 99 Bootloader firmware and diagnostics
// when no application code is loaded.
// Power 100 Power switching hub or intelligent
// power supply
// Printer 110 Ticket printer for coupons and
// barcodes
// RNG 120 Random Number Generator
// Hopper Scale 130 Hopper with weigh scale
// Coin Feeder 140 Motorised coin feeder or singulator
// Debug 240 241 to 255 This address range may be used when
// developing new peripherals
