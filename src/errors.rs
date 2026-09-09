use std::{error::Error, fmt::Display};

#[derive(Debug, PartialEq, PartialOrd)]
pub enum CctalkMessageError {
    IncorrestDataLen(u8, u8), //incorrect current length, calced data length
    IncorrectChksum(u8, u8),  //current checksum, correct checksum
    MessageTooShort(u8),      // current message length
    NoChkSum,
}

impl Error for CctalkMessageError {}
impl Display for CctalkMessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            CctalkMessageError::IncorrestDataLen(curr, corr) => {
                format!("Wrong data length: {}, should be: {}", curr, corr)
            }
            CctalkMessageError::IncorrectChksum(curr, corr) => {
                format!("Wrong chksum: {}, should be: {}", curr, corr)
            }
            CctalkMessageError::MessageTooShort(len) => {
                format!(
                    "CCtalk packet too short: {} bytes (min valid length: 5 bytes)",
                    len
                )
            }
            CctalkMessageError::NoChkSum => String::from("Chksum not set!"),
        };

        write!(f, "Error: {msg}")
    }
}
