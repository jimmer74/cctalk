use core::fmt::Debug;
use thiserror::Error;

use crate::headers::CcTalkHeader;

#[derive(Error, Debug, PartialEq, PartialOrd)]
pub enum CctalkMessageError {
    #[error("Wrong data length: {0}, should be: {1}")]
    IncorrectDataLen(u8, u8), //incorrect current length, calced data length
    #[error("Wrong chksum: {0}, should be: {1}")]
    IncorrectChksum(u8, u8), //current checksum, correct checksum
    #[error("Wrong 16-bit chksum: {0}, should be: {1}")]
    IncorrectChksum16(u16, u16),
    #[error("CCtalk packet too short: {0} bytes (min valid length: 5 bytes)")]
    MessageTooShort(usize),
    #[error("Heapless vec is full, cannot push byte: {0:#02X}")] // current message length
    HVecFailedToPush(u8), // hVec is full!
    #[error("Chksum not set!")] //
    NoChkSum,
}

#[derive(Error, Debug, PartialEq, PartialOrd)]
pub enum CctalkTransmissionError {
    #[error("failed to receive tx echo")]
    FailedToReciveEcho,
    #[error("Rx data err")]
    RxDataMalformedLength,
    #[error("Rx data wrong chksum")]
    RxDataMalformedChksum,
    #[error("Rx data worng address")]
    RxDataWrongAddress,
    #[error("unknown error occured")]
    UnknownError,
    #[error("Message error: {0}")]
    CctalkMessageError(CctalkMessageError),
    #[error("Cctalk serial write error")]
    CctalkSerialWriteError,
    #[error("Cctalk serial read error")]
    CctalkSerialReadError,
    #[error("Failed to Ack. Response: {0}")]
    CctalkFailedToAck(CcTalkHeader),
    #[error("Device Type Unknown")]
    CctalkDeviceTypeUnknown,
}

#[derive(Debug, Clone, Error, PartialEq, PartialOrd)]
pub enum EventError {
    #[error("MasterInhibitActive")]
    MasterInhibitActive = 0,
    #[error("BillReturnedFromEscrow")]
    BillReturnedFromEscrow = 1,
    #[error("InvalidBillValidation")]
    InvalidBillValidation = 2,
    #[error("InvalidBillTransport")]
    InvalidBillTransport = 3,
    #[error("InhibitedBillCctalk")]
    InhibitedBillCctalk = 4,
    #[error("InhibitedBillDipSW")]
    InhibitedBillDipSW = 5,
    #[error("BillJammedTransportUnsafe")]
    BillJammedTransportUnsafe = 6,
    #[error("BillJammedStacker")]
    BillJammedStacker = 7,
    #[error("BillPulledBackwards")]
    BillPulledBackwards = 8,
    #[error("BillTamper")]
    BillTamper = 9,
    #[error("StackerOk")]
    StackerOk = 10,
    #[error("StackerRemoved")]
    StackerRemoved = 11,
    #[error("StackerInserted")]
    StackerInserted = 12,
    #[error("StackerFaulty")]
    StackerFaulty = 13,
    #[error("StackerFull")]
    StackerFull = 14,
    #[error("StackerJammed")]
    StackerJammed = 15,
    #[error("BillJammedTransportSafe")]
    BillJammedTransportSafe = 16,
    #[error("OptoFraudDetected")]
    OptoFraudDetected = 17,
    #[error("StringFraudDetected")]
    StringFraudDetected = 18,
    #[error("AntiStringMechFaulty")]
    AntiStringMechFaulty = 19,
    #[error("BarcodeDetected")]
    BarcodeDetected = 20,
    #[error("UnknownBillStacked")]
    UnknownBillStacked = 21,
    #[error("UnknownError")]
    UnknownError = 22,
}

impl From<u8> for EventError {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::MasterInhibitActive,
            1 => Self::BillReturnedFromEscrow,
            2 => Self::InvalidBillValidation,
            3 => Self::InvalidBillTransport,
            4 => Self::InhibitedBillCctalk,
            5 => Self::InhibitedBillDipSW,
            6 => Self::BillJammedTransportUnsafe,
            7 => Self::BillJammedStacker,
            8 => Self::BillPulledBackwards,
            9 => Self::BillTamper,
            10 => Self::StackerOk,
            11 => Self::StackerRemoved,
            12 => Self::StackerInserted,
            13 => Self::StackerFaulty,
            14 => Self::StackerFull,
            15 => Self::StackerJammed,
            16 => Self::BillJammedTransportSafe,
            17 => Self::OptoFraudDetected,
            18 => Self::StringFraudDetected,
            19 => Self::AntiStringMechFaulty,
            20 => Self::BarcodeDetected,
            21 => Self::UnknownBillStacked,
            _ => Self::UnknownError,
        }
    }
}
