use super::errors::CctalkMessageError;
use super::headers::CcTalkHeader;
use heapless::String as hString;
use heapless::Vec as hVec;
use heapless::format;
pub type Msg8 = CctalkMessage<Cctalk8BitChksumMessage>;
pub type Msg16 = CctalkMessage<CctalkCRC16ChksumMessage>;
// type TryFromBytesFn = fn(data: &[u8]) -> Result<CctalkMessage<CctalkCRC16ChksumMessage>, CctalkMessageError>;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct CctalkMessage<T> {
    value: T,
}

/*====================================================================
 *
 *
 *              These fn's are specific for the Msg16 type
 *
 *
 *==================================================================== */
impl CctalkMessage<CctalkCRC16ChksumMessage> {
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

        Self { value: msg }
    }

    pub fn try_to_bytes(&self) -> Result<hVec<u8, 260>, CctalkMessageError> {
        let mut tx_buf = hVec::new();

        _ = tx_buf
            .push(self.value.dest)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        _ = tx_buf
            .push(self.value.len)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        _ = tx_buf
            .push(self.value.chksum_lsb)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        _ = tx_buf
            .push(self.value.header)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        for dat in self.value.data.iter() {
            _ = tx_buf
                .push(*dat)
                .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        }
        _ = tx_buf
            .push(self.value.chksum_msb)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));

        Ok(tx_buf)
    }

    pub fn try_from_bytes(
        data: &[u8],
    ) -> Result<CctalkMessage<CctalkCRC16ChksumMessage>, CctalkMessageError> {
        let packet_len = data.len();

        if packet_len <= 4 {
            return Err(CctalkMessageError::MessageTooShort(packet_len));
        }

        if data[1] as usize + 5 != packet_len {
            // println!(
            //     "actual packet len: {}, calculated packet len: {}, \npacket: {:02X?}",
            //     packet_len,
            //     data[1] + 5,
            //     &data[..]
            // );
            return Err(CctalkMessageError::IncorrectDataLen(
                data[1] + 5,
                packet_len as u8,
            ));
        }

        let rx = CctalkCRC16ChksumMessage {
            chksum_lsb: data[2],
            dest: data[0],
            header: data[3],
            data: if packet_len > 5 {
                let mut tmp: hVec<u8, 255> = hVec::new();
                for n in 4..packet_len - 1 {
                    _ = tmp
                        .push(data[n as usize])
                        .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
                }
                tmp
            } else {
                hVec::new()
            },
            len: data[1],
            chksum_msb: data[packet_len as usize - 1],
        };

        Ok(CctalkMessage { value: rx })
    }
}
/*======================================================================
 *
 *
 *              These fn's are specific for the Msg8 type
 *
 *
 *====================================================================== */

impl CctalkMessage<Cctalk8BitChksumMessage> {
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
        Self { value: msg }
    }

    pub fn chksum(self: &Self) -> u8 {
        self.value.chksum
    }

    pub fn calc_chksum(self: &Self) -> u8 {
        let data_sum = self.value.data.iter().map(|&x| x as u16).sum::<u16>();
        let proto_sum: u8 = (self.value.dest as u16
            + self.value.len as u16
            + self.value.src as u16
            + self.value.header as u16
            + data_sum) as u8;
        (256 - proto_sum as u16) as u8
    }

    pub fn chksum_valid(self: &Self) -> Result<u8, CctalkMessageError> {
        //happy path, chksum exists
        let calc_chksum = self.calc_chksum();

        if self.value.chksum == calc_chksum {
            Ok(self.value.chksum)
        } else {
            Err(CctalkMessageError::IncorrectChksum(
                self.value.chksum,
                calc_chksum,
            ))
        }
    }

    pub fn try_from_bytes(
        data: &[u8],
    ) -> Result<CctalkMessage<Cctalk8BitChksumMessage>, CctalkMessageError> {
        let packet_len = data.len();

        if packet_len <= 4 {
            return Err(CctalkMessageError::MessageTooShort(packet_len));
        }

        if data[1] as usize + 5 != packet_len {
            // println!(
            //     "actual packet len: {}, calculated packet len: {}, \npacket: {:02X?}",
            //     packet_len,
            //     data[1] + 5,
            //     &data[..]
            // );
            return Err(CctalkMessageError::IncorrectDataLen(
                data[1] + 5,
                packet_len as u8,
            ));
        }

        let rx = Cctalk8BitChksumMessage {
            src: data[2],
            dest: data[0],
            header: data[3],
            data: if packet_len > 5 {
                let mut tmp: hVec<u8, 255> = hVec::new();
                for n in 4..packet_len - 1 {
                    _ = tmp
                        .push(data[n as usize])
                        .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
                }
                tmp
            } else {
                hVec::new()
            },
            len: data[1],
            chksum: data[packet_len as usize - 1],
        };

        Ok(CctalkMessage { value: rx })
    }

    pub fn try_to_bytes(&self) -> Result<hVec<u8, 260>, CctalkMessageError> {
        let mut tx_buf = hVec::new();

        _ = tx_buf
            .push(self.value.dest)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        _ = tx_buf
            .push(self.value.len)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        _ = tx_buf
            .push(self.value.src)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        _ = tx_buf
            .push(self.value.header)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        for dat in self.value.data.iter() {
            _ = tx_buf
                .push(*dat)
                .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        }
        _ = tx_buf
            .push(self.value.chksum)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));

        Ok(tx_buf)
    }
}
/*=====================================================================================
 *
 *
 *          These fn's work for any/all message types (eg Msg8,Msg16, etc)
 *
 *
 *===================================================================================== */
impl<M> CctalkMessage<M>
where
    M: MessageType,
{
    pub fn inner(&self) -> &M {
        &self.value
    }

    pub fn data_str(&self) -> hString<255>
    where
        M: MessageType,
    {
        hString::from_utf8(self.value.data().clone()).unwrap()
    }

    pub fn len_valid(self: &Self) -> Result<u8, CctalkMessageError> {
        if self.value.data().len() as u8 == self.value.len() {
            Ok(self.value.len())
        } else {
            Err(CctalkMessageError::IncorrectDataLen(
                self.value.len(),
                self.value.data().len() as u8,
            ))
        }
    }

    pub fn data(&self) -> &hVec<u8, 255> {
        &self.value.data()
    }
    pub fn dest(self: &Self) -> u8 {
        self.value.dest()
    }

    pub fn header(self: &Self) -> CcTalkHeader {
        self.value.header().into()
    }

    pub fn len(self: &Self) -> u8 {
        self.value.data().len() as u8
    }

    pub fn packet_len(self: &Self) -> usize {
        self.value.data().len() as usize + 1 + 1 + 1 + 1 + 1
    }
}

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

impl From<CctalkCRC16ChksumMessage> for Cctalk8BitChksumMessage {
    fn from(value: CctalkCRC16ChksumMessage) -> Self {
        Self {
            dest: value.dest,
            len: value.len,
            src: value.chksum_lsb,
            header: value.header,
            data: value.data,
            chksum: value.chksum_msb,
        }
    }
}

impl CctalkCRC16ChksumMessage {
    pub fn data_str(&self) -> hString<255> {
        hString::from_utf8(self.data.clone()).unwrap()
    }

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
impl Cctalk8BitChksumMessage {
    pub fn calc_chksum(self: &Self) -> u8 {
        let data_sum = self.data.iter().map(|&x| x as u16).sum::<u16>();
        let proto_sum: u8 =
            (self.dest as u16 + self.len as u16 + self.src as u16 + self.header as u16 + data_sum)
                as u8;
        (256 - proto_sum as u16) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headers::CcTalkHeader::ResetDevice;
    use crate::message::CctalkMessage;

    #[test]
    fn test_16_bit_crc() {
        let msg = CctalkMessage::<CctalkCRC16ChksumMessage>::new(0x28, ResetDevice, hVec::new());

        assert_eq!(msg.value.chksum_lsb, 0x46);
        assert_eq!(msg.value.chksum_msb, 0x3f);
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

        println!("{}", testcase.calc_chksum());
        let res = testcase.chksum_valid();
        match res {
            Err(e) => {
                assert_eq!(
                    String::from("Wrong chksum: 255, should be: 251"),
                    format!("{}", e)
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
                    String::from("Wrong data length: 0, should be: 2"),
                    format!("{}", e)
                )
            }
            _ => {}
        };
    }

    #[test]
    fn test_to_bytes() {
        let testcase = CctalkMessage::<Cctalk8BitChksumMessage>::new(
            0x02,
            0x01,
            CcTalkHeader::UploadCalibrationData,
            hVec::from_array([0x34, 0xFF, 0x98, 0x13]),
        );

        assert_eq!(
            testcase
                .try_to_bytes()
                .expect("error couldn't convert test msg to bytes - fix the test!'"),
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
    fn test_from_bytes() {
        let cct = CctalkMessage::<Cctalk8BitChksumMessage>::new(
            0x02,
            0x01,
            CcTalkHeader::SimplePoll,
            hVec::from_array([0x22, 0x01]),
        );

        let data = cct
            .try_to_bytes()
            .expect("error couldn't convert test msg to bytes - fix the test!'");

        match CctalkMessage::<Cctalk8BitChksumMessage>::try_from_bytes(&data) {
            Ok(new_cct) => {
                assert_eq!(cct, new_cct);
            }
            Err(e) => {
                assert_eq!(e, CctalkMessageError::NoChkSum)
            }
        }
    }
}
