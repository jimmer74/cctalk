use std::fmt::Debug;
use thiserror::Error;

use crate::headers::CcTalkHeader;

#[derive(Error, Debug, PartialEq, PartialOrd)]
pub enum CctalkMessageError {
    #[error("Wrong data length: {0}, should be: {1}")]
    IncorrectDataLen(u8, u8), //incorrect current length, calced data length
    #[error("Wrong chksum: {0}, should be: {1}")]
    IncorrectChksum(u8, u8), //current checksum, correct checksum
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

#[derive(Debug, Clone, Error)]
pub enum EventError {
    #[error("Acceptance Error")]
    MasterInhibitActive = 0x00,
    #[error("Acceptance Error")]
    BillReturnedFromEscrow = 0x01,
    #[error("Acceptance Error")]
    InvalidBillValidation = 0x02,
    #[error("Acceptance Error")]
    InvalidBillTransport = 0x03,
    #[error("Acceptance Error")]
    InhibitedBillCctalk = 0x04,
    #[error("Acceptance Error")]
    InhibitedBillDipSW = 0x05,
    #[error("Acceptance Error")]
    BillJammedTransportUnsafe = 0x06,
    #[error("Acceptance Error")]
    BillJammedStacker = 0x07,
    #[error("Acceptance Error")]
    BillPulledBackwards = 0x08,
    #[error("Acceptance Error")]
    BillTamper = 0x09,
    #[error("Acceptance Error")]
    StackerOk = 0x0A,
    #[error("Acceptance Error")]
    StackerRemoved = 0x0B,
    #[error("Acceptance Error")]
    StackerInserted = 0x0C,
    #[error("Acceptance Error")]
    StackerFaulty = 0x0D,
    #[error("Acceptance Error")]
    StackerFull = 0x0E,
    #[error("Acceptance Error")]
    StackerJammed = 0x0F,
    #[error("Acceptance Error")]
    BillJammedTransportSafe = 0x10,
    #[error("Acceptance Error")]
    OptoFraudDetected = 0x11,
    #[error("Acceptance Error")]
    StringFraudDetected = 0x12,
    #[error("Acceptance Error")]
    AntiStringMechFaulty = 0x13,
    #[error("Acceptance Error")]
    BarcodeDetected = 0x14,
    #[error("Acceptance Error")]
    UnknownBillStacked = 0x15,
    #[error("Acceptance Error")]
    UnknownError = 0x16,
}

impl From<u8> for EventError {
    fn from(value: u8) -> Self {
        match value {
            0x01 => Self::MasterInhibitActive,
            0x02 => Self::BillReturnedFromEscrow,
            0x03 => Self::InvalidBillValidation,
            0x04 => Self::InvalidBillTransport,
            0x05 => Self::InhibitedBillCctalk,
            0x06 => Self::InhibitedBillDipSW,
            0x07 => Self::BillJammedTransportUnsafe,
            0x08 => Self::BillJammedStacker,
            0x09 => Self::BillPulledBackwards,
            0x0A => Self::BillTamper,
            0x0B => Self::StackerOk,
            0x0C => Self::StackerRemoved,
            0x0D => Self::StackerInserted,
            0x0E => Self::StackerFaulty,
            0x0F => Self::StackerFull,
            0x10 => Self::StackerJammed,
            0x11 => Self::BillJammedTransportSafe,
            0x12 => Self::OptoFraudDetected,
            0x13 => Self::StringFraudDetected,
            0x14 => Self::AntiStringMechFaulty,
            0x15 => Self::BarcodeDetected,
            _ => Self::UnknownBillStacked,
        }
    }
}
