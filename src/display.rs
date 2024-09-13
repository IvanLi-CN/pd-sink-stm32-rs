use core::convert::Infallible;

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{RgbColor, WebColors},
};
use embedded_hal::digital::OutputPin;
use embedded_hal_async::spi::SpiDevice;
use st7789::ST7789;

use crate::font::{
    get_index_by_char, ARIAL_ROUND_16_24, ARIAL_ROUND_16_24_INDEX, GROTESK_24_48,
    GROTESK_24_48_INDEX,
};

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

#[derive(Debug, Clone, Copy, defmt::Format)]
pub struct StatusInfo {
    pub target_volts: f64,
    pub limit_amps: f64,
    pub output: bool,
}

impl Default for StatusInfo {
    fn default() -> Self {
        Self {
            target_volts: 0.0,
            limit_amps: 0.0,
            output: false,
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
    status_info: StatusInfo,
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
            status_info: StatusInfo::default(),
            ryu_buffer: ryu::Buffer::new(),
            prev_ryu_buffer: ryu::Buffer::new(),
            force_render: true,
        }
    }

    pub async fn init(&mut self) -> Result<(), ()> {
        self.force_render = true;

        self.st7789.init().await.map_err(|_| ())?;

        self.st7789
            .fill_color(Rgb565::CSS_SLATE_GRAY)
            .await
            .unwrap();

        self.update_layout().await;

        self.update_monitor_amps(0.0).await;
        self.update_monitor_volts(0.0).await;
        self.update_monitor_watts(0.0).await;

        self.update_target_volts(0.0).await;
        self.update_limit_amps(0.0).await;
        self.update_output(false).await;

        self.force_render = false;
        Ok(())
    }

    pub async fn update_monitor_volts(&mut self, volts: f64) {
        self.power_info.volts = volts;

        let curr = self.ryu_buffer.format(self.power_info.volts);
        let prev = self.prev_ryu_buffer.format(self.prev_power_info.volts);

        Self::render_monitor(
            &mut self.st7789,
            curr,
            prev,
            10,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            self.force_render,
        )
        .await;

        self.prev_power_info.volts = self.power_info.volts;
    }

    pub async fn update_monitor_amps(&mut self, amps: f64) {
        self.power_info.amps = amps;

        let curr = self.ryu_buffer.format(self.power_info.amps);
        let prev = self.prev_ryu_buffer.format(self.prev_power_info.amps);

        Self::render_monitor(
            &mut self.st7789,
            curr,
            prev,
            60,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            self.force_render,
        )
        .await;

        self.prev_power_info.amps = self.power_info.amps;
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
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            self.force_render,
        )
        .await;

        self.prev_power_info.watts = self.power_info.watts;
    }

    pub async fn update_target_volts(&mut self, volts: f64) {
        self.status_info.target_volts = volts;

        let curr = self.ryu_buffer.format(self.status_info.target_volts);

        Self::render_status(
            &mut self.st7789,
            curr,
            210,
            35,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            4,
        )
        .await;
    }

    pub async fn update_limit_amps(&mut self, amps: f64) {
        self.status_info.limit_amps = amps;

        let curr: &str = self.ryu_buffer.format(self.status_info.limit_amps);

        Self::render_status(
            &mut self.st7789,
            curr,
            210,
            85,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            4,
        )
        .await;
    }

    pub async fn update_output(&mut self, output: bool) {
        self.status_info.output = output;

        Self::render_status(
            &mut self.st7789,
            if output { "ON" } else { "OFF" },
            210,
            135,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            3,
        )
        .await;
    }

    pub async fn update_layout(&mut self) {
        Self::render_status(
            &mut self.st7789,
            "V",
            186,
            34,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            1,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "A",
            186,
            82,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            1,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "W",
            186,
            130,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            1,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "PDO",
            210,
            10,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            3,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "Max",
            210,
            60,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            3,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "Out",
            210,
            110,
            Rgb565::CSS_DARK_GRAY,
            Rgb565::WHITE,
            3,
        )
        .await;
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

    async fn render_status(
        st7789: &mut ST7789<SPI, DC, RST>,
        curr: &str,
        x: u16,
        y: u16,
        bg_color: Rgb565,
        color: Rgb565,
        len: u16,
    ) {
        let mut chars = curr.chars();

        for idx in 0..len {
            let char = chars.next();

            let char = match char {
                Some(c) => c,
                None => '0',
            };

            st7789
                .write_area(
                    x + idx * 16,
                    y,
                    16,
                    ARIAL_ROUND_16_24[get_index_by_char(ARIAL_ROUND_16_24_INDEX, char)],
                    color,
                    bg_color,
                )
                .await
                .unwrap();
        }
    }
}
