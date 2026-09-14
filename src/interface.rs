use super::message::CctalkMessage;
use crate::errors::{CctalkMessageError, CctalkTransmissionError};
use embedded_hal::delay::DelayNs;
use embedded_hal_nb::serial::{self, Read, Write};
use heapless::Vec as hVec;
use nb::block;

const CCTALK_TIMEOUT_MS: u32 = 200_u32;

pub struct Cctalk<UART, DELAY> {
    uart: UART,
    delay: DELAY,
    echo: bool,
}

impl<UART, DELAY> Cctalk<UART, DELAY>
where
    UART: Read + Write,
    DELAY: DelayNs,
{
    pub fn new(uart: UART, delay: DELAY, echo: bool) -> Self {
        Self { uart, delay, echo }
    }

    pub fn transfer(
        &mut self,
        msg: CctalkMessage,
        timeout_ms: u32,
    ) -> Result<CctalkMessage, CctalkTransmissionError> {
        let msg_bytes = self
            .write(msg.clone())
            .map_err(|_e| CctalkTransmissionError::FailedToFillTxBuffer)?;

        // println!("flushing buffer");
        match block!(self.uart.flush()) {
            Ok(_) => {} // println!("successfully flushed"),
            Err(_) => {
                // println!("failed to flush");
                return Err(CctalkTransmissionError::FailedToTxData);
            }
        }
        self.delay.delay_ms(10);
        if self.echo {
            match self.read_exact(timeout_ms, msg_bytes.len()) {
                Ok(rx_msg) => {
                    if rx_msg == msg {
                        println!("echo matches, discarding");
                    } else {
                        println!("echo does not match");
                        return Err(CctalkTransmissionError::FailedToReciveEcho);
                    }
                }
                Err(e) => {
                    println!("read error: {}", e);
                    return Err(CctalkTransmissionError::FailedToReciveEcho);
                }
            }
        }

        let msg = self
            .read(timeout_ms)
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        // let _data_len = msg
        //     .len_valid()
        //     .map_err(|_e| CctalkTransmissionError::RxDataMalformedLength)?;

        let _chksum = msg
            .chksum_valid()
            .map_err(|_e| CctalkTransmissionError::RxDataMalformedChksum)?;

        Ok(msg)
    }
    pub fn read_exact(
        &mut self,
        timeout_ms: u32,
        num_bytes: usize,
    ) -> Result<CctalkMessage, CctalkMessageError> {
        let mut n = 0_usize;
        let mut rx_buf: [u8; 260] = [0u8; 260];
        const POLL_INTERVAL_MS: u32 = 1_u32;
        let mut elapsed_ms = 0_u32;
        loop {
            match self.uart.read() {
                Ok(byte) => {
                    // println!("rx: {:02x?}, byte no: {}", byte, n);
                    rx_buf[n] = byte;
                    n = n + 1;

                    if n == num_bytes {
                        break;
                    }
                }
                Err(nb::Error::WouldBlock) => {
                    self.delay.delay_ms(POLL_INTERVAL_MS);
                    elapsed_ms += POLL_INTERVAL_MS;

                    if elapsed_ms >= timeout_ms {
                        break;
                    }
                    continue;
                }
                Err(nb::Error::Other(_e)) => {
                    break; //writelnreturn // println!("{:?}", e);
                }
            }
        }

        CctalkMessage::try_from_bytes(&rx_buf[0..n])
        // .map_err(|_e| embedded_hal_nb::serial::ErrorKind::Other)
    }
    pub fn read(&mut self, timeout_ms: u32) -> Result<CctalkMessage, CctalkMessageError> {
        let mut n = 0_usize;
        let mut rx_buf: [u8; 260] = [0u8; 260];
        const POLL_INTERVAL_MS: u32 = 1_u32;
        let mut elapsed_ms = 0_u32;
        loop {
            match self.uart.read() {
                Ok(byte) => {
                    // println!("rx: {:02x?}, byte no: {}", byte, n);
                    rx_buf[n] = byte;
                    n = n + 1;

                    if n == 260 {
                        break;
                    }
                }
                Err(nb::Error::WouldBlock) => {
                    self.delay.delay_ms(POLL_INTERVAL_MS);
                    elapsed_ms += POLL_INTERVAL_MS;

                    if elapsed_ms >= timeout_ms {
                        break;
                    }
                    continue;
                }
                Err(nb::Error::Other(_e)) => {
                    break; //writelnreturn // println!("{:?}", e);
                }
            }
        }

        CctalkMessage::try_from_bytes(&rx_buf[0..n])
        // .map_err(|_e| embedded_hal_nb::serial::ErrorKind::Other)
    }

    pub fn write(
        &mut self,
        msg: CctalkMessage,
    ) -> Result<hVec<u8, 260>, embedded_hal_nb::serial::ErrorKind> {
        let msg_bytes = msg.try_to_bytes().map_err(|_e| serial::ErrorKind::Other)?;

        println!("sending: {:02x?}", msg_bytes);

        for dat in msg_bytes.as_slice() {
            block!(self.uart.write(*dat)).map_err(|_e| serial::ErrorKind::Other)?;
        }

        Ok(msg_bytes)
    }
}

#[allow(unused)]
mod tests {
    use super::*;
    use crate::{errors::CctalkTransmissionError::RxDataMalformedLength, headers::CcTalkHeader};
    use embedded_hal::delay::DelayNs;
    use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};
    use heapless::Vec as hVec;
    use nb::Error::WouldBlock;

    struct MockDelay {
        pub total_ms_delayed: u32,
    }

    impl DelayNs for MockDelay {
        fn delay_ns(&mut self, ns: u32) {
            self.total_ms_delayed += ns / 1_000_000;
        }
        fn delay_ms(&mut self, mut ms: u32) {
            self.total_ms_delayed += ms;
        }
    }

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
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
        ];
        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let result = cctalk.read(200).map_err(|_e| CctalkMessageError::NoChkSum);

        assert_eq!(CctalkMessage::try_from_bytes(&rx_bytes), result);

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
        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let result = cctalk.write(tx_case.clone()).unwrap();
        assert_eq!(&result[..], tx_case.try_to_bytes().unwrap().as_slice());
        uart.done();
    }

    #[test]
    fn test_send_cctalk_echo() {
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
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_many(rx_bytes),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
        ];

        let mut uart = UartMock::new(&expectations);
        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let result = cctalk.transfer(tx_case);

        assert_eq!(
            result.unwrap(),
            CctalkMessage::try_from_bytes(&rx_bytes).unwrap()
        );

        uart.done();
    }

    #[test]
    fn test_send_cctalk_no_echo() {
        // use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};

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
            UartTransaction::read_error(WouldBlock), // functionality at end of transfer fn
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock), //
        ];

        let mut uart = UartMock::new(&expectations);
        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut cctalk = Cctalk::new(uart.clone(), timer, false);

        let result = cctalk.transfer(tx_case);

        assert_eq!(
            result.unwrap(),
            CctalkMessage::try_from_bytes(&rx_bytes).unwrap()
        );

        uart.done();
    }

    #[test]
    fn test_send_cctalk_incorrect_echo() {
        // use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};

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
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
        ];

        let mut uart = UartMock::new(&expectations);
        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let result = cctalk.transfer(tx_case);

        assert_eq!(result, Err(CctalkTransmissionError::FailedToReciveEcho));

        uart.done();
    }

    #[test]
    fn test_rx_cctalk_incorrect_len() {
        // use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};

        let tx_case = CctalkMessage::new(
            0x02,
            0x01,
            crate::headers::CcTalkHeader::SimplePoll,
            hVec::new(),
        );

        let rx_bytes = [
            0x01, //dest
            0x03, //data len - should be 4 bytes - will create
            //RxDataMalformedLength error
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
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            // UartTransaction::read_error(WouldBlock),
        ];

        let mut uart = UartMock::new(&expectations);

        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut cctalk = Cctalk::new(uart.clone(), timer, false);

        //this is definately an error, and this will
        //convert it to an Option<E> for easy assert
        let result = cctalk.transfer(tx_case).err();

        assert_eq!(
            Some(CctalkTransmissionError::CctalkMessageError(
                CctalkMessageError::IncorrectDataLen(8, 9)
            )),
            result
        );

        uart.done();
    }
}
