use super::message::CctalkMessage;
use crate::errors::{CctalkMessageError, CctalkTransmissionError};
use embedded_hal_nb::serial::{Read, Write};
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
        // Ok(msg)
        let msg_bytes = match msg.try_to_bytes() {
            Ok(msg) => msg,
            Err(CctalkMessageError::HVecFailedToPush(dat)) => {
                return Err(CctalkTransmissionError::FailedToFillTxBuffer(dat));
            }
            Err(_) => return Err(CctalkTransmissionError::FailedToFillTxBuffer(0x00)),
        };

        let msg_len = msg_bytes.len();
        println!("sending: {:02x?}", msg_bytes);
        for dat in msg_bytes.as_slice() {
            block!(self.uart.write(*dat)).map_err(|_e| CctalkTransmissionError::FailedToTxData)?;
        }

        println!("flushing buffer");
        match block!(self.uart.flush()) {
            Ok(_) => {}
            Err(_) => return Err(CctalkTransmissionError::FailedToTxData),
        }

        if self.echo {
            println!("reading echo");
            let mut buf = [0u8; 1];
            for n in 0..msg_len {
                buf[0] = block!(self.uart.read())
                    .map_err(|_| CctalkTransmissionError::FailedToFetchHeader)?;
                if buf[0] != msg_bytes[n] {
                    return Err(CctalkTransmissionError::FailedToReciveEcho(
                        msg_bytes[n],
                        buf[0],
                    ));
                } else {
                    println!("read echo: {:02x?}", msg_bytes);
                }
            }
        }

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
            Ok(msg) => return Ok(msg),
            Err(e) => {
                println!(
                    "rx bytes are not a valid msg: {:02x?}",
                    &rx_buf[0..packet_size]
                );
                return Err(CctalkTransmissionError::FailedToConvertToMessage(e));
            }
        };
    }
}

mod tests {
    use super::*;
    use crate::headers::CcTalkHeader;
    use heapless::Vec as hVec;

    #[test]
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
            UartTransaction::read_many(rx_bytes),
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
