# CCtalk library

Made by hand in rust.

## Goals

Initially had an idea to make a coin mech project run off a pi for fun. 

Then realised that I could hive off the cctalk part into it's own library and continue to work on it there.

I was using rpi-pal crate to develop directly on the pi, but that became painful.

Since rpi-pal supports the embedded-hal stuff, I used the traits from that to divorce it from rpi-pal.

That way the library can be worked on, compiled and tested successfully on my laptop (and it can also be ported to be used on a microcontroller down the line.)

Eventually I will need to make it possible to use nostd, but will attempt to just get it working for tx/rx first then attempt that afterwards!


  
