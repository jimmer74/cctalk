use super::errors::CctalkMessageError;
use super::headers::CcTalkHeader;
use heapless::Vec as hVec;

// #[allow(dead_code)]
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
    //length and chksum are calced from supplied data (source, dest, header, data).
    //if everything else is correct, then message will be correct
    pub fn new(dest: u8, header: CcTalkHeader, data: hVec<u8, 255>) -> CctalkCRC16ChksumMessage {
        // let data_sum = data.iter().map(|&x| x as u16).sum::<u16>();
        let data_len = data.len() as u8;
        // let proto_sum: u8 =
        //     (dest as u16 + data_len as u16 + src as u16 + header as u16 + data_sum) as u8;
        // let chksum = Some((256 - proto_sum as u16) as u8);
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

    fn calc_chksum(&self) -> u16 {
        //WARN: Kermit uses x^16 + x^12+x^5 + 1 Polynomial
        //Initial CRC Reg = 0x0000
        // Which mataches Appendix 9 of CCtalk spec Part 3
        // INFO: however it is non-reflected, I stubled
        // accross XMODEM which is non-reflected and works!

        let mut container: hVec<u8, 260> = hVec::new();
        use crc16::*;

        _ = container.push(self.dest);
        _ = container.push(self.len);
        // _ = container.push(0x00);
        _ = container.push(self.header);

        if !self.data.is_empty() {
            // println!("found data array: {:?}, adding to chksum", self.data);
            for dat in self.data.clone() {
                _ = container.push(dat);
            }
        };

        // _ = container.push(0x00);

        // println!("data to be chksummed: {:?}", container);
        // container.reverse();
        let chksum = State::<XMODEM>::calculate(container.as_slice());
        // println!("Actual 16-bit CRC: {:04X?}", chksum);

        chksum
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum CctalkMessage {
    Standard8Bit(Cctalk8BitChksumMessage),
    CRC16Bit(CctalkCRC16ChksumMessage),
}

// impl Deref for CcTalkMessage {}

impl core::fmt::Display for Cctalk8BitChksumMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = format!(
            "From: {}\nData Len: {}\nTo: {}\nHeader: {}\nData: {:?}\nChksum: {:?} ",
            self.src,
            self.len,
            self.dest,
            CcTalkHeader::try_from(self.header).unwrap(),
            self.data.to_vec(),
            self.chksum
        );
        write!(f, "{}", msg)
    }
}
#[allow(dead_code)]
impl Cctalk8BitChksumMessage {
    //length and chksum are calced from supplied data (source, dest, header, data).
    //if everything else is correct, then message will be correct
    pub fn new(
        dest: u8,
        src: u8,
        header: CcTalkHeader,
        data: hVec<u8, 255>,
    ) -> Cctalk8BitChksumMessage {
        // let data_sum = data.iter().map(|&x| x as u16).sum::<u16>();
        let data_len = data.len() as u8;
        // let proto_sum: u8 =
        //     (dest as u16 + data_len as u16 + src as u16 + header as u16 + data_sum) as u8;
        // let chksum = Some((256 - proto_sum as u16) as u8);
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
    // packet [dest, len, src, header, data[..], chksum]
    pub fn try_from_bytes(data: &[u8]) -> Result<Cctalk8BitChksumMessage, CctalkMessageError> {
        let packet_len = data.len();

        if packet_len <= 4 {
            println!(
                "packet too short! Actual packet len: {}, packet: {:02X?}",
                packet_len,
                &data[..]
            );
            return Err(CctalkMessageError::MessageTooShort(packet_len));
        }

        if data[1] as usize + 5 != packet_len {
            println!(
                "actual packet len: {}, calculated packet len: {}, \npacket: {:02X?}",
                packet_len,
                data[1] + 5,
                &data[..]
            );
            return Err(CctalkMessageError::IncorrectDataLen(
                data[1] + 5,
                packet_len as u8,
            ));
        }
        //if data_len > 5

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

        Ok(rx)
    }

    pub fn try_to_bytes(&self) -> Result<hVec<u8, 260>, CctalkMessageError> {
        // let tx_buf_len = 5 + self.len;
        let mut tx_buf = hVec::new();

        //println!("msg buffer len = {} bytes", tx_buf_len);
        _ = tx_buf
            .push(self.dest)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        _ = tx_buf
            .push(self.len)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        _ = tx_buf
            .push(self.src)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        _ = tx_buf
            .push(self.header)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        for dat in self.data.iter() {
            _ = tx_buf
                .push(*dat)
                .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        }
        _ = tx_buf
            .push(self.chksum)
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        // println!("{:#04X?}", tx_buf);

        Ok(tx_buf)
    }

    pub fn dest(self: &Self) -> u8 {
        self.dest
    }
    pub fn data(self: &Self) -> &hVec<u8, 255> {
        &self.data
    }
    pub fn header(self: &Self) -> CcTalkHeader {
        self.header.into()
    }

    pub fn len(self: &Self) -> u8 {
        self.data.len() as u8
    }

    pub fn packet_len(self: &Self) -> usize {
        self.data.len() as usize + 1 + 1 + 1 + 1 + 1
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
    pub fn len_valid(self: &Self) -> Result<u8, CctalkMessageError> {
        if self.data.len() as u8 == self.len {
            Ok(self.len)
        } else {
            Err(CctalkMessageError::IncorrectDataLen(
                self.len,
                self.data.len() as u8,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    // use embedded_hal_mock::eh0::i2c::Transaction;

    use crate::headers::CcTalkHeader::{ResetDevice, SimplePoll};

    use super::*;

    #[test]
    fn test_16_bit_crc() {
        let msg = CctalkCRC16ChksumMessage::new(0x28, ResetDevice, hVec::new());

        assert_eq!(msg.chksum_lsb, 0x46);
        assert_eq!(msg.chksum_msb, 0x3f);
    }

    #[test]
    fn test_bad_8bit_cctalk() {
        //this is an incorrect message (checksum should be wrong)
        let testcase = Cctalk8BitChksumMessage {
            dest: 0x02,
            //new fn auto-calcs len, need to supply manually here
            len: 0x02,
            src: 0x01,
            //new fn converts this on fly, but have to do manually here
            header: CcTalkHeader::DispenseHopperCoins as u8,
            data: hVec::from_array([0x02, 0x02]),
            chksum: 0x22,
        };

        //check new() does not match above (i.e. checksum is wrong as expected)
        assert_ne!(
            testcase,
            Cctalk8BitChksumMessage::new(
                0x02,
                0x01,
                CcTalkHeader::DispenseHopperCoins,
                hVec::from_array([0x02, 0x02])
            )
        );
    }

    #[test]
    fn test_wrong_chksum() {
        let testcase = Cctalk8BitChksumMessage {
            dest: 0x02,
            len: 0x02,
            src: 0x01,
            header: CcTalkHeader::SimplePoll as u8,
            data: hVec::from([0x2, 0x0]),
            chksum: 0xFF,
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
        let testcase = Cctalk8BitChksumMessage {
            dest: 0x02,
            len: 0x00,
            src: 0x01,
            header: CcTalkHeader::SimplePoll as u8,
            data: hVec::from_array([0x2, 0x0]),
            chksum: 0xFF,
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
        let testcase = Cctalk8BitChksumMessage::new(
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
        let cct = Cctalk8BitChksumMessage::new(
            0x02,
            0x01,
            CcTalkHeader::SimplePoll,
            hVec::from_array([0x22, 0x01]),
        );

        let data = cct
            .try_to_bytes()
            .expect("error couldn't convert test msg to bytes - fix the test!'");

        match Cctalk8BitChksumMessage::try_from_bytes(&data) {
            Ok(new_cct) => {
                assert_eq!(cct, new_cct);
            }
            Err(e) => {
                assert_eq!(e, CctalkMessageError::NoChkSum)
            }
        }
    }
}
