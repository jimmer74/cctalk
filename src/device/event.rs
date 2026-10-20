use crate::{device::CctalkDeviceKind, errors::EventError};

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum IntEvent {
    StackedNote { slot: u8 },
    AcceptedCoin { slot: u8, sorter_path: Option<u8> },
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
 * (a,b) => (1..=254, 1..=8) Coin Diverted to Sorter Path  NOTE: Overlaps with Note in escrow!
 * (a,b) => (0, 1..=21) Error Codes Various
 *
 *
 * */

impl IntEvent {
    pub fn new(value: RawResult, kind: &CctalkDeviceKind) -> Self {
        let a = value.a;
        let b = value.b;

        //1st case if a is zero, then b is error code
        //TODO: rework match statement below to incorporate this fact
        if a == 0 {
            return Self::DevError(EventError::from(b));
        }

        if *kind == CctalkDeviceKind::NoteAcceptor {
            match (a, b) {
                //note slot 1->254, error slot: Escrow Hold
                (1..255, 1) => Self::NoteEscrow(a),
                //note slot 1->254, error slot: Note stacked/Accepted
                (1..255, 0) => Self::StackedNote { slot: a },
                //barcode/ticket slot 255, error slot: Ticket stacked/Accepted
                (255, 0) => Self::Barcode,
                //barcodde/ticket slot: 255, error slot: Escrow Hold
                (255, 1) => Self::BarcodeEscrow,
                //anything else (b must be an error code)
                (_, _) => Self::DevError(EventError::from(b)),
            }
        } else {
            //WARN: need this as not smart enough to clock
            //my (_,_) in last arm! Do not remove!
            #[allow(non_contiguous_range_endpoints)]
            match (a, b) {
                //note slot 1->254, error slot: Escrow Hold
                (1..255, 1..=8) => Self::AcceptedCoin {
                    slot: a,
                    sorter_path: Some(b),
                },
                (1..255, 0) => Self::AcceptedCoin {
                    slot: a,
                    sorter_path: None,
                },
                //anything else (b must be an error code)
                (_, _) => Self::DevError(EventError::from(b)),
            }
        }
    }
}

#[derive(Default, Debug)]
pub struct RawResult {
    pub a: u8,
    pub b: u8,
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
