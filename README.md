# Open DMX
Open DMX is a rust implementation to drive [Enttec Open DMX USB](https://www.enttec.com/product/lighting-communication-protocols/dmx512/open-dmx-usb) devices from your application. It is based on the [libftd2xx](https://crates.io/crates/libftd2xx) crate whichs is a wrapper around the API provided by FTDI.

## Prerequists
Make sure to install the [FTDI drivers](https://ftdichip.com/drivers/d2xx-drivers/). FTDI-Chips is what Enttec used to build its device.

## Tests
There are Unit-Test for this crate. However, these tests cannot be run in parallel because the crate needs exclusive access to the device. So make sure to run unit test with appropriate parameter like:
`cargo test -- --test-threads=1`

Furthermore keep in mind, that some of the test will fail if no or multiple devices are connected to your machine.