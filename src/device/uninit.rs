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
        if let Ok(res) = self.header_only8(self.info.addr, CcTalkHeader::SimplePoll, None) {
            #[cfg(feature = "std")]
            std::println!("found 8 bit device!");
            self.r#type = CctalkEncStatus::Unenc8Bit;
        } else {
            if let Ok(res) = self.header_only16(self.info.addr, CcTalkHeader::SimplePoll, None) {
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

        match self.r#type {
            CctalkEncStatus::Unenc8Bit => {
                //Device type
                let mut res = self.header_only8(
                    self.info.addr,
                    CcTalkHeader::RequestEquipmentCategory,
                    None,
                )?;

                let kind = CctalkDeviceKind::from(res.data().as_slice());

                if kind == CctalkDeviceKind::Unknown {
                    return Err(CctalkTransmissionError::CctalkDeviceTypeUnknown);
                }

                //Encryption Key/Status
                let encrypted = match self.retrieve_enc_key(Some(200)) {
                    Ok(val) => val,
                    Err(_e) => None,
                };

                //Manufacturer
                res =
                    self.header_only8(self.info.addr, CcTalkHeader::RequestManufacturerId, None)?;
                let manu = res.data_str();

                //Model
                res = self.header_only8(self.info.addr, CcTalkHeader::RequestProductCode, None)?;
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
            CctalkEncStatus::Unenc16bit => {
                //Device type
                let mut res = self.header_only16(
                    self.info.addr,
                    CcTalkHeader::RequestEquipmentCategory,
                    None,
                )?;

                let kind = CctalkDeviceKind::from(res.data().as_slice());

                if kind == CctalkDeviceKind::Unknown {
                    return Err(CctalkTransmissionError::CctalkDeviceTypeUnknown);
                }

                //Encryption Key/Status
                let encrypted = match self.retrieve_enc_key(Some(200)) {
                    Ok(val) => val,
                    Err(_e) => None,
                };

                //Manufacturer
                res =
                    self.header_only16(self.info.addr, CcTalkHeader::RequestManufacturerId, None)?;
                let manu = res.data_str();

                //Model
                res = self.header_only16(self.info.addr, CcTalkHeader::RequestProductCode, None)?;
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
            _ => Err(CctalkTransmissionError::CctalkMessageError(
                CctalkMessageError::NoChkSum,
            )),
        }
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
            if self.r#type == CctalkEncStatus::Unenc8Bit {
                Some(MASTER_ADDR)
            } else {
                None
            },
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
                    match CctalkMessage::msg_from_bytes(self.r#type.clone(), rx_bytes) {
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
