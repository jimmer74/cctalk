use crate::CctalkMessage;
use crate::errors::{CctalkMessageError, CctalkTransmissionError};

pub struct Cctalk<UART> {
    uart: UART,
    echo: bool,
}

impl<UART> Cctalk<UART>
where
    UART: embedded_io::Read + embedded_io::Write,
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

        match self.uart.write_all(&msg_bytes) {
            Ok(_) => {}
            Err(_) => return Err(CctalkTransmissionError::FailedToTxData),
        }

        match self.uart.flush() {
            Ok(_) => {}
            Err(_) => return Err(CctalkTransmissionError::FailedToTxData),
        }

        if self.echo {
            let mut buf = [0u8; 1];
            for n in 0..msg_bytes.len() {
                _ = self.uart.read_exact(&mut buf);
                if buf[0] != msg_bytes[n] {
                    return Err(CctalkTransmissionError::FailedToReciveEcho(
                        msg_bytes[n],
                        buf[0],
                    ));
                }
            }
        }

        let mut rx_buf: [u8; 260] = [0u8; 260];

        //grab 1st four [dest, data_len, src. header]
        match self.uart.read_exact(&mut rx_buf[0..=3]) {
            Ok(_) => {}
            Err(_) => return Err(CctalkTransmissionError::FailedToFetchHeader),
        };

        let data_len = rx_buf[1] as usize;

        let packet_size = 4 + data_len + 1; // [(dest, data_len, src. header), data[..], chksum ]

        match self.uart.read_exact(&mut rx_buf[4..packet_size]) {
            Ok(_) => {}
            Err(_) => return Err(CctalkTransmissionError::FailedToRxDataAndChksum),
        };
        match CctalkMessage::from_bytes(&rx_buf) {
            Ok(msg) => return Ok(msg),
            Err(e) => return Err(CctalkTransmissionError::FailedToConvertToMessage(e)),
        };
    }
}
