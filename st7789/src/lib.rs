#![no_std]

use core::convert::Infallible;

use embassy_time::Delay;
use embedded_graphics_core::geometry::Dimensions;
use embedded_graphics_core::prelude::RawData;
use embedded_graphics_core::{
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::{DrawTarget, OriginDimensions, Size},
    Pixel,
};
use embedded_hal::digital::OutputPin;
use embedded_hal_async::{delay::DelayNs, spi::SpiDevice};

const BUF_SIZE: usize = 10 * 160 * 2;

/// ST7735 instructions.
#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    NOP = 0x00,
    SWRESET = 0x01,
    RDDID = 0x04,
    RDDST = 0x09,
    SLPIN = 0x10,
    SLPOUT = 0x11,
    PTLON = 0x12,
    NORON = 0x13,
    INVOFF = 0x20,
    INVON = 0x21,
    DISPOFF = 0x28,
    DISPON = 0x29,
    CASET = 0x2A,
    RASET = 0x2B,
    RAMWR = 0x2C,
    RAMRD = 0x2E,
    PTLAR = 0x30,
    COLMOD = 0x3A,
    MADCTL = 0x36,
    FRMCTR1 = 0xB1,
    FRMCTR2 = 0xB2,
    FRMCTR3 = 0xB3,
    INVCTR = 0xB4,
    DISSET5 = 0xB6,
    PWCTR1 = 0xC0,
    PWCTR2 = 0xC1,
    PWCTR3 = 0xC2,
    PWCTR4 = 0xC3,
    PWCTR5 = 0xC4,
    VMCTR1 = 0xC5,
    RDID1 = 0xDA,
    RDID2 = 0xDB,
    RDID3 = 0xDC,
    RDID4 = 0xDD,
    PWCTR6 = 0xFC,
    GMCTRP1 = 0xE0,
    GMCTRN1 = 0xE1,
}

#[derive(Clone, Copy)]
pub enum Orientation {
    Portrait = 0x00,
    Landscape = 0x60,
    PortraitSwapped = 0xC0,
    LandscapeSwapped = 0xA0,
}

#[derive(Clone, Copy)]
pub struct Config {
    pub rgb: bool,
    pub inverted: bool,
    pub orientation: Orientation,
    pub height: u16,
    pub width: u16,
    pub dx: u16,
    pub dy: u16,
}

#[derive(Debug)]
pub enum Error<E = ()> {
    /// Communication error
    Comm(E),
    /// Pin setting error
    Pin(Infallible),
}

pub struct Display<SPI, DC, RST>
where
    SPI: SpiDevice,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
{
    spi: SPI,
    dc: DC,
    rst: RST,
    config: Config,
}

impl<SPI, DC, RST, E> Display<SPI, DC, RST>
where
    SPI: SpiDevice<Error = E>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
{
    pub fn new(config: Config, spi: SPI, dc: DC, rst: RST) -> Self {
        Self {
            spi,
            dc,
            rst,
            config,
        }
    }

    pub async fn init(&mut self) -> Result<(), Error<E>> {
        self.reset().await?;

        let dc = &mut self.dc;
        let inverted = self.config.inverted;
        let rgb = self.config.rgb;

        struct Command<'a> {
            instruction: Instruction,
            params: &'a [u8],
            delay_time: u32,
        }

        impl<'a> Command<'a> {
            fn new(instruction: Instruction, params: &'a [u8], delay_time: u32) -> Self {
                Self {
                    instruction,
                    params,
                    delay_time,
                }
            }
        }

        let commands = [
            Command::new(Instruction::SWRESET, &[], 200),
            Command::new(Instruction::SLPOUT, &[], 200),
            Command::new(Instruction::FRMCTR1, &[0x01, 0x2C, 0x2D], 0),
            Command::new(Instruction::FRMCTR2, &[0x01, 0x2C, 0x2D], 0),
            Command::new(
                Instruction::FRMCTR3,
                &[0x01, 0x2C, 0x2D, 0x01, 0x2C, 0x2D],
                0,
            ),
            Command::new(Instruction::INVCTR, &[0x07], 0),
            Command::new(Instruction::PWCTR1, &[0xA2, 0x02, 0x84], 0),
            Command::new(Instruction::PWCTR2, &[0xC5], 0),
            Command::new(Instruction::PWCTR3, &[0x0A, 0x00], 0),
            Command::new(Instruction::PWCTR4, &[0x8A, 0x2A], 0),
            Command::new(Instruction::PWCTR5, &[0x8A, 0xEE], 0),
            Command::new(Instruction::VMCTR1, &[0x0E], 0),
            Command::new(
                if inverted {
                    Instruction::INVON
                } else {
                    Instruction::INVOFF
                },
                &[],
                0,
            ),
            Command::new(Instruction::MADCTL, if rgb { &[0x00] } else { &[0x08] }, 0),
            Command::new(Instruction::COLMOD, &[0x05], 0),
            Command::new(Instruction::DISPON, &[], 200),
        ];

        for Command {
            instruction,
            params,
            delay_time,
        } in commands
        {
            dc.set_low().ok();
            let mut data = [0_u8; 1];
            data.copy_from_slice(&[instruction as u8]);
            self.spi.write(&data).await.map_err(Error::Comm)?;
            if !params.is_empty() {
                dc.set_high().ok();
                let mut buf = [0_u8; 8];
                buf[..params.len()].copy_from_slice(params);
                self.spi
                    .write(&buf[..params.len()])
                    .await
                    .map_err(Error::Comm)?;
            }
            if delay_time > 0 {
                Delay.delay_ms(delay_time).await;
            }
        }

        self.set_orientation(self.config.orientation).await?;
        Ok(())
    }

    pub async fn reset(&mut self) -> Result<(), Error<E>> {
        self.rst.set_high().map_err(Error::Pin)?;
        Delay.delay_ms(10).await;
        self.rst.set_low().map_err(Error::Pin)?;
        Delay.delay_ms(10).await;
        self.rst.set_high().map_err(Error::Pin)?;

        Ok(())
    }

    pub async fn set_orientation(&mut self, orientation: Orientation) -> Result<(), Error<E>> {
        if self.config.rgb {
            self.write_command(Instruction::MADCTL, &[orientation as u8])
                .await?;
        } else {
            self.write_command(Instruction::MADCTL, &[orientation as u8 | 0x08])
                .await?;
        }
        self.config.orientation = orientation;
        Ok(())
    }

    async fn write_command(
        &mut self,
        instruction: Instruction,
        params: &[u8],
    ) -> Result<(), Error<E>> {
        let dc = &mut self.dc;
        dc.set_low().ok();
        let mut data = [0_u8; 1];
        data.copy_from_slice(&[instruction as u8]);
        self.spi.write(&data).await.map_err(Error::Comm)?;
        if !params.is_empty() {
            dc.set_high().ok();
            let mut buf = [0_u8; 8];
            buf[..params.len()].copy_from_slice(params);
            self.spi
                .write(&buf[..params.len()])
                .await
                .map_err(Error::Comm)?;
        }
        Ok(())
    }

    fn start_data(&mut self) -> Result<(), Error<E>> {
        self.dc.set_high().map_err(Error::Pin)
    }

    async fn write_data(&mut self, data: &[u8]) -> Result<(), Error<E>> {
        let mut buf = [0_u8; 8];
        buf[..data.len()].copy_from_slice(data);
        self.spi
            .write(&buf[..data.len()])
            .await
            .map_err(Error::Comm)
    }

    /// Sets the global offset of the displayed image
    pub fn set_offset(&mut self, dx: u16, dy: u16) {
        self.config.dx = dx;
        self.config.dy = dy;
    }

    /// Sets the address window for the display.
    pub async fn set_address_window(
        &mut self,
        sx: u16,
        sy: u16,
        ex: u16,
        ey: u16,
    ) -> Result<(), Error<E>> {
        self.write_command(Instruction::CASET, &[]).await?;
        self.start_data()?;
        let sx_bytes = (sx + self.config.dx).to_be_bytes();
        let ex_bytes = (ex + self.config.dx).to_be_bytes();
        self.write_data(&[sx_bytes[0], sx_bytes[1], ex_bytes[0], ex_bytes[1]])
            .await?;
        self.write_command(Instruction::RASET, &[]).await?;
        self.start_data()?;
        let sy_bytes = (sy + self.config.dy).to_be_bytes();
        let ey_bytes = (ey + self.config.dy).to_be_bytes();
        self.write_data(&[sy_bytes[0], sy_bytes[1], ey_bytes[0], ey_bytes[1]])
            .await
    }

    pub async fn flush_frame<const N: usize>(&mut self, frame: &Frame<N>) -> Result<(), Error<E>> {
        self.set_address_window(0, 0, frame.width as u16 - 1, frame.height as u16 - 1)
            .await?;
        self.write_command(Instruction::RAMWR, &[]).await?;
        self.start_data()?;
        self.spi.write(&frame.buffer).await.map_err(Error::Comm)
    }

    pub async fn fill_color(&mut self, color: Rgb565) -> Result<(), Error<E>> {
        self.set_address_window(0, 0, self.config.width - 1, self.config.height - 1)
            .await?;
        let color = RawU16::from(color).into_inner();
        let mut buf = [0_u8; 1440];
        for i in 0..720 {
            let bytes = color.to_le_bytes(); // 将u16转换为小端字节序的[u8; 2]
            buf[i * 2 + 1] = bytes[0]; // 存储低字节
            buf[i * 2] = bytes[1]; // 存储高字节
        }
        self.write_command(Instruction::RAMWR, &[]).await?;
        self.start_data()?;
        for _ in 0..self.config.height / 4 {
            self.spi.write(&buf).await.map_err(Error::Comm)?;
        }
        Ok(())
    }
}

pub struct Frame<const N: usize> {
    pub width: u32,
    pub height: u32,
    pub orientation: Orientation,
    pub buffer: [u8; N],
}

impl<const N: usize> Frame<N> {
    pub fn new(width: u32, height: u32, orientation: Orientation, buffer: [u8; N]) -> Self {
        Self {
            width,
            height,
            orientation,
            buffer,
        }
    }
    pub fn set_pixel(&mut self, x: u16, y: u16, color: Rgb565) {
        let color = RawU16::from(color).into_inner();
        let idx = match self.orientation {
            Orientation::Landscape | Orientation::LandscapeSwapped => {
                if x as u32 >= self.width {
                    return;
                }
                ((y as usize) * self.width as usize) + (x as usize)
            }

            Orientation::Portrait | Orientation::PortraitSwapped => {
                if y as u32 >= self.width {
                    return;
                }
                ((y as usize) * self.height as usize) + (x as usize)
            }
        } * 2;

        // Split 16 bit value into two bytes
        let low = (color & 0xff) as u8;
        let high = ((color & 0xff00) >> 8) as u8;
        if idx >= self.buffer.len() - 1 {
            return;
        }
        self.buffer[idx] = high;
        self.buffer[idx + 1] = low;
    }
}
impl<const N: usize> Default for Frame<N> {
    fn default() -> Self {
        Self {
            width: 160,
            height: 80,
            orientation: Orientation::Landscape,
            buffer: [0; N],
        }
    }
}

impl<const N: usize> DrawTarget for Frame<N> {
    type Error = ();
    type Color = Rgb565;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let bb = self.bounding_box();
        pixels
            .into_iter()
            .filter(|Pixel(pos, _color)| bb.contains(*pos))
            .for_each(|Pixel(pos, color)| self.set_pixel(pos.x as u16, pos.y as u16, color));
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let c = RawU16::from(color).into_inner();
        for i in 0..BUF_SIZE {
            assert!(i < self.buffer.len());
            self.buffer[i] = if i % 2 == 0 {
                ((c & 0xff00) >> 8) as u8
            } else {
                (c & 0xff) as u8
            };
        }
        Ok(())
    }
}

impl<const N: usize> OriginDimensions for Frame<N> {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}
