use crate::errors::CctalkMessageError;

use super::*;

impl<U, D> CctalkDevice<Unprobed, U, D> {
    pub fn new(addr: u8, cctalk: SharedCctalk<U, D>) -> CctalkDevice<Unprobed, U, D>
    where
        U: Read + Write,
        D: DelayNs,
    {
        Self {
            info: DevInfo {
                addr,
                ..Default::default()
            },
            cctalk: cctalk,
            _init_state: PhantomData,
            currancy: None,
            r#type: Default::default(),
            encrypted: Default::default(),
            events: ECs::default(),
        }
    }

    pub fn probe(mut self) -> Result<CctalkDevice<UnInit, U, D>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let addr = self.info.addr;

        //First determine if unecrypted 8bit or 16bit device
        if let Ok(_res) = self.simp_poll_8(None) {
            #[cfg(feature = "std")]
            std::println!("found 8 bit device!");
            self.r#type = CctalkEncStatus::Unenc8Bit;
        } else {
            if let Ok(_res) = self.simp_poll_16(None) {
                #[cfg(feature = "std")]
                std::println!("found 16-bit device!");
                self.r#type = CctalkEncStatus::Unenc16bit;
            } else {
                #[cfg(feature = "std")]
                std::println!("Unable to ascertain device chksum type!");
                return Err(CctalkTransmissionError::CctalkMessageError(
                    CctalkMessageError::NoChkSum,
                ));
            };
        };

        //TODO: rewrite so device agnostic, using a universal header_only fn that works for all
        //devtypes (header8 and header16 are only needed for unknown devices after all)
        //then rename header8/16 fns to indicate they are setup only (and only availa)
        //Device type
        let mut res = self.header_only(CcTalkHeader::RequestEquipmentCategory, None)?;

        let kind = CctalkDeviceKind::from(res.data().as_slice());

        if kind == CctalkDeviceKind::Unknown {
            return Err(CctalkTransmissionError::CctalkDeviceTypeUnknown);
        }

        //Encryption Key/Status
        #[cfg(feature = "std")]
        std::println!("fetching encryption information");
        let encrypted = match self.retrieve_enc_key(Some(200)) {
            Ok(val) => val,
            Err(_e) => None,
        };

        #[cfg(feature = "std")]
        std::println!("info: {:?}", encrypted);

        //Manufacturer
        res = self.header_only(CcTalkHeader::RequestManufacturerId, None)?;
        let manu = res.data_str();

        //Model
        res = self.header_only(CcTalkHeader::RequestProductCode, None)?;
        let model = res.data_str();

        let info = DevInfo {
            addr,
            kind,
            manu,
            model,
        };

        Ok(CctalkDevice {
            info,
            currancy: self.currancy,
            cctalk: self.cctalk,
            events: ECs::default(),
            r#type: self.r#type,
            encrypted: encrypted,
            _init_state: PhantomData,
        })
    }

    fn simp_poll_16(
        &mut self,
        timeout_ms: Option<u32>,
    ) -> Result<CctalkMessage, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let msg = CctalkMessage::new(
            CctalkEncStatus::Unenc16bit,
            None,
            self.info.addr,
            CcTalkHeader::SimplePoll,
            hVec::new(),
        );

        let rx_bytes = self.transfer(msg, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;
        let msg = CctalkMessage::from_bytes(CctalkEncStatus::Unenc16bit, rx_bytes)?;

        if msg.header() != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(msg.header()));
        };

        Ok(msg)
    }

    fn simp_poll_8(
        &mut self,
        timeout_ms: Option<u32>,
    ) -> Result<CctalkMessage, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let msg = CctalkMessage::new(
            CctalkEncStatus::Unenc8Bit,
            Some(MASTER_ADDR),
            self.info.addr,
            CcTalkHeader::SimplePoll,
            hVec::new(),
        );

        let rx_bytes = self.transfer(msg, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;
        let msg = CctalkMessage::from_bytes(CctalkEncStatus::Unenc8Bit, rx_bytes)?;

        if msg.header() != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(msg.header()));
        };

        Ok(msg)
    }

    fn retrieve_enc_key(
        &mut self,
        // addr: u8,
        timeout_ms: Option<u32>,
    ) -> Result<Option<CcTalkEncryptionKeys>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let msg = CctalkMessage::new(
            self.r#type.clone(),
            Some(MASTER_ADDR),
            self.info.addr,
            CcTalkHeader::RequestEncryptionSupport,
            hVec::from_array(REQ_ENC_SUPPORT_BYTES),
        );

        match self.transfer(msg, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)) {
            Ok(rx_bytes) => {
                // println!("enc bytes: {:?}", rx_bytes);
                if rx_bytes.len() == 17 {
                    //packet layout consists of 17 bytes (see struct below for
                    //layout )
                    match CctalkMessage::from_bytes(self.r#type.clone(), rx_bytes) {
                        Ok(msg) => {
                            let data = msg.data().as_slice();
                            return Ok(Some(CcTalkEncryptionKeys {
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
                        Err(e) => Err(e),
                    }
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                // println!("enc bytes error: {}", e);
                Err(e)
            }
        }
    }
}
