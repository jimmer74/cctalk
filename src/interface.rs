extern crate alloc;
extern crate spin;

use alloc::sync::Arc;
use spin::{Mutex, MutexGuard};

use super::headers::CcTalkHeader;
use crate::{device::CctalkEncStatus, errors::CctalkTransmissionError, message::CctalkMessage};
use embedded_hal::delay::DelayNs;
use embedded_hal_nb::serial::{Read, Write};
use heapless::Vec as hVec;
use nb::block;

const ADDR_POL: [u8; 5] = [0x00, 0x00, 0x01, CcTalkHeader::AddressPoll as u8, 0x02];

#[derive(Debug, Clone, Default, PartialEq, PartialOrd)]
pub struct Cctalk<U, D> {
    uart: U,
    delay: D,
    pub echo: bool,
}

impl<U, D> Cctalk<U, D>
where
    U: Read + Write,
    D: DelayNs,
{
    pub fn new(uart: U, delay: D, echo: bool) -> Self {
        Self { uart, delay, echo }
    }

    pub fn read_bytes(
        &mut self,
        timeout_ms: u32,
    ) -> Result<hVec<u8, 260>, CctalkTransmissionError> {
        let mut n = 0_usize;
        let mut rx_buf: hVec<u8, 260> = hVec::new();
        const POLL_INTERVAL_MS: u32 = 1_u32;
        let mut elapsed_ms = 0_u32;
        loop {
            match self.uart.read() {
                Ok(byte) => {
                    elapsed_ms = 0_u32; //reset timeout after each successful read
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
                    elapsed_ms = 0_u32; //reset timeout after successful read;
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

    pub fn write_bytes<T: Into<hVec<u8, 260>>>(
        &mut self,
        bytes: T,
    ) -> Result<(), CctalkTransmissionError> {
        let bytes = bytes.into();
        let data_slice = bytes.as_slice();
        for dat in data_slice {
            let _ = block!(self.uart.write(*dat))
                .map_err(|_e| CctalkTransmissionError::CctalkSerialWriteError);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)] // Deriving Clone is cheap because it just clones the Arc pointer
pub struct SharedCctalk<U, D>(Arc<Mutex<Cctalk<U, D>>>);

impl<U, D> SharedCctalk<U, D>
where
    U: Read + Write,
    D: DelayNs,
{
    pub fn new(uart: U, delay: D, echo: bool) -> Self {
        let cctalk = Cctalk::new(uart, delay, echo);
        Self(Arc::new(Mutex::new(cctalk)))
    }

    pub fn lock(&self) -> MutexGuard<'_, Cctalk<U, D>> {
        self.0.lock()
    }

    /***************************************************************
     *
     *
     *                  Addr scan functions
     *
     *
     *****************************************************************/

    pub fn addr_scan(&self, timeout_ms: u32) -> Result<hVec<u8, 260>, CctalkTransmissionError> {
        let mut cctalk = self.0.lock();

        let _ = cctalk.write_bytes(hVec::from_array(ADDR_POL))?;

        if cctalk.echo {
            match cctalk.read_bytes_exact(20, ADDR_POL.len()) {
                Ok(rx_bytes) => {
                    if rx_bytes != ADDR_POL {
                        // println!("echo: {:?} does not match {:?}", rx_bytes, ADDR_POL);
                        return Err(CctalkTransmissionError::FailedToReciveEcho);
                    }
                }
                Err(_e) => {
                    // println!("error: {}", e);
                    return Err(CctalkTransmissionError::CctalkSerialReadError);
                }
            }
        }

        match cctalk.read_bytes(timeout_ms) {
            Ok(bytes) => return Ok(bytes),
            Err(e) => return Err(e),
        }
    }

    pub fn addr_scan_16(&self, timeout_ms: u32) -> Result<hVec<u8, 260>, CctalkTransmissionError> {
        let mut cctalk = self.lock();

        let tx = CctalkMessage::new(
            CctalkEncStatus::Unenc16bit,
            None,
            0x00,
            CcTalkHeader::AddressPoll,
            hVec::new(),
        );

        let _ = cctalk.write_bytes(tx.clone())?;

        if cctalk.echo {
            match cctalk.read_bytes_exact(20, tx.clone().packet_len()) {
                Ok(rx_msg) => {
                    let tx_bytes: hVec<u8, 260> = tx.into();
                    if rx_msg != tx_bytes {
                        return Err(CctalkTransmissionError::FailedToReciveEcho);
                    }
                }
                Err(_e) => return Err(CctalkTransmissionError::CctalkSerialReadError),
            }
        }

        match cctalk.read_bytes(timeout_ms) {
            Ok(bytes) => return Ok(bytes),
            Err(e) => return Err(e),
        }
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
        let mut cctalk = SharedCctalk::new(uart.clone(), timer, true);
        let mut cctalk = cctalk.lock();
        let result = cctalk.read_bytes_exact(5, 5);

        assert_ne!(result.unwrap(), rx_bytes); // WARN: left 4 bytes, right 5 bytes - currently no error
        // handling on fn. Need to implement and revise!

        uart.done();

        drop(cctalk);
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
        let mut cctalk = SharedCctalk::new(uart.clone(), timer, true);
        let mut cctalk = cctalk.lock();
        let result = cctalk.read_bytes(2);

        assert_eq!(result.unwrap(), rx_bytes);

        uart.done();
    }
}
