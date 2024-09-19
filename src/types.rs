use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_stm32::peripherals;
use embassy_stm32::{gpio::Output, spi::Spi};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use st7789::ST7789;

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

pub(crate) type SpiBus =
    Spi<'static, peripherals::SPI1, peripherals::DMA1_CH1, peripherals::DMA1_CH2>;

pub(crate) type ST7789CSPin = Output<'static, embassy_stm32::peripherals::PA4>;
pub(crate) type ST7789DCPin = Output<'static, embassy_stm32::peripherals::PA15>;
pub(crate) type ST7789RstPin = Output<'static, embassy_stm32::peripherals::PA12>;

pub(crate) type ST7789SpiDev = SpiDevice<'static, CriticalSectionRawMutex, SpiBus, ST7789CSPin>;

pub(crate) type ST7789Display = ST7789<ST7789SpiDev, ST7789DCPin, ST7789RstPin>;
