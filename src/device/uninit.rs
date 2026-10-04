use super::*;

impl<U, D> CctalkDevice<NoChksum, Unprobed, U, D> {
    pub fn new(addr: u8, cctalk: SharedCctalk<U, D>) -> CctalkDevice<NoChksum, Unprobed, U, D>
    where
        U: Read + Write,
        D: DelayNs,
    {
        Self {
            addr,
            cctalk: cctalk,
            _enc_state: PhantomData,
            _init_state: PhantomData,
            kind: Default::default(),
            manu: Default::default(),
            model: Default::default(),
            currancy: None,
            chksum: Default::default(),
            encrypted: Default::default(),
            event_counter: Default::default(),
            last_event: Default::default(),
        }
    }

    pub fn probe(mut self) -> Result<CctalkDevice<Unenc8Bit, UnInit, U, D>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let addr = self.addr;
        let mut res = self.header_only(self.addr, CcTalkHeader::SimplePoll, None)?;
        _ = res;

        //Device type
        res = self.header_only(self.addr, CcTalkHeader::RequestEquipmentCategory, None)?;

        let kind = CctalkDeviceKind::from(res.data().as_slice());

        if kind == CctalkDeviceKind::Unknown {
            return Err(CctalkTransmissionError::CctalkDeviceTypeUnknown);
        }

        //Encryption Key/Status
        let encrypted = match self.retrieve_enc_key(self.addr, Some(200)) {
            Ok(val) => val,
            Err(_e) => EncKey::CctalkUnEncrypted,
        };

        //Manufacturer
        res = self.header_only(self.addr, RequestManufacturerId, None)?;
        let manu = String::from_utf8(res.data().to_vec()).unwrap();

        //Model
        res = self.header_only(self.addr, CcTalkHeader::RequestProductCode, None)?;
        let model = String::from_utf8(res.data().to_vec()).unwrap();

        Ok(CctalkDevice {
            addr: addr,
            kind: kind,
            manu: manu,
            model: model,
            currancy: self.currancy,
            cctalk: self.cctalk,
            event_counter: EventCounter::default(),
            last_event: EventCounter::default(),
            chksum: CctalkDeviceCRC::Simple8bit,
            encrypted: encrypted,
            _enc_state: PhantomData,
            _init_state: PhantomData,
        })
    }

    pub fn header_only(
        &mut self,
        addr: u8,
        header: CcTalkHeader,
        timeout_ms: Option<u32>,
    ) -> Result<Msg8, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let msg = Msg8::new(addr, MASTER_ADDR, header, hVec::new());
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;
        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        if msg.header() != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };

        Ok(msg)
    }

    pub fn probe16(
        mut self,
    ) -> Result<CctalkDevice<Unenc16Bit, UnInit, U, D>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let addr = self.addr;
        #[allow(unused_assignments)]
        let mut res = self.header_only(addr, SimplePoll, None)?;

        //Device type
        res = self.header_only(addr, RequestEquipmentCategory, None)?;

        let kind = CctalkDeviceKind::from(res.data().as_slice());

        if kind == CctalkDeviceKind::Unknown {
            return Err(CctalkTransmissionError::CctalkDeviceTypeUnknown);
        }

        //Encryption Key/Status
        let encrypted = self.retrieve_enc_key(addr, Some(200))?;

        //Manufacturer
        res = self.header_only(addr, RequestManufacturerId, None)?;
        let manu = String::from_utf8(res.data().to_vec()).unwrap();

        //Model
        res = self.header_only(addr, CcTalkHeader::RequestProductCode, None)?;
        let model = String::from_utf8(res.data().to_vec()).unwrap();

        Ok(CctalkDevice {
            addr: addr,
            kind: kind,
            manu: manu,
            model: model,
            cctalk: self.cctalk,
            currancy: self.currancy,
            chksum: CctalkDeviceCRC::Simple8bit,
            encrypted: encrypted,
            _enc_state: PhantomData,
            _init_state: PhantomData,
            event_counter: EventCounter::default(),
            last_event: EventCounter::default(),
        })
    }

    fn retrieve_enc_key(
        &mut self,
        addr: u8,
        timeout_ms: Option<u32>,
    ) -> Result<CctalkEncKey, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let msg = Msg8::new(
            addr,
            MASTER_ADDR,
            CcTalkHeader::RequestEncryptionSupport,
            hVec::from_array(REQ_ENC_SUPPORT_BYTES),
        );

        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        match self.transfer(tx_bytes, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)) {
            Ok(rx_bytes) => {
                println!("enc bytes: {:?}", rx_bytes);
                if rx_bytes.len() > 0 {
                    match Msg8::try_from_bytes(rx_bytes.as_slice()) {
                        Ok(msg) => {
                            let data = msg.data().as_slice();
                            return Ok(EncKey::CctalkEncSupport(CcTalkEncryptionStatus {
                                proto_level: data[0],
                                command_level: data[1],
                                proto_key_size: data[2],
                                com_key_size: data[3],
                                com_block_size: data[4],
                                trusted_mode: data[5],
                                bnv214365: [data[6], data[7], data[8]],
                                des: [
                                    data[9], data[10], data[11], data[12], data[13], data[14],
                                    data[15], data[16],
                                ],
                            }));
                        }
                        Err(e) => Err(CctalkTransmissionError::CctalkMessageError(e)),
                    }
                } else {
                    Ok(EncKey::CctalkUnEncrypted)
                }
            }
            Err(e) => {
                println!("enc bytes error: {}", e);
                Err(e)
            }
        }
    }
}
