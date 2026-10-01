use super::{
    CcTalkHeader, CctalkDevice, CctalkTransmissionError, DEFAULT_TIMEOUT_MS, DelayNs, InitStatus,
    Msg16, Read, Unenc16Bit, Write, hVec,
};

impl<I, U, D> CctalkDevice<Unenc16Bit, I, U, D>
where
    I: InitStatus,
    U: Read + Write,
    D: DelayNs,
{
    #[allow(unused)]
    fn header_only(
        &mut self,
        addr: u8,
        header: CcTalkHeader,
        timeout_ms: Option<u32>,
    ) -> Result<Msg16, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let msg = Msg16::new(addr, header, hVec::new());
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;

        let msg = Msg16::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let header = msg.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };
        Ok(msg)
    }
}
