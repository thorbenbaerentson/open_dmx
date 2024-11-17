use std::time::Duration;

// [Enttec]: https://www.enttec.com/product/lighting-communication-protocols/dmx512/open-dmx-usb
// [Enttec Open DMX USB]: https://www.enttec.com/product/lighting-communication-protocols/dmx512/open-dmx-usb
// [libftd2xx]: https://crates.io/crates/libftd2xx

use libftd2xx::{num_devices, DeviceInfo, DeviceStatus, Ftdi, FtdiCommon, StopBits};

const BUFFER_SIZE: usize = 513;

pub struct OpenDMX {
    ftdi: Ftdi,
    buffer: [u8; BUFFER_SIZE],
    info: DeviceInfo,

    baud_rate: u32,
    bits_per_word: libftd2xx::BitsPerWord,
    stop_bits: libftd2xx::StopBits,
    parity_none: libftd2xx::Parity,

    read_time_out: Duration,
    write_time_out: Duration,
}

impl OpenDMX {
    pub fn new(device_id: i32) -> Result<Self, String> {
        let mut ft: Ftdi;
        match Ftdi::with_index(device_id) {
            Ok(d) => {
                ft = d;
            }
            Err(e) => {
                return Err(format!("Could not open ftdi device. Error: {}", e));
            }
        }

        let device_info: DeviceInfo;
        match ft.device_info() {
            Ok(d) => {
                device_info = d;
            }
            Err(e) => {
                return Err(format!("Could read device info. Error: {}", e));
            }
        }

        Ok(OpenDMX {
            ftdi: ft,
            buffer: [0; BUFFER_SIZE],
            info: device_info,
            baud_rate: 250000,
            bits_per_word: libftd2xx::BitsPerWord::Bits8,
            stop_bits: StopBits::Bits2,
            read_time_out: Duration::from_millis(1000),
            write_time_out: Duration::from_millis(1000),
            parity_none: libftd2xx::Parity::No,
        })
    }

    /// Reset the device.
    pub fn reset(&mut self) -> Result<(), String> {
        match self.ftdi.reset() {
            Ok(_) => {}
            Err(e) => return Err(format!("Could not reset device. Error: {}", e)),
        }

        match self.ftdi.set_baud_rate(self.baud_rate) {
            Ok(_) => {}
            Err(e) => return Err(format!("Could not set baud rate. Error: {}", e)),
        };

        match self.ftdi.set_data_characteristics(
            self.bits_per_word,
            self.stop_bits,
            self.parity_none,
        ) {
            Ok(_) => {}
            Err(e) => return Err(format!("Could not set data characteristics. Error: {}", e)),
        };

        match self
            .ftdi
            .set_timeouts(self.read_time_out, self.write_time_out)
        {
            Ok(_) => {}
            Err(e) => return Err(format!("Could not set time outs. Error: {}", e)),
        };

        match self.ftdi.set_flow_control_none() {
            Ok(_) => {}
            Err(e) => return Err(format!("Could not set flow control. Error: {}", e)),
        };

        match self.ftdi.clear_rts() {
            Ok(_) => {}
            Err(e) => return Err(format!("Could not clear rts. Error: {}", e)),
        };

        match self.ftdi.purge_rx() {
            Ok(_) => {}
            Err(e) => return Err(format!("Could not purge (1). Error: {}", e)),
        };

        match self.ftdi.purge_tx() {
            Ok(_) => {}
            Err(e) => return Err(format!("Could not purge (2). Error: {}", e)),
        };

        Ok(())
    }

    /// Set the value of the given channel. The data is not written directly to the device but
    /// buffered until a call to write().
    pub fn set_dmx_value(&mut self, channel: usize, value: u8) -> Result<(), String> {
        if channel >= BUFFER_SIZE {
            return Err("Invalid channel number".to_owned());
        }
        self.buffer[channel] = value;

        Ok(())
    }

    /// Read the value for the given channel from the local buffer. This is not the value stored on
    /// the open_dmx device. In order to read values from the device the local buffer and
    /// the device have to be synchronized first (see self.sync()).
    pub fn get_dmx_value(&self, channel: usize) -> Result<u8, String> {
        if channel >= BUFFER_SIZE {
            return Err("Invalid channel number".to_owned());
        }
        Ok(self.buffer[channel])
    }

    /// Synchornize local buffer with open_dmx device.
    pub fn sync(&mut self) -> Result<(), String> {
        let data = self.read().unwrap();

        for (dst, src) in self.buffer.iter_mut().zip(&data) {
            *dst = *src
        }

        Ok(())
    }

    /// Close the current device. This is automatically called when a dmx device is dropped.
    pub(crate) fn close(&mut self) -> Result<(), String> {
        match self.ftdi.close() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Could close device. Error: {}", e)),
        }
    }

    /// Read current device status.
    pub fn read(&mut self) -> Result<Vec<u8>, String> {
        let size : usize;
        match self.ftdi.queue_status() {
            Ok(s) => { size = s; },
            Err(e) => {
                return Err(format!("Could read queue status. Error: {}", e));
            },
        }

        let mut buf: [u8; 4096] = [0; 4096];
        match self.ftdi.read_all(&mut buf[0..size]) {
            Ok(_) => {
                let r: Vec<u8> = buf.into();
                Ok(r)
            },
            Err(e) => {
                Err(format!("Could read device data. Error: {}", e))
            },
        }
    }

    /// Return the number of devices.
    pub fn get_num_of_devices() -> Result<u32, String> {
        match num_devices() {
            Ok(num) => Ok(num),
            Err(e) => Err(format!(
                "Could not retrieve number of devices. Error: {}",
                e
            )),
        }
    }

    /// Retrieve data about the current device.
    pub fn get_device_info(&self) -> &DeviceInfo {
        &self.info
    }

    /// Get device status from the current device.
    pub fn get_device_status(&mut self) -> Result<DeviceStatus, String> {
        match self.ftdi.status() {
            Ok(d) => return Ok(d),
            Err(e) => {
                return Err(format!("Could read device status. Error: {}", e));
            }
        }
    }

    /// Write local buffer to device.
    pub fn write(&mut self) -> Result<(), String> {
        match self.ftdi.set_break_on() {
            Ok(_) => { },
            Err(e) => {
                return Err(format!("Could not set device break on. Error: {}", e));
            },
        }

        match self.ftdi.set_break_off() {
            Ok(_) => { },
            Err(e) => {
                return Err(format!("Could not set device break off. Error: {}", e));
            },
        }

        match self.ftdi.write_all(&self.buffer) {
            Ok(_) => { Ok(()) },
            Err(e) => {
                return Err(format!("Could not write data to device. Error: {}", e));
            },
        }
    }

    /// Reset the buffer to zero 
    pub fn reset_buffer(&mut self) {
        self.buffer = [0; BUFFER_SIZE];
    }
}

/// A device must be closed once its not used anymore. If not, the device will be blocked.
impl Drop for OpenDMX {
    fn drop(&mut self) {
        match self.close() {
            Ok(_) => {}
            Err(e) => {
                println!("Could not close open_dmx device. Error: {}", e);
            }
        }
    }
}

/// Tests cannot run in parallel, because in most cases we got only one device and
/// this library needs exclusive access to the device.
///
/// Run tests with:
/// cargo test -- --nocapture --test-threads=1
#[cfg(test)]
mod tests {
    use libftd2xx::DeviceType;

    use super::*;

    #[test]
    fn num_devices_test() {
        let subject = OpenDMX::get_num_of_devices().unwrap();
        assert_eq!(subject, 1);
    }

    #[test]
    fn local_buffer_test() {
        let mut subject = OpenDMX::new(0).unwrap();
        // Check default
        assert_eq!(subject.get_dmx_value(0).unwrap(), 0);

        // Set a value...
        subject.set_dmx_value(0, 1).unwrap();
        assert_eq!(subject.get_dmx_value(0).unwrap(), 1);

        // ... overwrite the value again.
        subject.set_dmx_value(0, 0).unwrap();
        assert_eq!(subject.get_dmx_value(0).unwrap(), 0);

        // Test invalid channel numbers.
        let e = subject.set_dmx_value(BUFFER_SIZE, 10);
        assert_eq!(e, Err("Invalid channel number".to_owned()));

        let e2 = subject.get_dmx_value(BUFFER_SIZE);
        assert_eq!(e2, Err("Invalid channel number".to_owned()));
    }

    #[test]
    fn sync_test() {
        let mut subject = OpenDMX::new(0).unwrap();
        // Open device
        subject.reset().unwrap();

        // Check default
        assert_eq!(subject.get_dmx_value(0).unwrap(), 0);

        // Write a value ...
        subject.set_dmx_value(0, 1).unwrap();
        assert_eq!(subject.get_dmx_value(0).unwrap(), 1);

        // Sync data with device. Should reset the local buffer to zero again
        subject.sync().unwrap();

        // Check default
        assert_eq!(subject.get_dmx_value(0).unwrap(), 0);
    }

    #[test]
    #[should_panic]
    fn multiple_devices_test() {
        let _subject1 = OpenDMX::new(0).unwrap();
        // Should panic here. A device can only be opened once.
        let _subject2 = OpenDMX::new(0).unwrap();
    }

    /// This test might fail with different version of open_dmx hardware.
    #[test]
    pub fn device_info_test() {
        let mut subject = OpenDMX::new(0).unwrap();
        // Open device
        subject.reset().unwrap();

        let info = subject.get_device_info();
        assert_eq!("FT232R USB UART".to_owned(), info.description);
        assert_eq!("AL05O9B5".to_owned(), info.serial_number);
        assert_eq!(DeviceType::FT232R, info.device_type);
    }

    #[test]
    pub fn device_status_test() {
        let mut subject = OpenDMX::new(0).unwrap();
        // Open device
        subject.reset().unwrap();

        // Without data send all values should be zero.
        let status = subject.get_device_status().unwrap();
        assert_eq!(0, status.ammount_in_rx_queue);
        assert_eq!(0, status.ammount_in_tx_queue);
        assert_eq!(0, status.event_status);
    }

    #[test]
    pub fn write_data_test() {
        let mut subject = OpenDMX::new(0).unwrap();
        // Open device
        subject.reset().unwrap();

        let pause = 100;
        let r : u8 = 255;
        let g : u8 = 10;
        let b : u8 = 10;

        subject.set_dmx_value(1, r).unwrap();
        subject.set_dmx_value(2, g).unwrap();
        subject.set_dmx_value(3, b).unwrap();

        subject.write().unwrap();

        // Reset the buffer...
        subject.reset_buffer();
        // ... and sync again with device.
        subject.sync().unwrap();

        // Give driver some time to write data.
        std::thread::sleep(std::time::Duration::from_millis(pause));
    }
}
