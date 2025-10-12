use crate::utils::DebugPrinter;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use embedded_graphics::prelude::Size;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, Output};
use esp_hal::spi::master::Spi;
use esp_hal::spi::Error;
use esp_hal::Blocking;
use log::debug;

// ref 1: https://www.waveshare.net/wiki/Pico-ePaper-2.9
// ref 2: https://www.waveshare.net/w/upload/7/79/2.9inch-e-paper-v2-specification.pdf
pub struct EPaper<'a> {
    spi: RefCell<Spi<'a, Blocking>>,
    // 0 for cmd, 1 for data
    dc: RefCell<Output<'a>>,
    // 1 for reset
    reset: RefCell<Output<'a>>,
    power: RefCell<Output<'a>>,
    // 1 for busy
    busy: RefCell<Input<'a>>,

    delay: Delay,

    width: u16,
    height: u16,
}

impl<'a> EPaper<'a> {
    pub fn new(
        size: &Size,
        spi: Spi<'a, Blocking>,
        power: Output<'a>,
        busy: Input<'a>,
        reset: Output<'a>,
        dc: Output<'a>,
    ) -> Self {
        EPaper {
            spi: RefCell::new(spi),
            dc: RefCell::new(dc),
            reset: RefCell::new(reset),
            power: RefCell::new(power),
            busy: RefCell::new(busy),

            delay: Delay::new(),
            width: size.width as u16,
            height: size.height as u16,
        }
    }

    pub fn init_black_white(&self) -> Result<(), Error> {
        DebugPrinter::new("init black white".to_string());
        self.power_up();
        self.hw_reset();
        self.wait_busy();

        // soft reset
        self.write_cmd(0x12)?;
        self.wait_busy();

        let h1 = (self.height / 8 - 1) as u8;
        let w1 = ((self.width - 1) % 256) as u8;
        let w2 = ((self.width - 1) / 256) as u8;
        let init_seq = vec![
            // driver output control
            (0x01, vec![w1, w2, 0x00]),
            // data entry mode
            (0x11, vec![0x03]),
            // screen resolution, set x
            (0x44, vec![0x00, h1]),
            // screen resolution, set y
            (0x45, vec![0x00, 0x00, w1, w2]),
            // display update control
            (0x21, vec![0x00, 0x80]),
            // cursor x
            (0x4e, vec![0x00]),
            // cursor y
            (0x4f, vec![0x00, 0x00]),
        ];
        for (cmd, data) in init_seq {
            self.write_cmd(cmd)?;
            self.write_data(data.as_slice())?;
        }
        self.wait_busy();

        let lut = self.black_white_lut();
        // self.delay.delay_millis(1000);
        self.init_lut(lut)?;
        // TODO: this is not necessary on init, we should call it manually
        self.clear_screen()?;
        self.wait_busy();
        // self.delay.delay_millis(1000);
        Ok(())
    }

    pub fn init_gray4(&self) -> Result<(), Error> {
        DebugPrinter::new("init gray4".to_string());
        self.power_up();
        self.hw_reset();
        self.wait_busy();

        // soft reset
        self.write_cmd(0x12)?;
        self.wait_busy();

        let h1 = (self.height / 8) as u8;
        let w1 = ((self.width - 1) % 256) as u8;
        let w2 = ((self.width - 1) / 256) as u8;
        let init_seq = vec![
            // driver output control
            (0x01, vec![w1, w2, 0x00]),
            // data entry mode
            (0x11, vec![0x03]),
            // TODO: dynamic resolution
            // screen resolution, x: 128
            // x start, x end, divided by 8
            (0x44, vec![0x01, h1]),
            // screen resolution, y: 296
            // y start, y end, divided by 256
            (0x45, vec![0x00, 0x00, w1, w2]),
            // display update control
            (0x3c, vec![0x04]),
            // cursor x
            (0x4e, vec![0x01]),
            // cursor y
            (0x4f, vec![0x00, 0x00]),
        ];
        for (cmd, data) in init_seq {
            self.write_cmd(cmd)?;
            self.write_data(data.as_slice())?;
        }
        self.wait_busy();

        let lut = self.gray4_lut();
        self.delay.delay_millis(1000);
        self.init_lut(lut)?;
        // TODO: this is not necessary on init, we should call it manually
        self.clear_screen()?;
        self.delay.delay_millis(1000);
        Ok(())
    }

    pub fn display_partial(&self, data: &[u8]) -> Result<(), Error> {
        DebugPrinter::new("display partial".to_string());
        self.init_partial_update()?;

        // write data to black-white cache
        self.write_cmd(0x24)?;
        self.write_data(data)?;

        self.sync_partial_screen()?;
        Ok(())
    }

    pub fn init_partial_update(&self) -> Result<(), Error> {
        DebugPrinter::new("init partial update".to_string());
        self.power_up();
        self.hw_reset();
        self.wait_busy();

        let lut = self.partial_update_lut();
        self.write_cmd(0x32)?;
        self.write_data(lut[0..153].iter().as_slice())?;

        let init_seq = vec![
            (
                0x37,
                vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00],
            ),
            (0x3c, vec![0x80]),
            (0x22, vec![0xc0]),
            (0x20, vec![]),
        ];
        for (cmd, data) in init_seq {
            self.write_cmd(cmd)?;
            self.write_data(data.as_slice())?;
        }
        self.wait_busy();
        let h1 = (self.height / 8 - 1) as u8;
        let w1 = ((self.width - 1) % 256) as u8;
        let w2 = ((self.width - 1) / 256) as u8;
        let init_seq = vec![
            // screen resolution, x
            (0x44, vec![0x00, h1]),
            // screen resolution, y
            (0x45, vec![0x00, 0x00, w1, w2]),
            // display update control
            // (0x3c, vec![0x04]),
            // cursor x: 0
            (0x4e, vec![0x00]),
            // cursor y: 0
            (0x4f, vec![0x00, 0x00]),
        ];
        for (cmd, data) in init_seq {
            self.write_cmd(cmd)?;
            self.write_data(data.as_slice())?;
        }
        self.wait_busy();
        Ok(())
    }

    // clear screen, aka set all pixel to white
    pub fn clear_screen(&self) -> Result<(), Error> {
        DebugPrinter::new("clear screen".to_string());
        let len = self.width / 8 * self.height;
        let data = vec![0xff; len as usize];
        // write data to black-white cache
        self.write_cmd(0x24)?;
        self.write_data(data.as_slice())?;
        // clean up red cache or gray cache
        self.write_cmd(0x26)?;
        self.write_data(data.as_slice())?;
        self.sync_screen()?;
        Ok(())
    }

    pub fn sync_screen(&self) -> Result<(), Error> {
        DebugPrinter::new("sync screen".to_string());
        self.write_cmd(0x22)?;
        self.write_data(0xc7u8.to_be_bytes().as_ref())?;
        self.write_cmd(0x20)?;
        self.wait_busy();
        Ok(())
    }

    pub fn sync_partial_screen(&self) -> Result<(), Error> {
        DebugPrinter::new("sync partial screen".to_string());
        self.write_cmd(0x22)?;
        self.write_data(0x0fu8.to_be_bytes().as_ref())?;
        self.write_cmd(0x20)?;
        self.wait_busy();
        Ok(())
    }

    pub fn display_black_white(&self, data: &[u8]) -> Result<(), Error> {
        // write data to black-white cache
        self.write_cmd(0x24)?;
        self.write_data(data)?;
        // write data to red cache or gray cache
        self.write_cmd(0x26)?;
        self.write_data(data)?;
        self.sync_screen()?;
        Ok(())
    }

    pub fn display_gray4(&self, data: &[u8]) -> Result<(), Error> {
        let (data1, data2) = Self::parse_gray4_data(data);
        // write data to black-white cache
        self.write_cmd(0x24)?;
        self.write_data(data1.as_slice())?;
        // clean up red cache or gray cache
        self.write_cmd(0x26)?;
        self.write_data(data2.as_slice())?;
        self.sync_screen()?;
        Ok(())
    }

    pub fn halt(&self) -> Result<(), Error> {
        self.write_cmd(0x10)?;
        self.write_data(0x01u8.to_be_bytes().as_ref())?;
        // self.shutdown();
        Ok(())
    }

    // TODO: make private
    pub fn write_cmd(&self, cmd: u8) -> Result<(), Error> {
        self.set_cmd_flag();
        debug!("# 0x{:x}", cmd);
        self.spi.borrow_mut().write(cmd.to_be_bytes().as_ref())
    }

    pub fn write_data(&self, data: &[u8]) -> Result<(), Error> {
        if data.is_empty() {
            return Ok(());
        }
        self.set_data_flag();
        // debug!("{:?}", data);
        self.spi.borrow_mut().write(data)
    }
}

// private functions
impl<'a> EPaper<'a> {
    fn set_cmd_flag(&self) {
        self.dc.borrow_mut().set_low()
    }

    fn set_data_flag(&self) {
        self.dc.borrow_mut().set_high()
    }

    fn power_up(&self) {
        self.power.borrow_mut().set_high();
        self.delay.delay_millis(100);
    }

    #[allow(dead_code)]
    fn shutdown(&self) {
        self.power.borrow_mut().set_low();
    }

    fn hw_reset(&self) {
        DebugPrinter::new("hw_reset".to_string());
        self.reset.borrow_mut().set_high();
        self.delay.delay_millis(10);
        self.reset.borrow_mut().set_low();
        self.delay.delay_millis(2);
        self.reset.borrow_mut().set_high();
        self.delay.delay_millis(100);
    }

    fn wait_busy(&self) {
        DebugPrinter::new("wait_busy".to_string());
        loop {
            if self.busy.borrow_mut().is_low() {
                break;
            }
            self.delay.delay_millis(50);
        }
    }

    fn init_lut(&self, lut: [u8; 159]) -> Result<(), Error> {
        DebugPrinter::new("init lut".to_string());
        // lut
        self.write_cmd(0x32)?;
        self.write_data(lut[0..153].iter().as_slice())?;
        self.wait_busy();
        self.write_cmd(0x3f)?;
        self.write_data(lut[153].to_be_bytes().as_slice())?;
        // gate voltage
        self.write_cmd(0x03)?;
        self.write_data(lut[154].to_be_bytes().as_slice())?;
        // source voltage
        self.write_cmd(0x04)?;
        self.write_data(lut[155..158].iter().as_slice())?;
        // common(?) voltage
        self.write_cmd(0x2c)?;
        self.write_data(lut[158].to_be_bytes().as_slice())?;
        Ok(())
    }

    fn parse_gray4_data(data: &[u8]) -> (Vec<u8>, Vec<u8>) {
        DebugPrinter::new("parse gray".to_string());
        let mut data1 = Vec::new();
        let mut data2 = Vec::new();
        let mut index = 0;
        data.chunks_exact(2).for_each(|chunk| {
            let b1 = 0b10000000;
            let b2 = 0b01000000;
            let b3 = 0b00100000;
            let b4 = 0b00010000;
            let b5 = 0b00001000;
            let b6 = 0b00000100;
            let b7 = 0b00000010;
            let b8 = 0b00000001;

            let a = chunk[0];
            let b = chunk[1];
            let d1 = 0
                + (a & b1)
                + ((a & b3) << 1)
                + ((a & b5) << 2)
                + ((a & b7) << 3)
                + ((b & b1) >> 4)
                + ((b & b3) >> 3)
                + ((b & b5) >> 2)
                + ((b & b7) >> 1);
            data1.push(d1);
            let d2 = 0
                + ((a & b2) << 1)
                + ((a & b4) << 2)
                + ((a & b6) << 3)
                + ((a & b8) << 4)
                + ((b & b2) >> 3)
                + ((b & b4) >> 2)
                + ((b & b6) >> 1)
                + (b & b8);
            data2.push(d2);
            index += 1;
        });
        debug!("{}: {}, {}", data.len(), data1.len(), data2.len());
        // data1.reverse();
        // data2.reverse();
        (data1, data2)
    }
}

// init lookup tables
impl<'a> EPaper<'a> {
    fn black_white_lut(&self) -> [u8; 159] {
        [
            0x80, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, // VS L0
            0x10, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, // VS L1
            0x80, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, // VS L2
            0x10, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, // VS L3
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L4
            0x14, 0x08, 0x00, 0x00, 0x00, 0x00, 0x01, // TP, SR, RP of Group0
            0x0A, 0x0A, 0x00, 0x0A, 0x0A, 0x00, 0x01, // TP, SR, RP of Group1
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group2
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group3
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group4
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group5
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group6
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group7
            0x14, 0x08, 0x00, 0x01, 0x00, 0x00, 0x01, // TP, SR, RP of Group8
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // TP, SR, RP of Group9
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group10
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group11
            0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x00, 0x00, 0x00, //FR, XON
            0x22, 0x17, 0x41, 0x00, 0x32, 0x36, // EOPT VGH VSH1 VSH2 VSL VCOM
        ]
    }

    fn gray4_lut(&self) -> [u8; 159] {
        [
            0x00, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L0
            0x20, 0x60, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L1
            0x28, 0x60, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L2
            0x2A, 0x60, 0x15, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L3
            0x00, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L4
            0x00, 0x02, 0x00, 0x05, 0x14, 0x00, 0x00, // TP, SR, RP of Group0
            0x1E, 0x1E, 0x00, 0x00, 0x00, 0x00, 0x01, // TP, SR, RP of Group1
            0x00, 0x02, 0x00, 0x05, 0x14, 0x00, 0x00, // TP, SR, RP of Group2
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group3
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group4
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group5
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group6
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group7
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group8
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group9
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group10
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group11
            0x24, 0x22, 0x22, 0x22, 0x23, 0x32, 0x00, 0x00, 0x00, // FR, XON
            0x22, 0x17, 0x41, 0xAE, 0x32, 0x28, // EOPT VGH VSH1 VSH2 VSL VCOM
        ]
    }

    fn partial_update_lut(&self) -> [u8; 159] {
        [
            0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L0
            0x80, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L1
            0x40, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L2
            0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L3
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // VS L4
            0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, // TP, SR, RP of Group0
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group1
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group2
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group3
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group4
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group5
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group6
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group7
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group8
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group9
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group10
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TP, SR, RP of Group11
            0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x00, 0x00, 0x00, // FR, XON
            0x22, 0x17, 0x41, 0xB0, 0x32, 0x36, // EOPT VGH VSH1 VSH2 VSL VCOM
        ]
    }
}
