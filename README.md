# CCtalk library

Made by hand in rust. 

## Current State
You can use this to init a note acceptor (that's using 8-bit/16-bit checksums and is unencrypted) currently. 

Then you can read the note event buffer in a (at most) 200ms loop (or else note acceptor goes back to sleep) for changes in it's event buffer (i.e. inserted note, inserted barcode, strimming attempt, etc).

It's defo pre-release and will hopefully change massively to include:

* BNV encrypted devices (maybe, but not priority)
* DES encryted devices (definately)
* Coinmechs & hoppers (definately coinmechs, not sure about hoppers)
* Card readers (don't have a cctalk one, but will work on it if someone donates one)
* better user experience!

An example (using rpi-pal on a raspberry pi zero):

```
use cctalk::{device::CctalkDevice, interface::SharedCctalk};
use rpi_pal::uart::Uart;
use std::thread::sleep;

const ADDR_SCAN_PAUSE: Duration = Duration::from_millis(1250);
const ADDR_SCAN_TIMEOUT: u32 = 200_u32;


fn main() -> Result<(), Box<dyn Error>> {
    
    let mut uart = Uart::new(9600, rpi_pal::uart::Parity::None, 8, 1).unwrap();
    let _ = uart.set_software_flow_control(false);
    let _ = uart.set_hardware_flow_control(false);
    let _ = uart.set_write_mode(true);
    let delay = rpi_pal::hal::Delay;
    //If your hw setup produces an rx echo everytime you tx
    //(which is normal) then you need to set this to true
    let echo = true;
    let cctalk = SharedCctalk::new(uart, delay, echo);

    println!("scanning cctalk addresses....");
    let addrs = cctalk.addr_scan(ADDR_SCAN_TIMEOUT)?;
    
    //this only cares about the 1st device it detects
    //so will only work if you have a single note acceptor attached
    //currently!

    if addrs.len() > 0 {
       
        println!("received addresses: {:?}", addrs);
        //bus needs loooong delay after address scan
        //before devices will respond again
        //to give all devices chance to answer
        sleep(ADDR_SCAN_PAUSE);
       

        println!("probing addr: {}", addrs[0]);
        let device = CctalkDevice::new(addrs[0], cctalk);
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

        //main polling loop for NV event counter
        let mut last_event: IntEvent = IntEvent::default();
        loop {
            match device.get_int_events() {
                Ok(res) => {
                    if !res.is_empty() {
                        for event in res {
                            match event.clone() {
                                IntEvent::StackedNote(slot) => {
                                    //rejected barcode ticket produces spurious slot 1 acceptance
                                    //event on non-=barcode enabled n/acceptors (e.g NV9/NV10)
                                    if last_event == IntEvent::DevError(BillReturnedFromEscrow) {
                                        println!("ignoring spurious event")
                                    } else {
                                        println!("note slot {slot} stacked");
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
                                }
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
1) showing device setup 
2) showing acceptance of a £10 note 
3) rejecting an unidentified object (barcode ticket))
```
scanning cctalk addresses....
received addresses: [40]
probing addr: 40
found 16-bit device!
device found: ITL Note Acceptor NV10
Note Acceptor NV10 initialised! Ready for use

events: [RawResult { a: 2, b: 1 }]
actioning escrow on note slot 2
events: [RawResult { a: 2, b: 0 }]
note slot 2 stacked

events: [RawResult { a: 0, b: 1 }, RawResult { a: 1, b: 0 }]
event error: BillReturnedFromEscrow
ignoring spurious event
```

## CCTalk Packet description
Standard Packet (in bytes) is:

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
