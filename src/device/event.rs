use crate::errors::EventError;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
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

        //1st case if a is zero, then b is error code
        //TODO: rework match statement below to incorporate this fact
        if a == 0 {
            return Self::DevError(EventError::from(b));
        }

        match (a, b) {
            //note slot 1->254, error slot: Escrow Hold
            (1..255, 1) => Self::NoteEscrow(a),
            //note slot 1->254, error slot: Note stacked/Accepted
            (1..255, 0) => Self::StackedNote(a),
            //barcode/ticket slot 255, error slot: Ticket stacked/Accepted
            (255, 0) => Self::Barcode,
            //barcodde/ticket slot: 255, error slot: Escrow Hold
            (255, 1) => Self::BarcodeEscrow,
            //anything else (b must be an error code)
            (_, _) => Self::DevError(EventError::from(b)),
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
