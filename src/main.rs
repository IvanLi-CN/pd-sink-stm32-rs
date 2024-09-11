#![no_std]
#![no_main]

use cortex_m::peripheral::SCB;
use display::Display;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_stm32::{
    gpio::{Level, Output, Speed},
    spi::{self, Spi},
    time::Hertz,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor as _};
use font::{
    get_indexes_by_str, DOT_MATRIX_XL_NUM, DOT_MATRIX_XL_NUM_INDEX, GROTESK_24_48,
    GROTESK_24_48_INDEX,
};
use heapless::String;

use defmt_rtt as _;
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

// This marks the entrypoint of our application.

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    defmt::println!("Hello, world!");

    let mut led = Output::new(p.PB8, Level::High, Speed::Low);

    let mut config = spi::Config::default();
    config.frequency = Hertz(16_000_000);
    let spi = Spi::new_txonly(p.SPI1, p.PA5, p.PA7, p.DMA1_CH1, p.DMA1_CH2, config); // SCK is unused.
    let mut spi: Mutex<NoopRawMutex, _> = Mutex::new(spi);

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

    let mut num = 0f64;

    loop {
        num += 1.123456789;
        display.update_monitor_amps(num).await;
        num += 1.123456789;
        display.update_monitor_volts(num).await;
        num += 1.123456789;
        display.update_monitor_watts(num).await;

        if num > 100.0 {
            num -= 100.0;
        }

        Timer::after(Duration::from_millis(1000)).await;
    }
}

async fn write_number<'a, 'b>(display: &mut ST7789_Display<'a, 'b>, number: u16) {
    let mut indexes = [0; 10];
    get_indexes_by_str(GROTESK_24_48_INDEX, "1234567890", &mut indexes);

    let width = 24;
    let height = 50;

    let color = Rgb565::WHITE;
    let bg_color = Rgb565::BLACK;

    for i in 0..10 {
        // for ele in GROTESK_24_48[*idx] {
        //     defmt::info!("indexes: {:?}", ele);
        // }
        display
            .write_area(
                (10 + i * (4 + width)) as u16,
                10,
                width as u16,
                GROTESK_24_48[indexes[i]],
                color,
                bg_color,
            )
            .await
            .unwrap();
    }
}

// same panicking *behavior* as `panic-probe` but doesn't print a panic message
// this prevents the panic message being printed *twice* when `defmt::panic` is invoked
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

struct FpsCounter {
    frame_count: u32,
    last_time: Instant,
    fps: f32,
}

impl FpsCounter {
    fn new() -> Self {
        Self {
            frame_count: 0,
            last_time: Instant::now(),
            fps: 0.0,
        }
    }

    async fn update(&mut self) {
        self.frame_count += 1;
        let now = Instant::now();
        let elapsed = now - self.last_time;

        if elapsed >= Duration::from_millis(1000) {
            self.fps = self.frame_count as f32 / (elapsed.as_millis() as f32 / 1000.0);
            self.frame_count = 0;
            self.last_time = now;
        }
    }
}
