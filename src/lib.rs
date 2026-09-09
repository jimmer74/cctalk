/*

 *  pin 1 - ccTalk Data
 *  pin 2 - n/a
 *  pin 3 - n/a
 *  pin 4 - n/a
 *  pin 5 - /RESET - leave floating if not used
 *  pin 6v- n/a
 *  pin 7 - 12-24v dc
 *  pin 8 - GND
 *  pin 9 - Serial mode (low for ccTalk if no dip sw)
 *  pin 10 - reserved (leave floating)

               GND
                │
      ┌────────▼────┐
    2 │  O O O X O  │ 10
      │             │
    1 │  X O X X X  │ 9
      └──▲─┌─▲─▲─▲──┘
         │ └─┼─┤ │
  Data ──┘   │ │ └── Mode
/RESET ──────┘ └──── 12v-24v)


Non open-collector TX on raspberry pi needs following to enable 2-wire
to 1-wire cctalk device. D should be a fast switching schottky (low forward voltage drop)

           PWR
            ▲                            3v3
            │                             ▲
            │                             │
            │                            ┌┴┐
┌───────────┴────────────┐               │ │
│                        │               │ │ 10K
│                        │               └┬┘
│                     TX ┼───────── D ────┤
│                        │       ◄─────   │
│           uC           │                │
│                        │                │
│                        │                │
│                     RX ┼────────────────┴─────── CCtalk Device
│                        │
│                        │
└────────────┬───────────┘
             │
             │
             │
             ▼
            GND

*/

pub mod errors;
pub mod headers;
pub mod tx;
use std::iter;

use embedded_io::{Read, Write};
use errors::CctalkMessageError;
use headers::CcTalkHeader;

pub struct Cctalk<UART> {
    uart: UART,
}

impl<UART> Cctalk<UART>
where
    UART: Read + Write,
{
    pub fn new(uart: UART) -> Self {
        Self { uart }
    }

    pub fn transfer(msg: CctalkMessage) -> Result<CctalkMessage, UART::Error> {
        // Ok(msg)
        todo!("need to write tx fn and test on actual hardware + make interface board/connector");
    }
}

// #[allow(dead_code)]
#[derive(Debug, PartialEq, PartialOrd)]
pub struct CctalkMessage {
    src: u8,
    dest: u8,
    header: u8,
    data: Vec<u8>,
    len: u8,
    chksum: Option<u8>,
}

#[allow(dead_code)]
impl CctalkMessage {
    //length and chksum are calced from supplied data (source, dest, header, data).
    //if everything else is correct, then message will be correct
    // TODO: if header has no data component, should we test this and fail construction?
    pub fn new(dest: u8, src: u8, header: CcTalkHeader, data: Vec<u8>) -> CctalkMessage {
        // let data_sum = data.iter().map(|&x| x as u16).sum::<u16>();
        let data_len = data.len() as u8;
        // let proto_sum: u8 =
        //     (dest as u16 + data_len as u16 + src as u16 + header as u16 + data_sum) as u8;
        // let chksum = Some((256 - proto_sum as u16) as u8);
        let mut msg = CctalkMessage {
            dest,
            len: data_len,
            src,
            header: header as u8,
            data,
            chksum: None,
        };

        msg.chksum = Some(msg.calc_chksum());

        msg
    }
    // packet [dest, len, src, header, data[..], chksum]
    pub fn from_bytes(data: &[u8]) -> Result<CctalkMessage, CctalkMessageError> {
        let data_len = data.len() as u8;

        if data_len <= 4 {
            return Err(CctalkMessageError::MessageTooShort(data_len));
        }

        //if data_len > 5

        let rx = CctalkMessage {
            src: data[2],
            dest: data[0],
            header: data[3],
            data: if data_len > 5 {
                let mut tmp: Vec<u8> = vec![];
                for n in 4..data_len - 1 {
                    tmp.push(data[n as usize])
                }
                tmp
            } else {
                vec![]
            },
            len: data[1],
            chksum: Some(data[data_len as usize - 1]),
        };

        Ok(rx)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let tx_buf_len = 5 + self.len;
        let mut tx_buf = Vec::new();

        println!("msg buffer len = {} bytes", tx_buf_len);
        tx_buf.push(self.dest);
        tx_buf.push(self.len);
        tx_buf.push(self.src);
        tx_buf.push(self.header);
        for dat in self.data.iter() {
            tx_buf.push(*dat);
        }
        tx_buf.push(self.chksum.unwrap());
        println!("{:#04X?}", tx_buf);

        tx_buf
    }

    pub fn dest(self: &Self) -> u8 {
        self.dest
    }
    pub fn data(self: &Self) -> &Vec<u8> {
        &self.data
    }
    pub fn header(self: &Self) -> u8 {
        self.header
    }

    pub fn len(self: &Self) -> u8 {
        self.data.len() as u8
    }

    pub fn chksum(self: &Self) -> Option<u8> {
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
        if let Some(curr_chksum) = self.chksum {
            let calc_chksum = self.calc_chksum();

            if curr_chksum == calc_chksum {
                Ok(curr_chksum)
            } else {
                Err(CctalkMessageError::IncorrectChksum(
                    curr_chksum,
                    calc_chksum,
                ))
            }
        }
        //sad path, chksum not set!
        else {
            Err(CctalkMessageError::NoChkSum)
        }
    }

    pub fn len_valid(self: &Self) -> Result<u8, CctalkMessageError> {
        if self.data.len() as u8 == self.len {
            Ok(self.len)
        } else {
            Err(CctalkMessageError::IncorrestDataLen(
                self.len,
                self.data.len() as u8,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_cctalk() {
        //this is message that new should construct
        let testcase = CctalkMessage {
            dest: 0x02,
            len: 0x00,
            src: 0x01,
            header: CcTalkHeader::SimplePoll as u8,
            data: Vec::new(),
            chksum: Some(0xFF),
        };

        assert_eq!(
            testcase,
            CctalkMessage::new(0x02, 0x01, CcTalkHeader::SimplePoll, Vec::new())
        );
    }

    #[test]
    fn test_bad_cctalk() {
        //this is an incorrect message (checksum should be wrong)
        let testcase = CctalkMessage {
            dest: 0x02,
            //new fn auto-calcs len, need to supply manually here
            len: 0x02,
            src: 0x01,
            //new fn converts this on fly, but have to do manually here
            header: CcTalkHeader::DispenseHopperCoins as u8,
            data: vec![0x02, 0x02],
            chksum: Some(0x22),
        };

        //check new() does not match above (i.e. checksum is wrong as expected)
        assert_ne!(
            testcase,
            CctalkMessage::new(
                0x02,
                0x01,
                CcTalkHeader::DispenseHopperCoins,
                vec![0x02, 0x02]
            )
        );
    }

    #[test]
    fn test_wrong_chksum() {
        let testcase = CctalkMessage {
            dest: 0x02,
            len: 0x02,
            src: 0x01,
            header: CcTalkHeader::SimplePoll as u8,
            data: vec![0x2, 0x0],
            chksum: Some(0xFF),
        };

        println!("{}", testcase.calc_chksum());
        let res = testcase.chksum_valid();
        match res {
            Err(e) => {
                assert_eq!(
                    String::from("Error: Wrong chksum: 255, should be: 251"),
                    format!("{}", e)
                )
            }
            _ => {}
        };
    }

    #[test]
    fn test_wrong_len() {
        let testcase = CctalkMessage {
            dest: 0x02,
            len: 0x00,
            src: 0x01,
            header: CcTalkHeader::SimplePoll as u8,
            data: vec![0x2, 0x0],
            chksum: Some(0xFF),
        };
        let res = testcase.len_valid();
        match res {
            Err(e) => {
                assert_eq!(
                    String::from("Error: Wrong data length: 0, should be: 2"),
                    format!("{}", e)
                )
            }
            _ => {}
        };
    }

    #[test]
    fn test_to_bytes() {
        let testcase = CctalkMessage::new(
            0x02,
            0x01,
            CcTalkHeader::UploadCalibrationData,
            vec![0x34, 0xFF, 0x98, 0x13],
        );

        assert_eq!(
            testcase.to_bytes(),
            vec![
                0x02,                                      //dest
                0x04,                                      //data len - 4 bytes
                0x01,                                      //source
                CcTalkHeader::UploadCalibrationData as u8, //header
                0x34,                                      // data byte 1
                0xFF,                                      // data byte 2
                0x98,                                      // data byte 3
                0x13,                                      // data byte 4
                0x53                                       // chksum
            ]
        )
    }

    #[test]
    fn test_from_bytes() {
        let cct = CctalkMessage::new(0x02, 0x01, CcTalkHeader::SimplePoll, vec![0x22, 0x01]);

        let data = cct.to_bytes();

        match CctalkMessage::from_bytes(&data) {
            Ok(new_cct) => {
                assert_eq!(cct, new_cct);
            }
            Err(e) => {
                assert_eq!(e, CctalkMessageError::NoChkSum)
            }
        }
    }
}
