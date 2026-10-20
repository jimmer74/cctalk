# CCtalk library

Made by hand in rust. 

## Current State
You can use this to init a note acceptor or coin acceptor (that's using 8-bit/16-bit checksums and is unencrypted) currently. 

Then you can read the note/coin event buffer in a (at most) 200ms loop (or else acceptor goes back to sleep) for changes in it's event buffer (i.e. inserted note, coin, inserted barcode, strimming attempt, etc).

It's defo pre-release and will hopefully change massively to include:

* BNV encrypted devices (maybe, but not priority)
* DES encryted devices (definately)
* Hoppers (If I can rework my interface board include a 24v rail)
* Card readers (don't have a cctalk one, but will work on it if someone donates one)
* better ergonomics!

An example (using rpi-pal on a raspberry pi zero):

```
use cctalk::{
    device::{CctalkDevice, Init, event::IntEvent},
    errors::EventError::BillReturnedFromEscrow,
    interface::CctalkDriver,
};
use rpi_pal::{hal::Delay, uart::Uart};
use std::{time::Duration, thread::sleep, error::Error, sync::LazyLock};

const ADDR_SCAN_PAUSE: Duration = Duration::from_millis(1250);
const ADDR_SCAN_TIMEOUT: u32 = 200_u32;

static CCTALK: LazyLock<CctalkDriver<Uart, Delay>> = LazyLock::new(|| {
    use spin::Mutex;
    extern crate alloc;
    use alloc::sync::Arc;

    let mut uart = Uart::new(9600, rpi_pal::uart::Parity::None, 8, 1).unwrap();
    let _ = uart.set_software_flow_control(false);
    let _ = uart.set_hardware_flow_control(false);
    let _ = uart.set_write_mode(true);

    let delay = Delay::new();

    let arc_uart = Arc::new(Mutex::new(uart));
    let arc_delay = Arc::new(Mutex::new(delay));

    CctalkDriver::new(arc_uart, arc_delay, true)
});


fn main() -> Result<(), Box<dyn Error>> {
    
    println!("scanning cctalk addresses....");
    let addrs = cctalk.addr_scan(ADDR_SCAN_TIMEOUT)?;
    
    //have to have at least 1 device, or we will bail...
    if addrs.len() > 0 {
        let mut devices: Vec<CctalkDevice<Init, Uart, Delay>> = vec![];
       
        println!("received addresses: {:?}", addrs);
        //bus needs loooong delay after address scan devices 
        // will respond again to give all devices chance to answer
        sleep(ADDR_SCAN_PAUSE);
       
        //setup all found devices
        for addr in addrs {

            println!("probing addr: {}", addr);
            let device = CctalkDevice::new(addr, CCTALK.clone());
            let device = device.probe()?;
            println!(
                "device found: {} {} {}",
                device.info.manu, device.info.kind, device.info.model
            );

            //enable escrow if you want to respond to notes and then reject
            //valid notes based on some criteria in you event loop
            let escrow = true;
            let mut device = device.init(escrow)?;
            
            println!(
                "{} {} initialised! Ready for use\n",
                device.info.kind, device.info.model
            );
            
            //add it to our collection of devices
            devices.push(device.clone());
        }
       
        //main polling loop for NV event counter
        let mut credits = 0_f32;
        let mut last_event: IntEvent = IntEvent::default();
        loop {
            match device in &mut devices {
                Ok(res) => {
                    if !res.is_empty() {
                        for event in res {
                            match event.clone() {
                                IntEvent::StackedNote(slot) => {
                                    //rejected barcode ticket/note produces spurious slot 1 acceptance
                                    //event on NV9/NV10 - omit if your n/acc doesn't have this problem'
                                    if last_event == IntEvent::DevError(BillReturnedFromEscrow) {
                                        println!("ignoring spurious event")
                                    } else {
                                        let credit =
                                            device.currancy.unwrap().slot_to_currancy(slot);
                                        credits += credit;
                                        println!(
                                            "£{credit:.2} accepted, current credit: £{credits:.2}\n"
                                        );
 
                                    }
                                }
                                //I'm auto-applying escrow acceptance - alter if it fits yout application
                                IntEvent::NoteEscrow(slot) => {
                                    println!("actioning escrow on note slot {slot}");
                                    device.action_escrow(true)?;
                                }
                                //this will only work with stuff like NV200, spectral's etc
                                //cannot be triggered with n/acc's that lack the optics
                                IntEvent::Barcode => println!("barcode detected"),
                                IntEvent::BarcodeEscrow => {
                                    println!("actioning escrow");
                                    device.action_escrow(true)?;
                                },
                                IntEvent::AcceptedCoin { slot, sorter_path } => {
                                    let credit =
                                        device.currancy.unwrap().slot_to_currancy(slot);
                                    credits += credit;
                                    println!(
                                        "£{credit:.2} coin accepted, Sorter Path: {sorter_path:?} current credit: £{credits:.2}\n"
                                    );
                                },
                                //catachall for something went wrong (jammed, fraud, transport error, etc)
                                IntEvent::DevError(e) => println!("event error: {:?}", e),
                            };
                            last_event = event;
                        }
                    }
                }
                Err(e) => {
                    println!("error: {e}");
                }
            };

            sleep(Duration::from_millis(200));
        }
    }

    Ok(())
    
}
```

Terminal Output: 
1) showing device setup (coinmech on 2 and note acceptor on 40)
2) showing acceptance of a £20 note 
3) rejecting an unidentified object (dodgy £5 note!))
4) accepting good £5 note

    credit meter accumalates as money is added
```
scanning cctalk addresses....
received addresses: [2, 40]
probing addr: 2
device found: MEI Coinmech CF952X
Coinmech CF952X initialised! Ready for use

probing addr: 40
device found: ITL Note Acceptor NV10
Note Acceptor NV10 initialised! Ready for use

 actioning escrow on note slot 3
£20 accepted, current credit: £20.00

event error: BillReturnedFromEscrow
ignoring spurious event

actioning escrow on note slot 1
£5 accepted, current credit: £25.00
```

## CCTalk Packet description
A standard 8-Bit chksum cctalk packet (in bytes) is:

[ Dest, Data Len, Src, Header, Data[], Chksum ]

Data is optional, as some commands (like a simple poll) are transmitted with header only and have no data payload. 
Data Len is the length of the data payload only - not the length of the whole packet!

Thus the minimum valid packet length is 5 bytes, so any packet with less than 5 bytes is incorrect.

The maximum length of the data payload is 255, therefore the maximum length of the whole data packet under the spec is 255 + src + len + dest + header + chksum = 260 bytes. Any cctalk packet above 260 bytes is incorrect.

Chksum is calculated by summing all the bytes in the packet and modulo 256 (or cast as a u8). The result is subtracted from 256 to get the chksum.

### Worked example:
we want to send a poll to a coinmech on addr 0x02 and our address is 0x01. 

We are carrying out a simple poll, so our header will be 0xFE (or decimal 254). 

Our tx packet (without chksum) will look like:

[ 0x02, 0x00, 0x01, 0xFE, ???? ]

now to calculate the checksum:

1st result = 0x02 + 0x00 + 0x01 + 0xFE = 0x0101 = 0x01 as a u8
final result = 0x100 (or 256) - 0x01 = 0xFF

so our chksum is 0xFF and our packet to transmit is:

[ 0x02, 0x00, 0x01, 0xFE, 0xFF ]



## Electrical Interface - CF9524e Coinmech

```
 *  pin 1 - ccTalk Data
 *  pin 2 - n/a
 *  pin 3 - n/a
 *  pin 4 - n/a
 *  pin 5 - /RESET - leave floating if not used
 *  pin 6v- n/a
 *  pin 7 - 12-24v dc
 *  pin 8 - GND
 *  pin 9 - Serial mode (low for ccTalk if no dip sw)
 *  pin 10 - reserved (leave floating)

               GND
                │
      ┌────────▼────┐
    2 │  O O O X O  │ 10
      │             │
    1 │  X O X X X  │ 9
      └──▲─┌─▲─▲─▲──┘
         │ └─┼─┤ │
  Data ──┘   │ │ └── Mode
/RESET ──────┘ └──── 12v-24v)

```
