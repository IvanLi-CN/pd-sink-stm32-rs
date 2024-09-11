use core::convert::Infallible;

use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};
use embedded_hal::digital::OutputPin;
use embedded_hal_async::spi::SpiDevice;
use heapless::String;
use st7789::ST7789;

use crate::font::{get_index_by_char, GROTESK_24_48, GROTESK_24_48_INDEX};

#[derive(Debug, Clone, Copy, defmt::Format)]
pub struct PowerInfo {
    pub amps: f64,
    pub volts: f64,
    pub watts: f64,
}

impl Default for PowerInfo {
    fn default() -> Self {
        Self {
            amps: 0.0,
            volts: 0.0,
            watts: 0.0,
        }
    }
}

pub struct Display<SPI, DC, RST>
where
    SPI: SpiDevice,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
{
    st7789: ST7789<SPI, DC, RST>,
    power_info: PowerInfo,
    prev_power_info: PowerInfo,
    ryu_buffer: ryu::Buffer,
    prev_ryu_buffer: ryu::Buffer,
    force_render: bool,
}

impl<SPI, DC, RST> Display<SPI, DC, RST>
where
    SPI: SpiDevice,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
{
    pub fn new(st7789: ST7789<SPI, DC, RST>) -> Self {
        Self {
            st7789,
            power_info: PowerInfo::default(),
            prev_power_info: PowerInfo::default(),
            ryu_buffer: ryu::Buffer::new(),
            prev_ryu_buffer: ryu::Buffer::new(),
            force_render: true,
        }
    }

    pub async fn init(&mut self) -> Result<(), ()> {
        self.st7789.init().await.map_err(|_| ())?;

        self.update_monitor_amps(0.0).await;
        self.update_monitor_volts(0.0).await;
        self.update_monitor_watts(0.0).await;

        Ok(())
    }

    pub async fn update_monitor_amps(&mut self, amps: f64) {
        self.power_info.amps = amps;

        let curr = self.ryu_buffer.format(self.power_info.amps);
        let prev = self.prev_ryu_buffer.format(self.prev_power_info.amps);

        Self::render_monitor(
            &mut self.st7789,
            curr,
            prev,
            10,
            Rgb565::BLACK,
            Rgb565::WHITE,
            self.force_render,
        )
        .await;

        self.prev_power_info.amps = self.power_info.amps;
    }

    pub async fn update_monitor_volts(&mut self, volts: f64) {
        self.power_info.volts = volts;

        let curr = self.ryu_buffer.format(self.power_info.volts);
        let prev = self.prev_ryu_buffer.format(self.prev_power_info.volts);

        Self::render_monitor(
            &mut self.st7789,
            curr,
            prev,
            60,
            Rgb565::BLACK,
            Rgb565::WHITE,
            self.force_render,
        )
        .await;

        self.prev_power_info.volts = self.power_info.volts;
    }

    pub async fn update_monitor_watts(&mut self, watts: f64) {
        self.power_info.watts = watts;

        let curr = self.ryu_buffer.format(self.power_info.watts);
        let prev = self.prev_ryu_buffer.format(self.prev_power_info.watts);

        Self::render_monitor(
            &mut self.st7789,
            curr,
            prev,
            110,
            Rgb565::BLACK,
            Rgb565::WHITE,
            self.force_render,
        )
        .await;

        self.prev_power_info.watts = self.power_info.watts;
    }

    async fn render_monitor(
        st7789: &mut ST7789<SPI, DC, RST>,
        curr: &str,
        prev: &str,
        y: u16,
        bg_color: Rgb565,
        color: Rgb565,
        force_render: bool,
    ) {
        let mut chars = curr.chars();
        let mut chars_prev = prev.chars();

        for idx in 0..7 {
            let char = chars.next();
            if char == chars_prev.next() {
                if !force_render {
                    continue;
                }
            }

            let char = match char {
                Some(c) => c,
                None => '0',
            };

            st7789
                .write_area(
                    10 + idx * 25,
                    y,
                    24,
                    GROTESK_24_48[get_index_by_char(GROTESK_24_48_INDEX, char)],
                    color,
                    bg_color,
                )
                .await
                .unwrap();
        }
    }
}
