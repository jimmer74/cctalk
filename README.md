# CCtalk library

Made by hand in rust.

## Goals

Initially had an idea to make a coin mech project run off a pi for fun. 

Then realised that I could hive off the cctalk part into it's own library and continue to work on it there.

I was using rpi-pal crate to develop directly on the pi, but that became painful.

Since rpi-pal supports the embedded-hal stuff, I used the traits from that to divorce it from rpi-pal.

That way the library can be worked on, compiled and tested successfully on my laptop (and it can also be ported to be used on a microcontroller down the line.)

Eventually I will need to make it possible to use nostd, but will attempt to just get it working for tx/rx first then attempt that afterwards!

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


Non open-collector TX on raspberry pi needs following to enable 2-wire
to 1-wire cctalk device. D should be a fast switching schottky (low forward voltage drop)

           PWR
            ▲                            3v3
            │                             ▲
            │                             │
            │                            ┌┴┐
┌───────────┴────────────┐               │ │
│                        │               │ │ 10K
│                        │               └┬┘
│                     TX ┼───────── D ────┤
│                        │       ◄─────   │
│           uC           │                │
│                        │                │
│                        │                │
│                     RX ┼────────────────┴─────── CCtalk Device
│                        │
│                        │
└────────────┬───────────┘
             │
             │
             │
             ▼
            GND



```
