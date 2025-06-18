# Open DMX
Open DMX is a rust implementation to control [Enttec Open DMX USB](https://www.enttec.com/product/lighting-communication-protocols/dmx512/open-dmx-usb) devices from your application. It is based on the [libftd2xx](https://crates.io/crates/libftd2xx) crate whichs is a wrapper around the API provided by FTDI.

## Prerequists
Make sure to install the [FTDI drivers](https://ftdichip.com/drivers/d2xx-drivers/). FTDI-Chips is what Enttec uses to build its devices.

## Tests
There are Unit-Test for this crate. However, these tests cannot be run in parallel because the crate needs exclusive access to the device. So make sure to run unit test with appropriate parameters like:
`cargo test -- --test-threads=1`

Furthermore keep in mind, that some of the test will fail if no or multiple devices are connected to your machine.

## Entry point
Open DMX devices need continous updates and works with a refresh rates of roughly 40 kHz. So a program has to refresh the device state at a similar refresh rate. If a program does not write often enough to the device it will cause flickering, however too many writes are a waste of resources (like USB Bandwith). So this crate implements a function that starts a background thread, that writes to the device continuosly (the code is a port from [QLC+](https://github.com/mcallegari/qlcplus/blob/master/plugins/dmxusb/src/enttecdmxusbopen.cpp)).

In most cases you should start your device using the 'pub fn run(id : i32) -> Sender<OpenDmxProtocol>' method. It returns a sender struct, which can be used to update device values or stop the background thread (see: OpenDmxProtocol).

## ToDos:
- Implement reading from device
- Implement a method to list all connected devices.
