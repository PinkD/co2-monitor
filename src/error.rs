use alloc::string::String;
use esp_hal::i2c::master::Error as i2cError;

#[derive(Debug)]
pub enum Error {
    I2cError(i2cError),
    SimpleError(String),
}
