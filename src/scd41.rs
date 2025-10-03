use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use esp_hal::delay::Delay;
use esp_hal::i2c::master::{Error, I2c};
use esp_hal::Blocking;
use esp_println::println;
use crate::{debug, log};

// ref: https://sensirion.com/media/documents/48C4B7FB/67FE0194/CD_DS_SCD4x_Datasheet_D1.pdf
// read measurement, call start_periodic_measurement before call this
pub const CMD_READ_MEASUREMENT: u16 = 0xec05;
// start periodic measurement
pub const CMD_START_PERIODIC_MEASUREMENT: u16 = 0x21b1;
// stop periodic measurement
pub const CMD_STOP_PERIODIC_MEASUREMENT: u16 = 0x3f86;
// measure oneshot, delay 5s before reading result
pub const CMD_ONESHOT_MEASUREMENT: u16 = 0x219d;
// start low power periodic measurement
pub const CMD_START_LOW_POWER_PERIODIC_MEASUREMENT: u16 = 0x21ac;
// wait data ready
pub const CMD_GET_DATA_READY_STATUS: u16 = 0xe4b8;
// get temperature offset
pub const CMD_GET_TEMPERATURE_OFFSET: u16 = 0x2318;
// set temperature offset
pub const CMD_SET_TEMPERATURE_OFFSET: u16 = 0x241d;
// wake up sensor
pub const CMD_WAKEUP: u16 = 0x36f6;
// persist settings
pub const CMD_PERSIST_SETTINGS: u16 = 0x3615;
// SCD41 I2C address
const SCD41_ADDRESS: u8 = 0x62;

#[derive(PartialEq, Default)]
pub struct MeasureResult {
    pub co2_ppm: u16,
    pub temp: f32,
    pub hum: f32,
}

pub struct SCD41<'a> {
    i2c: RefCell<I2c<'a, Blocking>>,
    delay: Delay,
    started: bool,
}

impl<'a> SCD41<'a> {
    pub fn new(i2c: I2c<'a, Blocking>) -> Self {
        SCD41 {
            i2c: RefCell::new(i2c),
            delay: Delay::new(),
            started: false,
        }
    }

    /// get measurement results from sensor
    pub fn measure(&self) -> Result<MeasureResult, Error> {
        if !self.started {
            return Ok(MeasureResult {
                co2_ppm: 0,
                temp: 0.0,
                hum: 0.0,
            });
        }
        self.wait_ready()?;
        self.cmd(CMD_READ_MEASUREMENT)?;
        self.delay.delay_millis(1);
        let data = self.read(9)?;
        self.parse(data.as_slice())
    }

    /// get measurement results from sensor
    pub fn measure_oneshot(&self) -> Result<MeasureResult, Error> {
        self.cmd(CMD_ONESHOT_MEASUREMENT)?;
        self.delay.delay_millis(5000);
        let data = self.read(9)?;
        self.parse(data.as_slice())
    }

    /// get temperature offset from sensor
    pub fn get_temperature_offset(&self) -> Result<f32, Error> {
        self.cmd(CMD_GET_TEMPERATURE_OFFSET)?;
        self.delay.delay_millis(1);
        let data = self.read(2)?;
        let offset = 175.0 * u16::from_be_bytes([data[0], data[1]]) as f32 / 65535.0;
        Ok(offset)
    }

    /// set temperature offset from sensor
    pub fn set_temperature_offset(&self, offset: f32) -> Result<(), Error> {
        let offset = (offset * 65535.0 / 175.0) as u16;
        let offset_data = offset.to_be_bytes();
        let mut data = offset_data.to_vec();
        let crc = crc(&offset_data);
        data.push(crc);
        self.cmd_with_arg(CMD_SET_TEMPERATURE_OFFSET, data)
    }

    /// persist settings for sensor
    pub fn persist_settings(&self) -> Result<(), Error> {
        self.cmd(CMD_PERSIST_SETTINGS)?;
        self.delay.delay_millis(600);
        Ok(())
    }

    /// start measurement
    pub fn start(&mut self) -> Result<(), Error> {
        self.cmd(CMD_START_PERIODIC_MEASUREMENT)?;
        self.delay.delay_millis(500);
        self.started = true;
        Ok(())
    }

    /// start low power measurement
    pub fn start_low_power(&mut self) -> Result<(), Error> {
        self.cmd(CMD_START_LOW_POWER_PERIODIC_MEASUREMENT)?;
        self.delay.delay_millis(500);
        self.started = true;
        Ok(())
    }

    /// stop measurement
    pub fn stop(&mut self) -> Result<(), Error> {
        self.cmd(CMD_STOP_PERIODIC_MEASUREMENT)?;
        self.delay.delay_millis(500);
        self.started = false;
        Ok(())
    }

    pub fn parse(&self, data: &[u8]) -> Result<MeasureResult, Error> {
        let co2_ppm = u16::from_be_bytes([data[0], data[1]]);
        let temp = -45.0 + 175.0 * u16::from_be_bytes([data[3], data[4]]) as f32 / 65535.0;
        let hum = 100.0 * u16::from_be_bytes([data[6], data[7]]) as f32 / 65535.0;
        // TODO: validate result
        Ok(MeasureResult { co2_ppm, temp, hum })
    }

    pub fn cmd(&self, cmd: u16) -> Result<(), Error> {
        self.i2c
            .borrow_mut()
            .write(SCD41_ADDRESS, cmd.to_be_bytes().as_ref())
    }

    pub fn cmd_with_arg(&self, cmd: u16, args: Vec<u8>) -> Result<(), Error> {
        let data = vec![cmd.to_be_bytes().as_ref(), args.as_slice()].concat();
        self.i2c.borrow_mut().write(SCD41_ADDRESS, data.as_ref())
    }

    pub fn wait_ready(&self) -> Result<(), Error> {
        // const READ_MASK: u16 = 0x7ff;
        const READ_MASK: u16 = 0x8000;
        loop {
            self.cmd(CMD_GET_DATA_READY_STATUS)?;
            self.delay.delay_millis(1);
            let data = self.read(3)?;
            let flag = u16::from_be_bytes([data[0], data[1]]);
            // if flag & READ_MASK == READ_MASK {
            if flag != READ_MASK {
                debug!("flag: 0x{:04x}", flag);
                return Ok(());
            }
            debug!("flag: 0x{:04x}", flag);
            debug!("sensor data not ready");
            self.delay.delay_millis(1000);
        }
    }

    pub fn read(&self, size: usize) -> Result<Vec<u8>, Error> {
        let mut buf = vec![0u8; size];
        self.i2c
            .borrow_mut()
            .read(SCD41_ADDRESS, buf.as_mut_slice())?;
        Ok(buf)
    }
}

pub fn crc(data: &[u8]) -> u8 {
    const CRC8_POLYNOMIAL: u8 = 0x31;
    const CRC8_INIT: u8 = 0xFF;

    let mut crc = CRC8_INIT;

    for &byte in data {
        crc ^= byte;

        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ CRC8_POLYNOMIAL;
            } else {
                crc <<= 1;
            }
        }
    }

    crc
}
