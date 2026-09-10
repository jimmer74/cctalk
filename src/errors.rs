use std::fmt::Debug;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, PartialOrd)]
pub enum CctalkMessageError {
    #[error("Wrong data length: {0}, should be: {1}")]
    IncorrestDataLen(u8, u8), //incorrect current length, calced data length
    #[error("Wrong chksum: {0}, should be: {1}")]
    IncorrectChksum(u8, u8), //current checksum, correct checksum
    #[error("CCtalk packet too short: {0} bytes (min valid length: 5 bytes)")]
    MessageTooShort(u8),
    #[error("Heapless vec is full, cannot push byte: {0:#02X}")] // current message length
    HVecFailedToPush(u8), // hVec is full!
    #[error("Chksum not set!")] //
    NoChkSum,
}

#[derive(Error, Debug)]
pub enum CctalkTransmissionError {
    #[error("Failed to tx cctalk data")]
    FailedToTxData,
    #[error("failed to receive tx echo. Sent: {0}, Received: {0}")]
    FailedToReciveEcho(u8, u8),
    #[error("failed to fill tx_buffer. Offending byte: {0}")]
    FailedToFillTxBuffer(u8),
    #[error("failed to fetch [dest,len,src,header]")]
    FailedToFetchHeader,
    #[error("failed to rx data and chksum")]
    FailedToRxDataAndChksum,
    #[error("failed to convert rx data to msg: {0}")]
    FailedToConvertToMessage(#[from] CctalkMessageError),
}
