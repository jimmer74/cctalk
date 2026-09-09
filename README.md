# CCtalk library

Made by hand in rust.

## Goals

Initially had an idea to make a coin mech project run off a pi for fun. 

Then released that I could hive off the cctalk part into it's own library and continue to work on it there.

I was using rpi-pal crate to develop directly on the pi, but that became painful.

Since rpi-pal supports the embedded-hal stuff, I used the traits from that to divorce it from rpi-pal.

That way it can be compiled and tested successfully on my laptop and it can be ported to be used a microcontroller down the line.

Will need to make it possible to use nostd, but will just get it working 1st then attempt that afterwards!


  
