use super::{
    BILL_EVENT_BUFF_LEN, CcTalkHeader, CctalkDevice, CctalkTransmissionError, DEFAULT_TIMEOUT_MS,
    DelayNs, Init, InitStatus, MASTER_ADDR, Msg8, PhantomData, Read, UnInit, Unenc8Bit, Write,
    hVec,
};
/*
 *
 *      Only for initialised 8bit devices
 *
 * */
impl<U, D> CctalkDevice<Unenc8Bit, Init, U, D> {
    pub fn read_buff_events(
        &mut self,
        timeout_ms: Option<u32>,
    ) -> Result<[u8; BILL_EVENT_BUFF_LEN], CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        let res = self.header_only(self.addr, CcTalkHeader::ReadBufferedBillEvents, timeout_ms)?;
        let header = res.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };

        let dat = res.data()[0..BILL_EVENT_BUFF_LEN]
            .try_into()
            .expect("error buff events ret value not 11 bytes long");

        Ok(dat)
    }
}

/*
 *
 *  8bit devices that have been probed, but not initialised
 *
 */

impl<U, D> CctalkDevice<Unenc8Bit, UnInit, U, D>
where
    U: Read + Write,
    D: DelayNs,
{
    pub fn init(
        mut self,
        // cctalk: &mut Cctalk<UART, DELAY>,
    ) -> Result<CctalkDevice<Unenc8Bit, Init, U, D>, CctalkTransmissionError>
    where
        D: DelayNs,
        U: Read + Write,
    {
        /*
         *
         *      Get all note slots
         *
         * */

        let mut country_code: [u8; 2] = [0x00, 0x00];
        for i in 1..=16 {
            let mut payload: hVec<u8, 255> = hVec::new();
            _ = payload.push(i);
            let msg = Msg8::new(self.addr, MASTER_ADDR, CcTalkHeader::RequestBillId, payload);
            let tx_bytes = msg
                .try_to_bytes()
                .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

            let rx_bytes = self.transfer(tx_bytes, 80)?;
            let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
                .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
            let dat = msg.data().clone();
            if *dat != [0, 0, 0, 0, 0, 0, 0] {
                println!("Note Slot {}: {:?}/{:?}", i, msg.data(), msg.data_str());

                for (j, _) in country_code.clone().iter().enumerate() {
                    if j < 2 {
                        country_code[j] = dat[j];
                    }
                }
            } else {
                println!("Note slot {} is unoccupied", i);
            }
        }
        println!(
            "Country code: {:?} / {}",
            country_code,
            String::from_utf8_lossy(&country_code[..])
        );

        /*
         *
         *      Request Country Scaling Factor
         *
         * */

        let payload: hVec<u8, 255> = hVec::from_array(country_code);
        let msg = Msg8::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::RequestCountryScalingFactor,
            payload,
        );
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, 80)?;

        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let dat = msg.data();

        println!("RCSF: {:?}", dat);

        /*
         *
         *      Request Currency Revision
         *
         * */
        let payload: hVec<u8, 255> = hVec::new(); //hVec::from_array(country_code);
        let msg = Msg8::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::RequestCurrencyRevision,
            payload,
        );

        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        match self.transfer(tx_bytes, 80) {
            Ok(bytes) => {
                let msg = Msg8::try_from_bytes(bytes.as_slice()).ok();
                if let Some(msg) = msg {
                    println!("Currency Rev: {:?}", String::from_utf8_lossy(msg.data()));
                } else {
                    println!("Error Curr Rev failed to recieve: {:?}", bytes);
                }
            }
            Err(e) => {
                println!("error curr revision: {}", e)
            }
        }

        /*
         *
         *      Modify Bill Operating Mode
         *
         * */

        let payload: hVec<u8, 255> = if self.model == String::from("NV10") {
            hVec::from_array([0x00])
        } else if self.model == String::from("NV9") {
            hVec::from_array([0x01])
        } else {
            hVec::from_array([0x01])
        };
        let msg = Msg8::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyBillOperatingMode,
            payload,
        );
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, 80)?;
        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
        println!("mod bill op mode: {}", msg.header());

        /*
         *
         *      Modify Inhibit Status
         *
         * */
        let payload: hVec<u8, 255> = hVec::from_array([0xFF, 0xFF]);
        let msg = Msg8::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyInhibitStatus,
            payload,
        );
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, 80)?;
        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        println!("mod master inhibit: {}", msg.header());

        /*
         *
         *      Modify Master Inhibit Status
         *
         * */

        let payload: hVec<u8, 255> = hVec::from_array([0x01]); //hVec::from_array(country_code);
        let msg = Msg8::new(
            self.addr,
            MASTER_ADDR,
            CcTalkHeader::ModifyMasterInhibitStatus,
            payload,
        );
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, 80)?;
        let msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
        println!("mod master inhibit: {}", msg.header());

        // drop(cctalk);

        Ok(CctalkDevice {
            addr: self.addr,
            kind: self.kind,
            manu: self.manu,
            model: self.model,
            cctalk: self.cctalk,
            chksum: self.chksum,
            encrypted: self.encrypted,
            event_counter: self.event_counter,
            last_event: self.last_event,
            _enc_state: PhantomData,
            _init_state: PhantomData,
        })
    }
}

/*
 *
 *
 * Any 8bit device, in any state of initialisation
 *
 *
 * */
impl<I, U, D> CctalkDevice<Unenc8Bit, I, U, D>
where
    I: InitStatus,
    U: Read + Write,
    D: DelayNs,
{
    pub fn header_only(
        &mut self,
        addr: u8,
        header: CcTalkHeader,
        timeout_ms: Option<u32>,
    ) -> Result<Msg8, CctalkTransmissionError>
    where
        I: InitStatus,
    {
        let msg = Msg8::new(addr, MASTER_ADDR, header, hVec::new());
        let tx_bytes = msg
            .try_to_bytes()
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;

        let rx_bytes = self.transfer(tx_bytes, timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))?;

        let rx_msg = Msg8::try_from_bytes(rx_bytes.as_slice())
            .map_err(|e| CctalkTransmissionError::CctalkMessageError(e))?;
        let header = rx_msg.header();
        if header != CcTalkHeader::Ack {
            return Err(CctalkTransmissionError::CctalkFailedToAck(header));
        };
        Ok(rx_msg)
    }
}
