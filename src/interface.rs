use super::headers::CcTalkHeader;
use super::message::CctalkMessage;
use crate::{DEFAULT_TIMEOUT_MS, MASTER_ADDR, errors::CctalkTransmissionError};
use embedded_hal::delay::DelayNs;
use embedded_hal_nb::serial::{Read, Write};
use heapless::Vec as hVec;
use nb::block;

const ADDR_POL: [u8; 5] = [000, 000, 001, CcTalkHeader::AddressPoll as u8, 002];

#[derive(Debug, PartialEq, PartialOrd)]
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

    //INFO: orginally had a write flush in here, which produced weird inconsistent results when
    //reading the echo
    //WARN: do not put flush back!!!!

    /***************************************************************
     *
     *
     *         Transfer functions
     *      - complete transactions:
     *              (send msg, check echo, get data back)
     *
     *****************************************************************/

    pub fn header_only(
        &mut self,
        addr: u8,
        header: CcTalkHeader,
        timeout_ms: Option<u32>,
    ) -> Result<CctalkMessage, CctalkTransmissionError> {
        let msg = CctalkMessage::new(addr, MASTER_ADDR, header, hVec::new());
        let res = self.transfer(msg, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;
        let header = res.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };
        Ok(res)
    }

    pub fn transfer(
        &mut self,
        msg: CctalkMessage,
        timeout_ms: u32,
    ) -> Result<CctalkMessage, CctalkTransmissionError> {
        let msg_bytes = self.write_msg(msg.clone())?;
        if self.echo {
            match self.read_msg_exact(timeout_ms, msg_bytes.len()) {
                Ok(rx_msg) => {
                    // println!("echo matches - discarding!");
                    if rx_msg != msg {
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

        let msg = self.read_msg(timeout_ms)?;

        let _chksum = msg
            .chksum_valid()
            .map_err(|_e| CctalkTransmissionError::RxDataMalformedChksum)?;

        Ok(msg)
    }

    pub fn addr_scan(&mut self, timeout_ms: u32) -> Result<hVec<u8, 260>, CctalkTransmissionError> {
        let tx = CctalkMessage::try_from_bytes(&ADDR_POL).unwrap();
        let _ = self.write_msg(tx.clone());

        if self.echo {
            match self.read_msg_exact(20, tx.packet_len()) {
                Ok(rx_msg) => {
                    if rx_msg != tx {
                        return Err(CctalkTransmissionError::FailedToReciveEcho);
                    }
                }
                Err(_e) => {
                    return Err(CctalkTransmissionError::CctalkSerialReadError);
                }
            }
        }

        match self.read_bytes(timeout_ms) {
            Ok(bytes) => return Ok(bytes),
            Err(e) => return Err(e),
        }
    }

    /***************************************************************
     *
     *
     *                       Read functions
     *
     *
     *****************************************************************/
    pub fn read_msg_exact(
        &mut self,
        timeout_ms: u32,
        num_bytes: usize,
    ) -> Result<CctalkMessage, CctalkTransmissionError> {
        // let mut n = 0_usize;
        let mut rx_buf: hVec<u8, 260> = hVec::new();
        const POLL_INTERVAL_MS: u32 = 5_u32;
        let mut elapsed_ms = 0_u32;
        if num_bytes <= 4 {
            return Err(CctalkTransmissionError::RxDataMalformedLength);
        }
        loop {
            match self.uart.read() {
                Ok(byte) => {
                    _ = rx_buf.push(byte);
                    if rx_buf.len() == num_bytes {
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
                    return Err(CctalkTransmissionError::CctalkSerialReadError);
                }
            }
        }

        CctalkMessage::try_from_bytes(rx_buf.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))
    }

    fn read_bytes(&mut self, timeout_ms: u32) -> Result<hVec<u8, 260>, CctalkTransmissionError> {
        let mut n = 0_usize;
        let mut rx_buf: hVec<u8, 260> = hVec::new();
        const POLL_INTERVAL_MS: u32 = 1_u32;
        let mut elapsed_ms = 0_u32;
        loop {
            match self.uart.read() {
                Ok(byte) => {
                    let _ = rx_buf.push(byte);
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
                    return Err(CctalkTransmissionError::CctalkSerialReadError);
                }
            }
        }

        Ok(rx_buf)
    }

    pub fn read_bytes_exact(
        &mut self,
        timeout_ms: u32,
        num_bytes: usize,
    ) -> Result<hVec<u8, 260>, CctalkTransmissionError> {
        let mut rx_buf: hVec<u8, 260> = hVec::new();
        const POLL_INTERVAL_MS: u32 = 5_u32;
        let mut elapsed_ms = 0_u32;
        loop {
            match self.uart.read() {
                Ok(byte) => {
                    let _ = rx_buf.push(byte);

                    if rx_buf.len() == num_bytes {
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
                    return Err(CctalkTransmissionError::CctalkSerialReadError);
                }
            }
        }

        Ok(rx_buf)
    }

    pub fn read_msg(&mut self, timeout_ms: u32) -> Result<CctalkMessage, CctalkTransmissionError> {
        let mut n = 0_usize;
        let mut rx_buf: [u8; 260] = [0u8; 260];
        const POLL_INTERVAL_MS: u32 = 1_u32;
        let mut elapsed_ms = 0_u32;
        loop {
            match self.uart.read() {
                Ok(byte) => {
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
                    return Err(CctalkTransmissionError::CctalkSerialReadError);
                }
            }
        }

        CctalkMessage::try_from_bytes(&rx_buf[0..n])
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))
    }
    /***************************************************************
     *
     *
     *                      Write functions
     *
     *
     *****************************************************************/
    pub fn write_msg(
        &mut self,
        msg: CctalkMessage,
    ) -> Result<hVec<u8, 260>, CctalkTransmissionError> {
        let msg_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        for dat in msg_bytes.as_slice() {
            let _ = block!(self.uart.write(*dat))
                .map_err(|_e| CctalkTransmissionError::CctalkSerialWriteError);
        }

        Ok(msg_bytes)
    }
}

#[allow(unused)]
mod tests {
    use super::*;
    use crate::device::CctalkDevice;
    use crate::headers::CcTalkHeader;
    use crate::{errors::*, headers::CcTalkHeader::Ack};
    use embedded_hal::delay::DelayNs;
    use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};
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

    use heapless::Vec as hVec;

    #[test]
    fn test_header_only_send() {
        const NOTE_ACC_ADDR: u8 = 0x28;

        let tx_msg = CctalkMessage::new(
            NOTE_ACC_ADDR,
            MASTER_ADDR,
            CcTalkHeader::SimplePoll,
            hVec::new(),
        );

        let rx_na_bytes = [MASTER_ADDR, 0x00, NOTE_ACC_ADDR, Ack as u8, 215];

        let expectations = [
            //Note Acceptor
            UartTransaction::write_many(tx_msg.try_to_bytes().unwrap()),
            UartTransaction::read_many(tx_msg.try_to_bytes().unwrap()),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_many(&rx_na_bytes),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
        ];

        let mut uart = UartMock::new(&expectations);
        let timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let res = cctalk.header_only(NOTE_ACC_ADDR, CcTalkHeader::SimplePoll, Some(5));
        println!("{:?}", res);
        assert_eq!(
            res.unwrap(),
            CctalkMessage::try_from_bytes(&rx_na_bytes).unwrap(),
        );
        //
        // dev.probe(&mut cctalk)
        //
        uart.done();
    }

    #[test]
    fn test_read_bytes_exact() {
        let tx = [
            0x28, 0x22, 0x34, 0x35, 0x00, //dest
        ];

        let rx_bytes: hVec<u8, 260> = hVec::from_array(tx);

        let expectations = [UartTransaction::read_many(tx)];

        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };

        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let result = cctalk.read_bytes_exact(5, 5);

        assert_eq!(result.unwrap(), rx_bytes);

        uart.done();
    }

    #[test]
    fn test_read_bytes_exact_too_many_bytes() {
        let tx = [
            0x28, 0x22, 0x34, 0x35, 0x00, //dest
        ];

        let rx_bytes: hVec<u8, 260> = hVec::from_array(tx);

        let expectations = [
            UartTransaction::read_many([0x28, 0x22, 0x34, 0x35]),
            UartTransaction::read_error(WouldBlock),
        ];

        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };

        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let result = cctalk.read_bytes_exact(5, 5);

        assert_ne!(result.unwrap(), rx_bytes); // WARN: left 4 bytes, right 5 bytes - currently no error
        // handling on fn. Need to implement and revise!

        uart.done();
    }

    #[test]
    fn test_rx_cctalk_msg() {
        let rx_bytes = [
            0x01,                                      //dest
            0x04,                                      //data len - 4 bytes
            0x02,                                      //source
            CcTalkHeader::UploadCalibrationData as u8, //header
            0x34,                                      // data byte 1
            0xFF,                                      // data byte 2
            0x98,                                      // data byte 3
            0x13,                                      // data byte 4
            0x53,                                      // chksum
        ];

        let expectations = [UartTransaction::read_many(rx_bytes)];
        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let result = cctalk
            .read_msg_exact(2, 9)
            .map_err(|_e| CctalkMessageError::NoChkSum);

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

        let result = cctalk.write_msg(tx_case.clone()).unwrap();
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
            UartTransaction::read_many(tx_case.try_to_bytes().unwrap()),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_many(rx_bytes),
            UartTransaction::read_error(WouldBlock),
        ];

        let mut uart = UartMock::new(&expectations);
        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let result = cctalk.transfer(tx_case, 2);

        assert_eq!(
            result.unwrap(),
            CctalkMessage::try_from_bytes(&rx_bytes).unwrap()
        );

        uart.done();
    }

    #[test]
    fn test_read_bytes() {
        let tx = ADDR_POL;
        let rx_bytes: hVec<u8, 260> = hVec::from_array([
            0x28, //dest
        ]);

        let expectations = [
            UartTransaction::read(0x28),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
        ];

        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };

        let mut uart = UartMock::new(&expectations);
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let result = cctalk.read_bytes(2);

        assert_eq!(result.unwrap(), rx_bytes);

        uart.done();
    }

    #[test]
    fn test_send_cctalk_no_echo() {
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
            UartTransaction::read_many(rx_bytes),
            UartTransaction::read_error(WouldBlock), // functionality at end of transfer fn
            UartTransaction::read_error(WouldBlock),
        ];

        let mut uart = UartMock::new(&expectations);
        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut cctalk = Cctalk::new(uart.clone(), timer, false);

        let result = cctalk.transfer(tx_case, 2);

        assert_eq!(
            result.unwrap(),
            CctalkMessage::try_from_bytes(&rx_bytes).unwrap()
        );

        uart.done();
    }

    #[test]
    fn test_send_cctalk_incorrect_echo() {
        // !todo("fix this");
        // use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};

        let tx_case = CctalkMessage::new(
            0x02,
            0x01,
            crate::headers::CcTalkHeader::SimplePoll,
            hVec::new(),
        );

        let expectations = [
            UartTransaction::write_many(tx_case.try_to_bytes().unwrap()),
            UartTransaction::read_many([0x02, 0x00, 0x01, 0xFE, 0xF7]), //chksum is incorrect should
                                                                        //be 0xFF (we have flipped
                                                                        //the 4th bit - simulates
                                                                        //a common erro on the wire)
                                                                        // UartTransaction::read_error(WouldBlock),
                                                                        // UartTransaction::read_error(WouldBlock),
                                                                        // UartTransaction::read_error(WouldBlock),
        ];

        let mut uart = UartMock::new(&expectations);
        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut cctalk = Cctalk::new(uart.clone(), timer, true);

        let result = cctalk.transfer(tx_case, 2);

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
            UartTransaction::read_many(rx_bytes),
            UartTransaction::read_error(WouldBlock),
            UartTransaction::read_error(WouldBlock),
        ];

        let mut uart = UartMock::new(&expectations);

        let mut timer = MockDelay {
            total_ms_delayed: 0,
        };
        let mut cctalk = Cctalk::new(uart.clone(), timer, false);

        //this is definately an error, and this will
        //convert it to an Option<E> for easy assert
        let result = cctalk.transfer(tx_case, 2).err();

        assert_eq!(
            Some(CctalkTransmissionError::CctalkMessageError(
                CctalkMessageError::IncorrectDataLen(8, 9)
            )),
            result
        );

        uart.done();
    }
}
