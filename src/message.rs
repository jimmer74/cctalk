use super::errors::CctalkMessageError;
use super::headers::CcTalkHeader;
use heapless::Vec as hVec;

// #[allow(dead_code)]
#[derive(Debug, PartialEq, PartialOrd)]
pub struct CctalkMessage {
    src: u8,
    dest: u8,
    header: u8,
    data: hVec<u8, 255>,
    len: u8,
    chksum: Option<u8>,
}

#[allow(dead_code)]
impl CctalkMessage {
    //length and chksum are calced from supplied data (source, dest, header, data).
    //if everything else is correct, then message will be correct
    pub fn new(dest: u8, src: u8, header: CcTalkHeader, data: hVec<u8, 255>) -> CctalkMessage {
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
                let mut tmp: hVec<u8, 255> = hVec::new();
                for n in 4..data_len - 1 {
                    _ = tmp
                        .push(data[n as usize])
                        .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
                }
                tmp
            } else {
                hVec::new()
            },
            len: data[1],
            chksum: Some(data[data_len as usize - 1]),
        };

        Ok(rx)
    }

    pub fn try_to_bytes(&self) -> Result<hVec<u8, 260>, CctalkMessageError> {
        let tx_buf_len = 5 + self.len;
        let mut tx_buf = hVec::new();

        println!("msg buffer len = {} bytes", tx_buf_len);
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
            .push(self.chksum.unwrap())
            .map_err(|x| CctalkMessageError::HVecFailedToPush(x));
        println!("{:#04X?}", tx_buf);

        Ok(tx_buf)
    }

    pub fn dest(self: &Self) -> u8 {
        self.dest
    }
    pub fn data(self: &Self) -> &hVec<u8, 255> {
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
