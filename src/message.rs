use crate::{device::CctalkEncStatus, errors::CctalkTransmissionError};

use super::errors::CctalkMessageError;
use super::headers::CcTalkHeader;
use heapless::{String as hString, Vec as hVec, format};

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum CctalkMessage {
    MSG8(Cctalk8BitChksumMessage),
    MSG16(CctalkCRC16ChksumMessage),
}

impl CctalkMessage {
    /*=========================================================================
     *
     *
     *                      Constructor functions
     *
     *
     *========================================================================= */
    pub fn new(
        r#type: CctalkEncStatus,
        src: Option<u8>,
        dest: u8,
        header: CcTalkHeader,
        data: hVec<u8, 255>,
    ) -> Self {
        let ret = match r#type {
            CctalkEncStatus::Unenc16bit => {
                Self::MSG16(CctalkCRC16ChksumMessage::new(dest, header, data))
            }
            _ => Self::MSG8(Cctalk8BitChksumMessage::new(
                dest,
                src.unwrap(),
                header,
                data,
            )),
        };

        ret
    }

    pub fn from_bytes(
        r#type: CctalkEncStatus,
        rx_bytes: hVec<u8, 260>,
    ) -> Result<CctalkMessage, CctalkTransmissionError> {
        match r#type {
            CctalkEncStatus::Unenc16bit => {
                let msg = CctalkCRC16ChksumMessage::try_from(rx_bytes)
                    .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
                Ok(Self::MSG16(msg))
            }
            _ => {
                let msg = Cctalk8BitChksumMessage::try_from(rx_bytes)
                    .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
                Ok(Self::MSG8(msg))
            }
        }
    }
    /*=========================================================================
     *
     *
     *                  Convenience functions for transparently
     *                  getting at underlyling struct data
     *
     *
     *========================================================================= */
    pub fn data(&self) -> &hVec<u8, 255> {
        match self {
            CctalkMessage::MSG8(msg) => &msg.data,
            CctalkMessage::MSG16(msg) => &msg.data,
        }
    }

    pub fn dest(self: &Self) -> u8 {
        match self {
            CctalkMessage::MSG8(msg) => msg.dest,
            CctalkMessage::MSG16(msg) => msg.dest,
        }
    }

    pub fn header(&self) -> CcTalkHeader {
        match self {
            CctalkMessage::MSG8(msg) => msg.header.into(),
            CctalkMessage::MSG16(msg) => msg.header.into(),
        }
    }
    pub fn len(self: &Self) -> u8 {
        match self {
            CctalkMessage::MSG8(msg) => msg.len,
            CctalkMessage::MSG16(msg) => msg.len,
        }
    }
    pub fn packet_len(&self) -> usize {
        match self {
            CctalkMessage::MSG8(msg) => msg.packet_len(),
            CctalkMessage::MSG16(msg) => msg.packet_len(),
        }
    }

    pub fn is_ack(&self) -> bool {
        match self {
            CctalkMessage::MSG8(msg) => msg.header == CcTalkHeader::Ack as u8,
            CctalkMessage::MSG16(msg) => msg.header == CcTalkHeader::Ack as u8,
        }
    }

    pub fn data_str(&self) -> hString<255> {
        match self {
            CctalkMessage::MSG8(msg) => hString::from_utf8(msg.data.clone()).unwrap(),
            CctalkMessage::MSG16(msg) => hString::from_utf8(msg.data.clone()).unwrap(),
        }
    }

    pub fn len_valid(self: &Self) -> Result<u8, CctalkMessageError> {
        let (data, len) = match self {
            CctalkMessage::MSG8(msg) => (msg.data.clone(), msg.len),
            CctalkMessage::MSG16(msg) => (msg.data.clone(), msg.len),
        };

        if data.len() as u8 == len {
            Ok(len)
        } else {
            Err(CctalkMessageError::IncorrectDataLen(len, data.len() as u8))
        }
    }
}
/*====================================================================
 *
 *
 *              These fn's are specific for the Msg16 type
 *
 *
 *==================================================================== */
impl Into<hVec<u8, 260>> for CctalkMessage {
    fn into(self) -> hVec<u8, 260> {
        match self {
            CctalkMessage::MSG8(msg8) => {
                let mut tx_buf = hVec::new();

                _ = tx_buf.push(msg8.dest);
                _ = tx_buf.push(msg8.len);
                _ = tx_buf.push(msg8.src);
                _ = tx_buf.push(msg8.header);
                for dat in msg8.data.iter() {
                    _ = tx_buf.push(*dat);
                }
                _ = tx_buf.push(msg8.chksum);

                tx_buf
            }
            CctalkMessage::MSG16(msg16) => {
                let mut tx_buf = hVec::new();

                _ = tx_buf.push(msg16.dest);
                _ = tx_buf.push(msg16.len);
                _ = tx_buf.push(msg16.chksum_lsb);
                _ = tx_buf.push(msg16.header);
                for dat in msg16.data.iter() {
                    _ = tx_buf.push(*dat);
                }
                _ = tx_buf.push(msg16.chksum_msb);

                tx_buf
            }
        }
    }
}

impl TryFrom<hVec<u8, 260>> for CctalkCRC16ChksumMessage {
    type Error = CctalkMessageError;

    fn try_from(value: hVec<u8, 260>) -> Result<Self, Self::Error> {
        //is packet too short (cctalk packets are 5 bytes minimum)
        let packet_len = value.len();

        if packet_len <= 4 {
            return Err(CctalkMessageError::MessageTooShort(packet_len));
        }

        if value[1] as usize + 5 != packet_len {
            return Err(CctalkMessageError::IncorrectDataLen(
                value[1] + 5,
                packet_len as u8,
            ));
        }

        let res = CctalkCRC16ChksumMessage {
            chksum_lsb: value[2],
            dest: value[0],
            header: value[3],
            data: if packet_len > 5 {
                let mut tmp: hVec<u8, 255> = hVec::new();
                for n in 4..packet_len - 1 {
                    _ = tmp
                        .push(value[n as usize])
                        .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
                }
                tmp
            } else {
                hVec::new()
            },
            len: value[1],
            chksum_msb: value[packet_len as usize - 1],
        };

        // is chksum correct?
        let calc_chksum = res.calc_chksum();
        let act_chksum: u16 = ((res.chksum_msb as u16) << 8) | res.chksum_lsb as u16;

        if calc_chksum != act_chksum {
            return Err(CctalkMessageError::IncorrectChksum16(
                act_chksum,
                calc_chksum,
            ));
        }

        Ok(res)
    }
}

impl CctalkCRC16ChksumMessage {
    //length and chksum are calced from supplied data (source, dest, header, data).
    //if everything else is correct, then message will be correct

    pub fn new(dest: u8, header: CcTalkHeader, data: hVec<u8, 255>) -> Self {
        let data_len = data.len() as u8;
        let mut msg = CctalkCRC16ChksumMessage {
            dest,
            len: data_len,
            chksum_lsb: 0x00,
            header: header as u8,
            data,
            chksum_msb: 0x00,
        };
        let chksum = msg.calc_chksum().to_le_bytes();
        msg.chksum_lsb = chksum[0];
        msg.chksum_msb = chksum[1];

        msg
    }

    pub fn chksum_valid(self: &Self) -> Result<u16, CctalkMessageError> {
        //happy path, chksum exists
        let calc_chksum = self.calc_chksum();
        let curr_chksum = (self.chksum_msb as u16) << 8 | self.chksum_lsb as u16;
        if curr_chksum == calc_chksum {
            Ok(curr_chksum)
        } else {
            Err(CctalkMessageError::IncorrectChksum16(
                curr_chksum,
                calc_chksum,
            ))
        }
    }
}
/*======================================================================
 *
 *
 *              These fn's are specific for the Msg8 type
 *
 *
 *====================================================================== */
// impl Into<hVec<u8, 260>> for Msg8 {
//     fn into(self) -> hVec<u8, 260> {}
// }

impl TryFrom<hVec<u8, 260>> for Cctalk8BitChksumMessage {
    type Error = CctalkMessageError;

    fn try_from(value: hVec<u8, 260>) -> Result<Self, Self::Error> {
        //is packet too short (cctalk packets are 5 bytes minimum)
        let packet_len = value.len();

        if packet_len <= 4 {
            return Err(CctalkMessageError::MessageTooShort(packet_len));
        }

        if value[1] as usize + 5 != packet_len {
            return Err(CctalkMessageError::IncorrectDataLen(
                value[1] + 5,
                packet_len as u8,
            ));
        }

        //convert to Msg8
        let res = Cctalk8BitChksumMessage {
            src: value[2],
            dest: value[0],
            header: value[3],
            data: if packet_len > 5 {
                let mut tmp: hVec<u8, 255> = hVec::new();
                for n in 4..packet_len - 1 {
                    _ = tmp
                        .push(value[n as usize])
                        .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
                }
                tmp
            } else {
                hVec::new()
            },
            len: value[1],
            chksum: value[packet_len as usize - 1],
        };

        // is chksum correct?
        let calc_chksum = res.calc_chksum();
        let act_chksum = res.chksum;

        if calc_chksum != act_chksum {
            return Err(CctalkMessageError::IncorrectChksum(act_chksum, calc_chksum));
        }

        Ok(res)
    }
}

impl Cctalk8BitChksumMessage {
    pub fn new(dest: u8, src: u8, header: CcTalkHeader, data: hVec<u8, 255>) -> Self {
        let data_len = data.len() as u8;

        let mut msg = Cctalk8BitChksumMessage {
            dest,
            len: data_len,
            src,
            header: header as u8,
            data,
            chksum: 0x00,
        };

        msg.chksum = msg.calc_chksum();
        msg
    }

    pub fn chksum(self: &Self) -> u8 {
        self.chksum
    }

    pub fn calc_chksum(self: &Self) -> u8 {
        let data_sum = self.data.iter().map(|&x| x as u16).sum::<u16>();
        let proto_sum: u8 =
            (self.dest as u16 + self.len as u16 + self.src as u16 + self.header as u16 + data_sum)
                as u8;
        (256 - proto_sum as u16) as u8
    }

    pub fn chksum_valid(self: &Self) -> Result<u8, CctalkMessageError> {
        //happy path, chksum exists
        let calc_chksum = self.calc_chksum();

        if self.chksum == calc_chksum {
            Ok(self.chksum)
        } else {
            Err(CctalkMessageError::IncorrectChksum(
                self.chksum,
                calc_chksum,
            ))
        }
    }

    pub fn src(&self) -> u8 {
        self.src
    }
}
/*=====================================================================================
 *
 *
 *          These fn's work for any/all message types (eg Msg8,Msg16, etc)
 *
 *
 *===================================================================================== */
// impl<M> CctalkMessage<M>
// where
//     M: MessageType,
// {
//     pub fn inner(&self) -> &M {
//         &self.value
//     }
//
//     pub fn data_str(&self) -> hString<255>
//     where
//         M: MessageType,
//     {
//         hString::from_utf8(self.value.data().clone()).unwrap()
//     }
//
//     pub fn len_valid(self: &Self) -> Result<u8, CctalkMessageError> {
//         if self.value.data().len() as u8 == self.value.len() {
//             Ok(self.value.len())
//         } else {
//             Err(CctalkMessageError::IncorrectDataLen(
//                 self.value.len(),
//                 self.value.data().len() as u8,
//             ))
//         }
//     }
//
//     pub fn data(&self) -> &hVec<u8, 255> {
//         &self.value.data()
//     }
//     pub fn dest(self: &Self) -> u8 {
//         self.value.dest()
//     }
//
//     pub fn header(self: &Self) -> CcTalkHeader {
//         self.value.header().into()
//     }
//
//     pub fn len(self: &Self) -> u8 {
//         self.value.data().len() as u8
//     }
//
//     pub fn packet_len(self: &Self) -> usize {
//         self.value.data().len() as usize + 1 + 1 + 1 + 1 + 1
//     }
//
//     pub fn is_ack(&self) -> bool {
//         self.header() == CcTalkHeader::Ack
//     }
// }

pub trait MessageType {
    fn data(&self) -> &hVec<u8, 255>;

    fn dest(self: &Self) -> u8;

    fn header(&self) -> CcTalkHeader;

    fn len(self: &Self) -> u8;

    fn packet_len(&self) -> usize;
}

impl MessageType for Cctalk8BitChksumMessage {
    fn data(&self) -> &hVec<u8, 255> {
        &self.data
    }
    fn dest(self: &Self) -> u8 {
        self.dest
    }

    fn header(self: &Self) -> CcTalkHeader {
        self.header.into()
    }

    fn len(self: &Self) -> u8 {
        self.data.len() as u8
    }

    fn packet_len(self: &Self) -> usize {
        self.data.len() as usize + 1 + 1 + 1 + 1 + 1
    }
}

impl MessageType for CctalkCRC16ChksumMessage {
    fn data(&self) -> &hVec<u8, 255> {
        &self.data
    }

    fn dest(self: &Self) -> u8 {
        self.dest
    }

    fn header(self: &Self) -> CcTalkHeader {
        self.header.into()
    }

    fn len(self: &Self) -> u8 {
        self.data.len() as u8
    }

    fn packet_len(self: &Self) -> usize {
        self.data.len() as usize + 1 + 1 + 1 + 1 + 1
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Cctalk8BitChksumMessage {
    dest: u8,
    len: u8,
    src: u8,
    header: u8,
    data: hVec<u8, 255>,
    chksum: u8,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct CctalkCRC16ChksumMessage {
    dest: u8,
    len: u8,
    chksum_lsb: u8,
    header: u8,
    data: hVec<u8, 255>,
    chksum_msb: u8,
}

impl CctalkCRC16ChksumMessage {
    fn calc_chksum(&self) -> u16 {
        //WARN: Kermit uses x^16 + x^12+x^5 + 1 Polynomial
        //Initial CRC Reg = 0x0000
        // Which mataches Appendix 9 of CCtalk spec Part 3
        // INFO: however I discovered crc is non-reflected, I then stumbled
        // accross XMODEM which is like KERMIT, but non-reflected, and works!

        let mut container: hVec<u8, 260> = hVec::new();
        use crc16::*;

        _ = container.push(self.dest);
        _ = container.push(self.len);
        _ = container.push(self.header);

        if !self.data.is_empty() {
            for dat in self.data.clone() {
                _ = container.push(dat);
            }
        };

        let chksum = State::<XMODEM>::calculate(container.as_slice());

        chksum
    }

    // pub fn header(&self) -> CcTalkHeader {
    //     self.header.into()
    // }
}

impl core::fmt::Display for Cctalk8BitChksumMessage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg: hString<255> = format!(
            "From: {}\nData Len: {}\nTo: {}\nHeader: {}\nData: {:?}\nChksum: {:?} ",
            self.src,
            self.len,
            self.dest,
            CcTalkHeader::try_from(self.header).unwrap(),
            self.data.to_vec(),
            self.chksum
        )
        .unwrap();
        write!(f, "{}", msg.as_view())
    }
}

// impl Cctalk8BitChksumMessage {
//     pub fn calc_chksum(self: &Self) -> u8 {
//         let data_sum = self.data.iter().map(|&x| x as u16).sum::<u16>();
//         let proto_sum: u8 =
//             (self.dest as u16 + self.len as u16 + self.src as u16 + self.header as u16 + data_sum)
//                 as u8;
//         (256 - proto_sum as u16) as u8
//     }
// }

#[cfg(feature = "std")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::headers::CcTalkHeader::ResetDevice;
    use crate::message::CctalkMessage;

    #[test]
    fn test_16_bit_crc() {
        let msg = CctalkMessage::<CctalkCRC16ChksumMessage>::new(0x28, ResetDevice, hVec::new());

        assert_eq!(msg.chksum_lsb, 0x46);
        assert_eq!(msg.chksum_msb, 0x3f);
    }

    #[test]
    fn test_bad_8bit_cctalk() {
        //this is an incorrect message (checksum should be wrong)
        let testcase = CctalkMessage {
            value: Cctalk8BitChksumMessage {
                dest: 0x02,
                //new fn auto-calcs len, need to supply manually here
                len: 0x02,
                src: 0x01,
                //new fn converts this on fly, but have to do manually here
                header: CcTalkHeader::DispenseHopperCoins as u8,
                data: hVec::from_array([0x02, 0x02]),
                chksum: 0x22,
            },
        };

        //check new() does not match above (i.e. checksum is wrong as expected)
        assert_ne!(
            testcase,
            CctalkMessage::<Cctalk8BitChksumMessage>::new(
                0x02,
                0x01,
                CcTalkHeader::DispenseHopperCoins,
                hVec::from_array([0x02, 0x02])
            )
        );
    }

    #[test]
    fn test_wrong_chksum() {
        let testcase = CctalkMessage {
            value: Cctalk8BitChksumMessage {
                dest: 0x02,
                len: 0x02,
                src: 0x01,
                header: CcTalkHeader::SimplePoll as u8,
                data: hVec::from([0x2, 0x0]),
                chksum: 0xFF,
            },
        };
        std::println!("{}", testcase.calc_chksum());
        let res = testcase.chksum_valid();
        match res {
            Err(e) => {
                assert_eq!(
                    std::string::String::from("Wrong chksum: 255, should be: 251"),
                    std::format!("{}", e)
                )
            }
            _ => {}
        };
    }

    #[test]
    fn test_wrong_len() {
        let testcase = CctalkMessage::<Cctalk8BitChksumMessage> {
            value: Cctalk8BitChksumMessage {
                dest: 0x02,
                len: 0x00,
                src: 0x01,
                header: CcTalkHeader::SimplePoll as u8,
                data: hVec::from_array([0x2, 0x0]),
                chksum: 0xFF,
            },
        };
        let res = testcase.len_valid();
        match res {
            Err(e) => {
                assert_eq!(
                    std::string::String::from("Wrong data length: 0, should be: 2"),
                    std::format!("{}", e)
                )
            }
            _ => {}
        };
    }

    #[test]
    fn test_msg8_to_bytes() {
        let testcase = CctalkMessage::<Cctalk8BitChksumMessage>::new(
            0x02,
            0x01,
            CcTalkHeader::UploadCalibrationData,
            hVec::from_array([0x34, 0xFF, 0x98, 0x13]),
        );
        let bytes: hVec<u8, 260> = testcase.into();
        assert_eq!(
            bytes,
            hVec::<u8, 255>::from_array([
                0x02,                                      //dest
                0x04,                                      //data len - 4 bytes
                0x01,                                      //source
                CcTalkHeader::UploadCalibrationData as u8, //header
                0x34,                                      // data byte 1
                0xFF,                                      // data byte 2
                0x98,                                      // data byte 3
                0x13,                                      // data byte 4
                0x53                                       // chksum
            ])
        )
    }

    #[test]
    fn test_msg8_from_bytes() {
        /*============================
         * TEST #1 - good transaction
         *============================ */
        let mut route_bill_to_stacker = [040, 001, 001, 154, 001, 059];
        let test_case: hVec<u8, 260> = hVec::from_array(route_bill_to_stacker);

        let test_msg = Msg8::try_from(test_case);
        let act_msg = Msg8::new(040, 001, CcTalkHeader::RouteBill, hVec::from_array([001]));

        //msg should be correct!
        assert_eq!(test_msg.unwrap(), act_msg);

        /*==========================
         * TEST #2 - bad msg chksum
         *========================== */
        route_bill_to_stacker[5] = 060; // alter chksum to incorrect value (60 - should be 59)
        let test_case2: hVec<u8, 260> = hVec::from_array(route_bill_to_stacker);

        let msg2 = Msg8::try_from(test_case2);

        //should be 59, but chksum is 60 so throws an error
        assert_eq!(Err(CctalkMessageError::IncorrectChksum(60, 59)), msg2);

        /*==============================
         * TEST #3 - incorrect data len
         *============================== */
        route_bill_to_stacker[5] = 059; //reset chksum to correct val
        route_bill_to_stacker[1] = 002; //set incorrect data len (should be 001)
        let test_case3: hVec<u8, 260> = hVec::from_array(route_bill_to_stacker);

        let msg3 = Msg8::try_from(test_case3);

        //msg should be 7 bytes total (according to byte 2) but only 6 bytes - so throws an error
        assert_eq!(Err(CctalkMessageError::IncorrectDataLen(7, 6)), msg3);
    }
}
