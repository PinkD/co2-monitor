use crate::error::Error;
use crate::utils::DebugPrinter;
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::{format, vec};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Dimensions, Point, Size};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::pixelcolor::raw::ToBytes;
use embedded_graphics::pixelcolor::{Gray4, GrayColor};
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::renderer::TextRenderer;
use embedded_graphics::text::Baseline;
use embedded_graphics::{mono_font, Pixel};
use log::{debug, warn};

use crate::scd41::MeasureResult;

enum Gray2Color {
    Black = 0b11,
    DarkGray = 0b01,
    LightGray = 0b10,
    White = 0b00,
}

impl Into<u8> for Gray2Color {
    fn into(self) -> u8 {
        self as u8
    }
}

pub struct Canvas {
    height: u32,
    width: u32,
    colors: Vec<u8>,
    pixels: Vec<Vec<u8>>,
}

impl Dimensions for Canvas {
    fn bounding_box(&self) -> Rectangle {
        Rectangle {
            top_left: Point { x: 0, y: 0 },
            size: Size::new(self.width, self.height),
        }
    }
}

impl DrawTarget for Canvas {
    // using 4 bit gray as input because some color conversion need more bits
    // to scale the original color to 2 bit
    type Color = Gray4;
    type Error = Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        DebugPrinter::new("draw iter".to_string());
        pixels.into_iter().for_each(|Pixel(point, color)| {
            // debug!("color: {:?}, point: {:?}", color, point);
            let mut data = color.to_be_bytes()[0];
            // store in first 4 bit to ensure min value is not 0
            data += 0x10;
            if !self.colors.contains(&data) {
                debug!("new color: {:?}", color);
                self.colors.push(data);
                debug!("color len: {}", self.colors.len());
            }
            if point.x >= self.pixels.len() as i32 || point.y >= self.pixels[0].len() as i32 {
                warn!("out of range, color: {:?}, point: {:?}", color, point);
            }
            // NOTE: I don't know why but the x-axis is reverted, so we revert it back
            let x = self.width - point.x as u32 - 1;
            // debug!("draw ({}, {}): {:04b}", point.x, point.y, data);
            self.pixels[x as usize][point.y as usize] = data;
        });
        // debug!("sort color, color len: {}", self.colors.len());
        self.colors.sort();

        let black;
        let mut light_gray = 0b10u8;
        let mut dark_gray = 0b01u8;
        let mut white = 0b00u8;
        match self.colors.len() {
            1 => {
                debug!("single color");
                black = self.colors[0];
            }
            2 => {
                debug!("2 colors");
                black = self.colors[0];
                light_gray = self.colors[1];
                // ignore dark gray
            }
            3 => {
                debug!("3 colors");
                black = self.colors[0];
                light_gray = self.colors[1];
                dark_gray = self.colors[2];
            }
            4 => {
                debug!("4 colors");
                // shrink gray4 color to gray2 color
                black = self.colors[0];
                dark_gray = self.colors[1];
                light_gray = self.colors[2];
                white = self.colors[3];
            }
            _ => {
                warn!("unknown color count: {}", self.colors.len());
                return Err(Error::SimpleError(format!(
                    "only 2 bit gray supported, got {} colors: {:?}",
                    self.colors.len(),
                    self.colors,
                )));
            }
        }
        debug!(
            "normalized color, black: {:04b}, gray1: {:04b}, gray2: {:04b}, white: {:04b}",
            black, light_gray, dark_gray, white
        );

        // TODO: we don't need to range full canvas,
        //   but we cannot collect colors from pixels then do range
        //   because it cannot be used after moved.
        //   neither can we collect all pixels and range twice because of the memory limit
        self.pixels.iter_mut().for_each(|row| {
            row.iter_mut().for_each(|pixel| {
                let color;
                match *pixel {
                    p if p == black => {
                        color = Gray2Color::Black;
                    }
                    p if p == light_gray => {
                        color = Gray2Color::LightGray;
                    }
                    p if p == dark_gray => {
                        color = Gray2Color::DarkGray;
                    }
                    p if p == white => {
                        color = Gray2Color::White;
                    }
                    _ => {
                        // ignore
                        return;
                    }
                }
                *pixel = color.into();
            })
        });
        Ok(())
    }
}

impl Canvas {
    pub fn new(size: &Size) -> Self {
        Canvas {
            width: size.width,
            height: size.height,
            colors: Vec::new(),
            pixels: vec![vec![0; size.height as usize]; size.width as usize],
        }
    }
}

impl Canvas {
    /// render gray pixels to width*height/4 sized vector
    pub fn render_gray(&self) -> Vec<u8> {
        DebugPrinter::new("render gray".to_string());
        // 1 byte -> 2 bit
        // 4 byte -> 1 byte
        let mut data = Vec::new();
        self.pixels.iter().for_each(|row| {
            row.chunks_exact(4).for_each(|chunk| {
                let d = (chunk[0] & 0b11) << 6
                    | (chunk[1] & 0b11) << 4
                    | (chunk[2] & 0b11) << 2
                    | (chunk[3] & 0b11);
                data.push(d);
            });
        });
        let required_len = (self.width * self.height / 4) as usize;
        assert_eq!(
            data.len(),
            required_len,
            "render result len {} not eq {}",
            data.len(),
            required_len,
        );
        data
    }

    /// render black white pixels to width*height/8 sized vector
    pub fn render_black_white(&self) -> Vec<u8> {
        DebugPrinter::new("render black white".to_string());
        // 1 byte -> 1 bit
        // 8 byte -> 1 byte
        let mut data = Vec::new();
        let black_white = |c: u8| -> u8 {
            if c == 1 {
                // black
                0
            } else {
                // white
                1
            }
        };
        self.pixels.iter().for_each(|row| {
            row.chunks_exact(8).for_each(|chunk| {
                let d = black_white(chunk[0] & 1) << 7
                    | black_white(chunk[1] & 1) << 6
                    | black_white(chunk[2] & 1) << 5
                    | black_white(chunk[3] & 1) << 4
                    | black_white(chunk[4] & 1) << 3
                    | black_white(chunk[5] & 1) << 2
                    | black_white(chunk[6] & 1) << 1
                    | black_white(chunk[7] & 1);
                data.push(d);
            });
        });
        let required_len = (self.width * self.height / 8) as usize;
        assert_eq!(
            data.len(),
            required_len,
            "render result len {} not eq {}",
            data.len(),
            required_len,
        );
        data
    }

    pub fn draw_at(&mut self, canvas: Canvas, point: Point) {
        canvas.pixels.iter().enumerate().for_each(|(x, row)| {
            row.iter().enumerate().for_each(|(y, pixel)| {
                self.pixels[x + point.x as usize][y + point.y as usize] = *pixel;
            })
        });
    }

    pub fn draw_text(&mut self, text: &str, point: Point) {
        let font = mono_font::ascii::FONT_10X20;
        let color = Gray4::BLACK;
        let style = MonoTextStyleBuilder::new()
            .text_color(color)
            .font(&font)
            .build();
        style
            .draw_string(text, point, Baseline::Bottom, self)
            .unwrap();
    }
}

pub struct Screen {
    size: Size,
}

impl Screen {
    pub fn new(size: &Size) -> Self {
        Screen { size: *size }
    }

    pub fn render(&mut self, measure_result: &MeasureResult) -> Vec<u8> {
        let mut canvas = Canvas::new(&self.size);
        let temp_str = format!("Temp: {:>2.1} C", measure_result.temp);
        canvas.draw_text(temp_str.as_str(), Point::new(20, 50));
        let hum_str = format!("Hum: {:>2.1} %", measure_result.hum);
        canvas.draw_text(hum_str.as_str(), Point::new(160, 50));
        let co2_str = format!("CO2: {:>4} ppm", measure_result.co2_ppm);
        canvas.draw_text(co2_str.as_str(), Point::new(20, 100));
        canvas.render_black_white()
    }
}
