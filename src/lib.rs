pub mod errors;
pub mod headers;
pub mod interface;
pub mod message;
pub mod tx;

#[cfg(test)]
mod tests {
    // use embedded_hal_mock::eh0::i2c::Transaction;

    use super::*;

    // add embedded_hal_mock here for tests;
    // #[test]
    // fn test_send_cctalk() {
    //     use embedded_hal_mock::eh1::serial::{Mock as UartMock, Transaction as UartTransaction};
    //     use embedded_hal_nb::serial::{Read, Write};
    //
    //     let testcase = CctalkMessage {
    //         dest: 0x02,
    //         len: 0x00,
    //         src: 0x01,
    //         header: CcTalkHeader::SimplePoll as u8,
    //         data: hVec::new(),
    //         chksum: Some(0xFF),
    //     };
    //     let msg = testcase.try_to_bytes().unwrap();
    //
    //     let expectations = [
    //         UartTransaction::write_many(testcase.try_to_bytes().unwrap()),
    //         UartTransaction::read_many(testcase.try_to_bytes().unwrap()),
    //     ];
    //
    //
    //     let uart = UartMock::new(&expectations);
    //     let cctalk = Cctalk::new(uart, false);
    // }

    #[test]
    fn test_new_cctalk() {
        //this is message that new should construct
        let testcase = CctalkMessage {
            dest: 0x02,
            len: 0x00,
            src: 0x01,
            header: CcTalkHeader::SimplePoll as u8,
            data: hVec::new(),
            chksum: Some(0xFF),
        };

        assert_eq!(
            testcase,
            CctalkMessage::new(0x02, 0x01, CcTalkHeader::SimplePoll, hVec::new())
        );
    }

    #[test]
    fn test_bad_cctalk() {
        //this is an incorrect message (checksum should be wrong)
        let testcase = CctalkMessage {
            dest: 0x02,
            //new fn auto-calcs len, need to supply manually here
            len: 0x02,
            src: 0x01,
            //new fn converts this on fly, but have to do manually here
            header: CcTalkHeader::DispenseHopperCoins as u8,
            data: hVec::from_array([0x02, 0x02]),
            chksum: Some(0x22),
        };

        //check new() does not match above (i.e. checksum is wrong as expected)
        assert_ne!(
            testcase,
            CctalkMessage::new(
                0x02,
                0x01,
                CcTalkHeader::DispenseHopperCoins,
                hVec::from_array([0x02, 0x02])
            )
        );
    }

    #[test]
    fn test_wrong_chksum() {
        let testcase = CctalkMessage {
            dest: 0x02,
            len: 0x02,
            src: 0x01,
            header: CcTalkHeader::SimplePoll as u8,
            data: hVec::from([0x2, 0x0]),
            chksum: Some(0xFF),
        };

        println!("{}", testcase.calc_chksum());
        let res = testcase.chksum_valid();
        match res {
            Err(e) => {
                assert_eq!(
                    String::from("Wrong chksum: 255, should be: 251"),
                    format!("{}", e)
                )
            }
            _ => {}
        };
    }

    #[test]
    fn test_wrong_len() {
        let testcase = CctalkMessage {
            dest: 0x02,
            len: 0x00,
            src: 0x01,
            header: CcTalkHeader::SimplePoll as u8,
            data: hVec::from_array([0x2, 0x0]),
            chksum: Some(0xFF),
        };
        let res = testcase.len_valid();
        match res {
            Err(e) => {
                assert_eq!(
                    String::from("Wrong data length: 0, should be: 2"),
                    format!("{}", e)
                )
            }
            _ => {}
        };
    }

    #[test]
    fn test_to_bytes() {
        let testcase = CctalkMessage::new(
            0x02,
            0x01,
            CcTalkHeader::UploadCalibrationData,
            hVec::from_array([0x34, 0xFF, 0x98, 0x13]),
        );

        assert_eq!(
            testcase
                .try_to_bytes()
                .expect("error couldn't convert test msg to bytes - fix the test!'"),
            hVec::<u8, 255>::from_array([
                0x02,                                      //dest
                0x04,                                      //data len - 4 bytes
                0x01,                                      //source
                CcTalkHeader::UploadCalibrationData as u8, //header
                0x34,                                      // data byte 1
                0xFF,                                      // data byte 2
                0x98,                                      // data byte 3
                0x13,                                      // data byte 4
                0x53                                       // chksum
            ])
        )
    }

    #[test]
    fn test_from_bytes() {
        let cct = CctalkMessage::new(
            0x02,
            0x01,
            CcTalkHeader::SimplePoll,
            hVec::from_array([0x22, 0x01]),
        );

        let data = cct
            .try_to_bytes()
            .expect("error couldn't convert test msg to bytes - fix the test!'");

        match CctalkMessage::from_bytes(&data) {
            Ok(new_cct) => {
                assert_eq!(cct, new_cct);
            }
            Err(e) => {
                assert_eq!(e, CctalkMessageError::NoChkSum)
            }
        }
    }
}
