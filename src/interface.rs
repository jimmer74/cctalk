use super::message::CctalkMessage;
use crate::errors::{CctalkMessageError, CctalkTransmissionError};
use embedded_hal_nb::serial::{self, Read, Write};
use heapless::Vec as hVec;
use nb::block;

pub struct Cctalk<UART> {
    uart: UART,
    echo: bool,
}

impl<UART> Cctalk<UART>
where
    UART: Read + Write,
{
    pub fn new(uart: UART, echo: bool) -> Self {
        Self { uart, echo }
    }

    pub fn transfer(
        &mut self,
        msg: CctalkMessage,
    ) -> Result<CctalkMessage, CctalkTransmissionError> {
        let _msg_bytes = self
            .write(msg.clone())
            .map_err(|_e| CctalkTransmissionError::FailedToTxData)?;

        println!("flushing buffer");
        match block!(self.uart.flush()) {
            Ok(_) => {}
            Err(_) => return Err(CctalkTransmissionError::FailedToTxData),
        }

        if self.echo {
            match self.read() {
                Ok(rx_msg) => {
                    if rx_msg == msg {
                        println!("echo matches, discarding");
                    } else {
                        return Err(CctalkTransmissionError::FailedToReciveEcho);
                    }
                }
                Err(_e) => {
                    return Err(CctalkTransmissionError::FailedToReciveEcho);
                }
            }
        }

        // TODO: rewrite the below!
        let mut rx_buf: [u8; 260] = [0u8; 260];
        println!("reading header");
        for n in 0..=3 {
            rx_buf[n] = block!(self.uart.read())
                .map_err(|_| CctalkTransmissionError::FailedToFetchHeader)?;
        }
        println!("read header bytes: {:02x?}", &rx_buf[0..4]);
        let data_len = rx_buf[1] as usize;

        let packet_size = 4 + data_len + 1; // [(dest, data_len, src. header), data[..], chksum ]

        println!("reading data/chksum bytes");
        for n in 4..packet_size {
            rx_buf[n] = block!(self.uart.read())
                .map_err(|_| CctalkTransmissionError::FailedToFetchHeader)?;
        }

        println!("read data bytes: {:02x?}", &rx_buf[4..packet_size - 1]);
        println!("read chksum: {:02x?}", &rx_buf[packet_size - 1]);

        match CctalkMessage::from_bytes(&rx_buf[0..packet_size]) {
            Ok(msg) => match (msg.len_valid(), msg.chksum_valid()) {
                (Ok(_len), Ok(_chksum)) => return Ok(msg),
                (Ok(_len), Err(e)) => {
                    println!("{}", e);
                    return Err(CctalkTransmissionError::FailedToRxData);
                }
                (Err(e), Ok(_)) => {
                    println!("{}", e);
                    return Err(CctalkTransmissionError::FailedToRxData);
                }
                (Err(elen), Err(echk)) => {
                    println!("{}, {}", elen, echk);
                    return Err(CctalkTransmissionError::FailedToRxData);
                }
            },
            Err(e) => {
                println!(
                    "rx bytes are not a valid msg: {:02x?}",
                    &rx_buf[0..packet_size]
                );
                return Err(CctalkTransmissionError::FailedToConvertToMessage);
            }
        };
    }

    pub fn read(&mut self) -> Result<CctalkMessage, embedded_hal_nb::serial::ErrorKind> {
        let mut n = 0_usize;
        let mut rx_buf: [u8; 260] = [0u8; 260];

        loop {
            match self.uart.read() {
                Ok(byte) => {
                    // println!("rx: {:02x?}, byte no: {}", byte, n);
                    rx_buf[n] = byte;
                    n = n + 1;
                }
                Err(nb::Error::WouldBlock) => {
                    // println!("would block");
                    break;
                }
                Err(nb::Error::Other(e)) => {
                    // println!("{:?}", e);
                    break;
                }
            }
        }

        CctalkMessage::from_bytes(&rx_buf[0..n])
            .map_err(|_e| embedded_hal_nb::serial::ErrorKind::Other)
    }

    pub fn write(
        &mut self,
        msg: CctalkMessage,
    ) -> Result<hVec<u8, 260>, embedded_hal_nb::serial::ErrorKind> {
        let msg_bytes = msg.try_to_bytes().map_err(|_e| serial::ErrorKind::Other)?; //{
        //     Ok(msg) => msg,
        //     Err(CctalkMessageError::HVecFailedToPush(dat)) => {
        //         return Err(CctalkTransmissionError::FailedToFillTxBuffer);
        //     }
        //     Err(_) => return Err(CctalkTransmissionError::FailedToFillTxBuffer),
        // };
        //
        println!("sending: {:02x?}", msg_bytes);

        for dat in msg_bytes.as_slice() {
            block!(self.uart.write(*dat)).map_err(|_e| serial::ErrorKind::Other)?;
        }

        Ok(msg_bytes)
    }
}

mod tests {
    use super::*;
    use crate::headers::CcTalkHeader;
    use embedded_hal_nb::serial::ErrorKind::Other;
    use heapless::Vec as hVec;
    use nb::Error::WouldBlock;

    #[test]
    fn test_rx_cctalk_msg() {
        use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};
        let rx_bytes = [
            0x01,                                      //dest
            0x04,                                      //data len - 4 bytes
            0x02,                                      //source
            CcTalkHeader::UploadCalibrationData as u8, //header
            0x34,                                      // data byte 1
            0xFF,                                      // data byte 2
            0x98,                                      // data byte 3
            0x13,                                      // data byte 4
            0x53,
        ];

        let expectations = [
            UartTransaction::read_many(rx_bytes),
            UartTransaction::read_error(WouldBlock),
        ];

        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), true);

        let result = cctalk.read().map_err(|_e| CctalkMessageError::NoChkSum);

        assert_eq!(CctalkMessage::from_bytes(&rx_bytes), result);

        uart.done();
    }

    #[test]
    fn test_tx_cctalk_msg() {
        use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};

        let tx_case = CctalkMessage::new(
            0x06,
            0x01,
            crate::headers::CcTalkHeader::ResetDevice,
            hVec::new(),
        );

        let expectations = [UartTransaction::write_many([
            0x06,
            0x00,
            0x01,
            crate::headers::CcTalkHeader::ResetDevice as u8,
            0xF8,
        ])];

        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), true);

        let result = cctalk.write(tx_case.clone()).unwrap();
        assert_eq!(&result[..], tx_case.try_to_bytes().unwrap().as_slice());
        uart.done();
    }

    // #[test]
    fn test_send_cctalk_echo() {
        use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};

        let tx_case = CctalkMessage::new(
            0x02,
            0x01,
            crate::headers::CcTalkHeader::SimplePoll,
            hVec::new(),
        );

        let rx_bytes = [
            0x01,                                      //dest
            0x04,                                      //data len - 4 bytes
            0x02,                                      //source
            CcTalkHeader::UploadCalibrationData as u8, //header
            0x34,                                      // data byte 1
            0xFF,                                      // data byte 2
            0x98,                                      // data byte 3
            0x13,                                      // data byte 4
            0x53,
        ];

        let expectations = [
            UartTransaction::write_many(tx_case.try_to_bytes().unwrap()),
            UartTransaction::flush(),
            UartTransaction::read_many(tx_case.try_to_bytes().unwrap()),
            // UartTransaction::read_error(embedded_hal_nb::serial::ErrorKind::from(
            //     CctalkTransmissionError::FailedToReciveEcho,
            // )),
        ];

        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), true);

        let result = cctalk.transfer(tx_case);

        assert_eq!(
            result.unwrap(),
            CctalkMessage::from_bytes(&rx_bytes).unwrap()
        );

        uart.done();
    }

    #[test]
    fn test_send_cctalk_noecho() {
        use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};

        let tx_case = CctalkMessage::new(
            0x02,
            0x01,
            crate::headers::CcTalkHeader::SimplePoll,
            hVec::new(),
        );

        let rx_bytes = [
            0x01,                                      //dest
            0x04,                                      //data len - 4 bytes
            0x02,                                      //source
            CcTalkHeader::UploadCalibrationData as u8, //header
            0x34,                                      // data byte 1
            0xFF,                                      // data byte 2
            0x98,                                      // data byte 3
            0x13,                                      // data byte 4
            0x53,
        ];

        let expectations = [
            UartTransaction::write_many(tx_case.try_to_bytes().unwrap()),
            UartTransaction::flush(),
            UartTransaction::read_many(rx_bytes),
            // UartTransaction::read_error(WouldBlock), -- need to re-enable, when i put read
            // functionality at end of transfer fn
        ];

        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), false);

        let result = cctalk.transfer(tx_case);

        assert_eq!(
            result.unwrap(),
            CctalkMessage::from_bytes(&rx_bytes).unwrap()
        );

        uart.done();
    }

    // #[test]
    fn test_send_cctalk_incorrect_echo() {
        use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};

        let tx_case = CctalkMessage::new(
            0x02,
            0x01,
            crate::headers::CcTalkHeader::SimplePoll,
            hVec::new(),
        );

        let expectations = [
            UartTransaction::write_many(tx_case.try_to_bytes().unwrap()),
            UartTransaction::flush(),
            UartTransaction::read_many([0x02, 0x00, 0x01, 0xFE, 0xF7]), //chksum is incorrect should
                                                                        //be 0xFF (we have flipped
                                                                        //the 4th bit - simulates
                                                                        //a common erro on the wire)
                                                                        // UartTransaction::read_error(CctalkTransmissionError::FailedToReciveEcho),
        ];

        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), true);

        let result = cctalk.transfer(tx_case);

        assert_eq!(result, Err(CctalkTransmissionError::FailedToReciveEcho));

        uart.done();
    }

    // #[test]
    fn test_rx_cctalk_incorrect_len() {
        use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};

        let tx_case = CctalkMessage::new(
            0x02,
            0x01,
            crate::headers::CcTalkHeader::SimplePoll,
            hVec::new(),
        );

        let rx_bytes = [
            0x01,                                      //dest
            0x04,                                      //data len - 4 bytes
            0x02,                                      //source
            CcTalkHeader::UploadCalibrationData as u8, //header
            0x34,                                      // data byte 1
            0xFF,                                      // data byte 2
            0x98,                                      // data byte 3
            0x13,                                      // data byte 4
            0x53,
        ];

        let expectations = [
            UartTransaction::write_many(tx_case.try_to_bytes().unwrap()),
            UartTransaction::flush(),
            UartTransaction::read_many(rx_bytes),
        ];

        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), false);

        let result = cctalk.transfer(tx_case);

        assert_eq!(
            result.unwrap(),
            CctalkMessage::from_bytes(&rx_bytes).unwrap()
        );

        uart.done();
    }
}
