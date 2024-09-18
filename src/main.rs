#![no_std]
#![no_main]

use cortex_m::peripheral::SCB;
use display::Display;
use embassy_embedded_hal::shared_bus::asynch::{i2c::I2cDevice, spi::SpiDevice};
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts,
    gpio::{Level, Output, Speed},
    i2c::{self, I2c},
    peripherals,
    spi::{self, Spi},
    time::Hertz,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    mutex::Mutex,
};
use embassy_time::{Duration, Timer};

use defmt_rtt as _;
use husb238::Husb238;
use ina226::{DEFAULT_ADDRESS, INA226};
// global logger
use panic_probe as _;

use st7789::{self, ST7789};

mod display;
mod font;

type ST7789_Display<'a, 'b> = ST7789<
    SpiDevice<
        'a,
        NoopRawMutex,
        Spi<
            'b,
            embassy_stm32::peripherals::SPI1,
            embassy_stm32::peripherals::DMA1_CH1,
            embassy_stm32::peripherals::DMA1_CH2,
        >,
        Output<'b, embassy_stm32::peripherals::PA4>,
    >,
    Output<'a, embassy_stm32::peripherals::PA15>,
    Output<'a, embassy_stm32::peripherals::PA12>,
>;

bind_interrupts!(struct Irqs {
    I2C1 => i2c::EventInterruptHandler<peripherals::I2C1>, i2c::ErrorInterruptHandler<peripherals::I2C1>;
});

// This marks the entrypoint of our application.

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    defmt::println!("Hello, world!");

    let mut out_ctl_pin = Output::new(p.PA8, Level::Low, Speed::Low);

    let mut config = spi::Config::default();
    config.frequency = Hertz(16_000_000);
    let spi = Spi::new_txonly(p.SPI1, p.PA5, p.PA7, p.DMA1_CH1, p.DMA1_CH2, config); // SCK is unused.
    let spi: Mutex<NoopRawMutex, _> = Mutex::new(spi);

    let cs_pin = Output::new(p.PA4, Level::High, Speed::High);
    let spi_dev = SpiDevice::new(&spi, cs_pin);

    let dc_pin = Output::new(p.PA15, Level::Low, Speed::High);
    let rst_pin = Output::new(p.PA12, Level::Low, Speed::High);

    let st7789 = ST7789::new(st7789::Config::default(), spi_dev, dc_pin, rst_pin);

    let mut display = Display::new(st7789);

    match display.init().await {
        Ok(_) => {
            defmt::info!("Display initialized.");
        }
        Err(_) => {
            defmt::info!("Display initialization failed.");

            Timer::after(Duration::from_millis(1000)).await;

            SCB::sys_reset();
        }
    }

    let i2c = I2c::new(
        p.I2C1,
        p.PB8,
        p.PB7,
        Irqs,
        p.DMA1_CH3,
        p.DMA1_CH4,
        Hertz(100_000),
        Default::default(),
    );

    let i2c: Mutex<CriticalSectionRawMutex, _> = Mutex::new(i2c);

    let i2c_dev = I2cDevice::new(&i2c);

    let mut ina226 = INA226::new(i2c_dev, DEFAULT_ADDRESS);
    ina226
        .set_configuration(&ina226::Config {
            mode: ina226::MODE::ShuntBusVoltageContinuous,
            avg: ina226::AVG::_4,
            vbusct: ina226::VBUSCT::_4156us,
            vshct: ina226::VSHCT::_4156us,
        })
        .await
        .unwrap();

    ina226.callibrate(0.01, 5.0).await.unwrap();

    out_ctl_pin.set_high();

    let i2c_dev = I2cDevice::new(&i2c);
    let mut husb238 = Husb238::new(i2c_dev);

    let mut count = 0u8;

    loop {
        match ina226.bus_voltage_millivolts().await {
            Ok(val) => {
                display.update_monitor_volts(val / 1000.0).await;
            }
            Err(_) => {
                display.update_monitor_volts(99999.99999).await;
            }
        }

        match ina226.current_amps().await {
            Ok(val) => {
                display.update_monitor_amps(val.unwrap_or(0.0)).await;
            }
            Err(_) => {
                display.update_monitor_amps(99999.99999).await;
            }
        }

        match ina226.power_watts().await {
            Ok(val) => {
                display.update_monitor_watts(val.unwrap_or(0.0)).await;
            }
            Err(_) => {
                display.update_monitor_watts(99999.99999).await;
            }
        }

        count += 1;
        if count < 10 {
            continue;
        }

        count = 0;

        match husb238.get_actual_voltage_and_current().await {
            Ok((volts, amps)) => {
                display.update_target_volts(volts.unwrap_or(0.0)).await;
                display.update_limit_amps(amps).await;
            }
            Err(_) => {}
        }

        // Timer::after(Duration::from_millis(1000)).await;
    }
}
