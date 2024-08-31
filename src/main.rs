#![no_std]
#![no_main]

use cortex_m::peripheral::SCB;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_stm32::{
    gpio::{Level, Output, Speed},
    spi::{self, Spi},
    time::Hertz,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Delay, Duration, Timer};
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    text::Text,
};

use defmt_rtt as _;
use embedded_hal::delay::DelayNs;
// global logger
use panic_probe as _;

use st7789;

// This marks the entrypoint of our application.

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    defmt::println!("Hello, world!");

    let mut led = Output::new(p.PB8, Level::High, Speed::Low);

    // let _sck = Pin::new(Port::A, 5, PinMode::Alt(0));
    // let _mosi = Pin::new(Port::A, 7, PinMode::Alt(0));
    // let mut _cs = Pin::new(Port::A, 4, PinMode::Output);
    // let mut dc = Pin::new(Port::A, 15, PinMode::Output);
    // _cs.output_speed(OutputSpeed::High);
    // dc.output_speed(OutputSpeed::High);

    let mut config = spi::Config::default();
    config.frequency = Hertz(16_000_000);
    let mut spi = Spi::new_txonly(p.SPI1, p.PA5, p.PA7, p.DMA1_CH1, p.DMA1_CH2, config); // SCK is unused.
    let spi: Mutex<NoopRawMutex, _> = Mutex::new(spi);

    let mut cs_pin = Output::new(p.PA4, Level::High, Speed::High);
    let mut spi_dev = SpiDevice::new(&spi, cs_pin);

    let mut dc_pin = Output::new(p.PA15, Level::Low, Speed::High);
    let mut rst_pin = Output::new(p.PA12, Level::Low, Speed::High);

    let colors = [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE];


    let mut display = st7789::Display::new(
        st7789::Config::default(),
        spi_dev,
        dc_pin,
        rst_pin,
    );

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

    // Clear the display initially

    loop {
        led.set_low();
        defmt::info!("Output pin is low.");
        match display.fill_color(colors[0]).await {
            Ok(_) => {}
            Err(_) => {
                defmt::error!("Fill color error");
            }
        };
        // Timer::after(Duration::from_millis(1000)).await;
        led.set_high();
        defmt::info!("Output pin is high.");
        match display.fill_color(colors[1]).await {
            Ok(_) => {}
            Err(_) => {
                defmt::error!("Fill color error");
            }
        };
        // Timer::after(Duration::from_millis(1000)).await;
    }
}

// same panicking *behavior* as `panic-probe` but doesn't print a panic message
// this prevents the panic message being printed *twice* when `defmt::panic` is invoked
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}
