# CCtalk library

Made by hand in rust. 

## Current State
You can use this to init a note acceptor, that's using 8-bit checksums and is unencrypted currently. 

Then you can read the note event buffer in a 200ms loop (or else note acceptor goes back to sleep) for changes (i.e. inserted note, inserted barcode, strimming attempt, etc).

It's all a bit proto and rough and will change massively to include:

* 16-bit unencrypted devices
* BNV encrypted devices
* DES encryted devices
* Coinmechs & hoppers
* much better abstractions
* more useful functions
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
    let mut cctalk = SharedCctalk::new(uart, delay, true);

    println!("scanning addresses 8bit addresses....");
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
        let device = CctalkDevice::new(addrs[0], &mut cctalk);
        let device = device.probe()?;
        println!("device: {:#?}", device);
        
        let mut device = device.init()?;

        let mut credits = 0_f32;
        loop {
            let res = device.get_buffered_credits()?;

            if !res.is_empty() {
                for credit in res {
                    println!("Accepted £{:.2} note", credit);
                    credits += credit;
                    println!("Total credit: £{:.2}", credits);
                }
            }

            sleep(Duration::from_millis(200));
        }
    }
         
    Ok(())
    
}
```

Terminal Output (showing acceptance of a £10 note):
```
scanning addresses 8bit addresses....
received addresses: [40]
probing addr: 40
enc bytes: [1, 17, 40, 0, 0, 0, 24, 64, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 46]
slot_data: [71, 66, 48, 48, 48, 53, 65], slot_amt: £5
slot_data: [71, 66, 48, 48, 49, 48, 65], slot_amt: £10
slot_data: [71, 66, 48, 48, 50, 48, 65], slot_amt: £20
currency_sf: [100, 0, 2], currency_country: GB
mod bill op mode: Ack
uninhibeted all slots, result: Ack
mod master inhibit: Ack
error: InvalidBillValidation
note_val: 10, multiplier: 100, sf: 1, dp: 100
Accepted £10.00 note
Total credit: £10.00
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
