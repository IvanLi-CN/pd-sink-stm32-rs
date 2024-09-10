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
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::{Rgb565, Rgb888},
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use font::{get_indexes_by_str, DOT_MATRIX_XL_NUM, DOT_MATRIX_XL_NUM_INDEX};
use heapless::String;
use numtoa::NumToA;

use defmt_rtt as _;
// global logger
use panic_probe as _;

use st7789::{self, Display, Frame};

mod font;

type ST7789_Display<'a, 'b> = Display<
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

    let colors = [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE];

    let mut display = st7789::Display::new(st7789::Config::default(), spi_dev, dc_pin, rst_pin);

    let mut fps_counter = FpsCounter::new();

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

    let character_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let text_style = TextStyleBuilder::new()
        .baseline(Baseline::Bottom)
        .alignment(Alignment::Right)
        .build();
    let bg_style = PrimitiveStyleBuilder::new()
        // #277da1
        .fill_color(Rgb565::from(Rgb888::new(0x27, 0x7d, 0xa1)))
        .build();

    let mut frame = Frame::new(40, 40, st7789::Orientation::Landscape, [0; 40 * 40 * 2]);
    let mut str = String::<20>::new();
    let mut str_buff = [0; 20];

    let mut t = 0;

    loop {
        display.fill_color(Rgb565::BLUE).await.unwrap();
        write_number(&mut display, t as u16).await;
        // led.set_low();
        // match display.fill_color(colors[0]).await {
        //     Ok(_) => {}
        //     Err(_) => {
        //         defmt::error!("Fill color error");
        //     }
        // };
        // fps_counter.update().await;
        // // Timer::after(Duration::from_millis(1000)).await;
        // // led.set_high();
        // match display.fill_color(colors[1]).await {
        //     Ok(_) => {}
        //     Err(_) => {
        //         defmt::error!("Fill color error");
        //     }
        // };
        // fps_counter.update().await;

        // RoundedRectangle::with_equal_corners(
        //     Rectangle::new(Point::new(0, 0), Size::new(40, 40)),
        //     Size::new(5, 5),
        // )
        // .into_styled(bg_style)
        // .draw(&mut frame)
        // .unwrap();

        // str.clear();
        // str.push_str((fps_counter.fps as u16).numtoa_str(10, &mut str_buff))
        //     .unwrap();

        // t = (t + 1) % 3;
        // match display.fill_color(colors[t]).await {
        //     Ok(_) => {}
        //     Err(_) => {
        //         defmt::error!("Fill color error");
        //     }
        // };

        // Text::with_text_style(&str, Point::new(30, 30), character_style, text_style)
        //     .draw(&mut frame)
        //     .unwrap();

        // display.flush_frame(&frame).await.unwrap();
        // write_number(&mut display, t as u16).await;
        Timer::after(Duration::from_millis(1000)).await;
        display.fill_color(Rgb565::RED).await.unwrap();
        Timer::after(Duration::from_millis(1000)).await;
    }
}

async fn write_number<'a, 'b>(display: &mut ST7789_Display<'a, 'b>, number: u16) {
    let mut indexes = [0; 10];
    get_indexes_by_str(DOT_MATRIX_XL_NUM_INDEX, "12345", &mut indexes);

    let width = 32;
    let height = 50;

    let color = Rgb565::WHITE;
    let bg_color = Rgb565::BLACK;

    for i in 0..5 {
        // for ele in DOT_MATRIX_XL_NUM[*idx] {
        //     defmt::info!("indexes: {:?}", ele);
        // }
        display
            .write_area(
                (10 + i * (10 + width)) as u16,
                10,
                width as u16,
                DOT_MATRIX_XL_NUM[indexes[i]],
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
