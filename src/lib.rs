use libftd2xx::{list_devices, num_devices, DeviceInfo, DeviceStatus, Ftdi, FtdiCommon, StopBits};
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

const BUFFER_SIZE: usize = 513;
const DMX_BREAK: u64 = 110;
const DMX_MAB: u64 = 16;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimerGranularity {
    #[default]
    Unknown,
    Good,
    Bad,
}

/// Commands that are being send to or from the dmx device across multiple threads.
#[derive(Debug)]
pub enum OpenDmxProtocol {
    /// Send to the device. Changes the channel x to value y
    SetValue(usize, u8),
    /// Send to device. Stop the thread. This will free the device as well.
    Stop,
    /// Send to device. Reset the device.
    Reset,
    /// Send to device. Set the entire buffer to zero
    ResetBuffer,
    /// Send to device. Lists all available devices.
    ListDevices,
    /// Returned from device. A list of all available devices.
    DeviceList(Vec<DeviceInfo>),
}

pub struct OpenDMX {
    ftdi: Ftdi,
    buffer: [u8; BUFFER_SIZE],
    info: DeviceInfo,

    baud_rate: u32,
    bits_per_word: libftd2xx::BitsPerWord,
    stop_bits: libftd2xx::StopBits,
    parity_none: libftd2xx::Parity,

    /// Time out for read operations
    read_time_out: Duration,

    /// Time out for write operations.
    write_time_out: Duration,

    /// Defaults to 40000 however this might cause flickering in some settings so users should be able to adjust this value.
    update_frequency: u32,
}

impl OpenDMX {
    /// Create a new device. Creating a device might fail (if no device is connected) this is why we return a result here.
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
            read_time_out: Duration::from_millis(500),
            write_time_out: Duration::from_millis(500),
            parity_none: libftd2xx::Parity::No,
            update_frequency: 40000,
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
        let size: usize;
        match self.ftdi.queue_status() {
            Ok(s) => {
                size = s;
            }
            Err(e) => {
                return Err(format!("Could read queue status. Error: {}", e));
            }
        }

        let mut buf: [u8; 4096] = [0; 4096];
        match self.ftdi.read_all(&mut buf[0..size]) {
            Ok(_) => {
                let r: Vec<u8> = buf.into();
                Ok(r)
            }
            Err(e) => Err(format!("Could read device data. Error: {}", e)),
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

    pub fn list_devices() -> Result<Vec<DeviceInfo>, String> {
        match list_devices() {
            Ok(l) => Ok(l),
            Err(e) => match e {
                libftd2xx::FtStatus::INVALID_HANDLE => Err("INVALID_HANDLE".to_string()),
                libftd2xx::FtStatus::DEVICE_NOT_FOUND => Err("DEVICE_NOT_FOUND".to_string()),
                libftd2xx::FtStatus::DEVICE_NOT_OPENED => Err("DEVICE_NOT_OPENED".to_string()),
                libftd2xx::FtStatus::IO_ERROR => Err("IO_ERROR".to_string()),
                libftd2xx::FtStatus::INSUFFICIENT_RESOURCES => {
                    Err("INSUFFICIENT_RESOURCES".to_string())
                }
                libftd2xx::FtStatus::INVALID_PARAMETER => Err("INVALID_PARAMETER".to_string()),
                libftd2xx::FtStatus::INVALID_BAUD_RATE => Err("INVALID_BAUD_RATE".to_string()),
                libftd2xx::FtStatus::DEVICE_NOT_OPENED_FOR_ERASE => {
                    Err("DEVICE_NOT_OPENED_FOR_ERASE".to_string())
                }
                libftd2xx::FtStatus::DEVICE_NOT_OPENED_FOR_WRITE => {
                    Err("DEVICE_NOT_OPENED_FOR_WRITE".to_string())
                }
                libftd2xx::FtStatus::FAILED_TO_WRITE_DEVICE => {
                    Err("FAILED_TO_WRITE_DEVICE".to_string())
                }
                libftd2xx::FtStatus::EEPROM_READ_FAILED => Err("EEPROM_READ_FAILED".to_string()),
                libftd2xx::FtStatus::EEPROM_WRITE_FAILED => Err("EEPROM_WRITE_FAILED".to_string()),
                libftd2xx::FtStatus::EEPROM_ERASE_FAILED => Err("EEPROM_ERASE_FAILED".to_string()),
                libftd2xx::FtStatus::EEPROM_NOT_PRESENT => Err("EEPROM_NOT_PRESENT".to_string()),
                libftd2xx::FtStatus::EEPROM_NOT_PROGRAMMED => {
                    Err("EEPROM_NOT_PROGRAMMED".to_string())
                }
                libftd2xx::FtStatus::INVALID_ARGS => Err("INVALID_ARGS".to_string()),
                libftd2xx::FtStatus::NOT_SUPPORTED => Err("NOT_SUPPORTED".to_string()),
                libftd2xx::FtStatus::OTHER_ERROR => Err("OTHER_ERROR".to_string()),
                libftd2xx::FtStatus::DEVICE_LIST_NOT_READY => {
                    Err("DEVICE_LIST_NOT_READY".to_string())
                }
            },
        }
    }

    /// Retrieve data about the current device.
    pub fn get_device_info(&self) -> &DeviceInfo {
        &self.info
    }

    pub fn set_break(&mut self, on: bool) -> bool {
        if on {
            match self.ftdi.set_break_on() {
                Ok(_) => true,
                Err(_) => false,
            }
        } else {
            match self.ftdi.set_break_off() {
                Ok(_) => true,
                Err(_) => false,
            }
        }
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
    /// This object keeps whether its internal state has changed or not and will only update device data
    /// if the local buffer has changed since the last write action.
    /// If you want to overwrite the device status regardless of the internal state set 'force' to true.
    pub fn write(&mut self) -> Result<(), String> {
        match self.ftdi.set_break_on() {
            Ok(_) => {}
            Err(e) => {
                return Err(format!("Could not set device break on. Error: {}", e));
            }
        }

        match self.ftdi.set_break_off() {
            Ok(_) => {}
            Err(e) => {
                return Err(format!("Could not set device break off. Error: {}", e));
            }
        }

        match self.ftdi.write_all(&self.buffer) {
            Ok(_) => Ok(()),
            Err(e) => {
                return Err(format!("Could not write data to device. Error: {}", e));
            }
        }
    }

    /// Reset the buffer to zero
    pub fn reset_buffer(&mut self) {
        self.buffer = [0; BUFFER_SIZE];
    }

    fn framesleep(timer: &Instant, frame_time: u128, granularity: TimerGranularity) {
        match granularity {
            TimerGranularity::Unknown => {
                while timer.elapsed().as_millis() < frame_time {
                    // Busy wait
                }
            }
            TimerGranularity::Good => {
                while timer.elapsed().as_millis() < frame_time {
                    thread::sleep(Duration::from_millis(1));
                }
            }
            TimerGranularity::Bad => {
                while timer.elapsed().as_millis() < frame_time {
                    // Busy wait
                }
            }
        }
    }

    /// Create and initialize a new open dmx module with the given id.
    /// This method also starts a new thread to continuously update the device.
    /// The device is beeing controlled using the returned Sender instance.
    ///
    /// This is a port of the implementation in QLC+. See:
    /// https://github.com/mcallegari/qlcplus/blob/master/plugins/dmxusb/src/enttecdmxusbopen.cpp
    ///
    pub fn run(id: i32) -> (Sender<OpenDmxProtocol>, Receiver<OpenDmxProtocol>) {
        let sender: Sender<OpenDmxProtocol>;
        let receiver: Receiver<OpenDmxProtocol>;
        (sender, receiver) = mpsc::channel();

        let sender2: Sender<OpenDmxProtocol>;
        let receiver2: Receiver<OpenDmxProtocol>;
        (sender2, receiver2) = mpsc::channel();

        thread::spawn(move || {
            // Wait for device to settle, in case the device was opened just recently.
            // Also, measure whether timer granularity is OK
            let mut now = Instant::now();

            let mut running = true;
            let mut device = OpenDMX::new(id).unwrap();
            thread::sleep(Duration::from_millis(1000));

            let granularity: TimerGranularity;

            if now.elapsed().as_secs() > 3 {
                granularity = TimerGranularity::Bad;
            } else {
                granularity = TimerGranularity::Good;
            }

            device.reset().unwrap();

            // The DMX frame time duration in microseconds.
            let frame_time: u128 =
                (((1000.0 / (device.update_frequency / 1000) as f64) + 0.5).floor()) as u128;

            while running {
                // Receive all incomming commands and update our buffer
                while let Ok(cmd) = receiver.try_recv() {
                    match cmd {
                        OpenDmxProtocol::SetValue(channel, value) => {
                            match device.set_dmx_value(channel, value) {
                                Ok(_) => {}
                                Err(_) => {}
                            }
                        }
                        OpenDmxProtocol::Stop => {
                            running = false;
                            continue;
                        }
                        OpenDmxProtocol::Reset => match device.reset() {
                            Ok(_) => {}
                            Err(_) => {
                                println!("Error resetting a DMX-Device.")
                            }
                        },
                        OpenDmxProtocol::ResetBuffer => {
                            device.reset_buffer();
                        }
                        OpenDmxProtocol::ListDevices => {
                            let mut payload = OpenDmxProtocol::DeviceList(Vec::new());
                            if let Ok(list) = Self::list_devices() {
                                payload = OpenDmxProtocol::DeviceList(list);
                            }

                            match sender2.send(payload) {
                                Ok(_) => {}
                                Err(_) => {
                                    println!("Could not send a list devices response.")
                                }
                            }
                        }
                        OpenDmxProtocol::DeviceList(_device_infos) => {}
                    }
                }

                // Update device.
                now = Instant::now();
                if !device.set_break(true) {
                    Self::framesleep(&now, frame_time, granularity);
                    continue;
                }

                if granularity == TimerGranularity::Good {
                    thread::sleep(Duration::from_micros(DMX_BREAK));
                }

                if !device.set_break(false) {
                    Self::framesleep(&now, frame_time, granularity);
                    continue;
                }

                if granularity == TimerGranularity::Good {
                    thread::sleep(Duration::from_micros(DMX_MAB));
                }

                match device.write() {
                    Ok(_) => {
                        Self::framesleep(&now, frame_time, granularity);
                    }

                    Err(_) => {
                        Self::framesleep(&now, frame_time, granularity);
                    }
                }
            }
        });

        (sender, receiver2)
    }
}

/// A device must be closed once it´s not used anymore. If not, the device will be blocked.
impl Drop for OpenDMX {
    fn drop(&mut self) {
        self.reset_buffer();

        match self.write() {
            Ok(_) => {}
            Err(e) => {
                println!("Could not reset device. Error: {}", e);
            }
        }

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

    /// This test might fail with different types of open_dmx hardware.
    #[test]
    pub fn device_info_test() {
        let mut subject = OpenDMX::new(0).unwrap();
        // Open device
        subject.reset().unwrap();

        let info = subject.get_device_info();
        assert_eq!("FT232R USB UART".to_owned(), info.description);
        assert_eq!("AL05O9B5".to_owned(), info.serial_number);
        assert_eq!(DeviceType::FT232R, info.device_type);       // This is hardware specific!
    }

    /// This test might fail with different types of open_dmx hardware.
    #[test]
    pub fn async_list_devices() {
        let (sender, receiver) = OpenDMX::run(0);
        sender.send(OpenDmxProtocol::ListDevices).unwrap();
        while let Ok(cmd) = receiver.try_recv() {
            match cmd {
                OpenDmxProtocol::DeviceList(device_infos) => {
                    assert!(device_infos.len() == 1);
                    assert!(device_infos[0].port_open);
                    assert_eq!(device_infos[0].device_type, DeviceType::FT232R);    // This is hardware specific!
                },
                _ => {
                    panic!("Expected a device list only.")
                }
            }
        }

        // Wait for the device to clear its queue.
        thread::sleep(Duration::from_millis(1000));
        sender.send(OpenDmxProtocol::Stop).unwrap();

        // And wait again so the device is properly shut down.
        thread::sleep(Duration::from_millis(100));
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
        let r: u8 = 255;
        let g: u8 = 10;
        let b: u8 = 10;

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

    #[test]
    pub fn run_test() {
        let sender = OpenDMX::run(0);

        match sender.0.send(OpenDmxProtocol::SetValue(2, 5 as u8)) {
            Ok(_) => {}
            Err(e) => {
                println!("Could not send data: {:?}", e);
            }
        }

        match sender.0.send(OpenDmxProtocol::SetValue(3, 5 as u8)) {
            Ok(_) => {}
            Err(e) => {
                println!("Could not send data: {:?}", e);
            }
        }

        for i in 1..255 {
            match sender.0.send(OpenDmxProtocol::SetValue(1, i as u8)) {
                Ok(_) => {}
                Err(e) => {
                    println!("Could not send data: {:?}", e);
                }
            }

            match sender.0.send(OpenDmxProtocol::SetValue(2, 255 - i as u8)) {
                Ok(_) => {}
                Err(e) => {
                    println!("Could not send data: {:?}", e);
                }
            }

            match sender.0.send(OpenDmxProtocol::SetValue(3, 255 - i as u8)) {
                Ok(_) => {}
                Err(e) => {
                    println!("Could not send data: {:?}", e);
                }
            }

            thread::sleep(Duration::from_millis(10));
        }

        thread::sleep(Duration::from_millis(1000));

        match sender.0.send(OpenDmxProtocol::Stop) {
            Ok(_) => {}
            Err(e) => {
                println!("Could not send stop: {:?}", e);
            }
        }

        thread::sleep(Duration::from_millis(100));
    }
}
