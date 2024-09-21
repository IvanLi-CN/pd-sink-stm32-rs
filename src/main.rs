#![no_std]
#![no_main]

use button::Button;
use controller::Controller;
use display::Display;
use embassy_embedded_hal::shared_bus::asynch::{i2c::I2cDevice, spi::SpiDevice};
use embassy_executor::Spawner;
use embassy_futures::select::{select3, Either3};
use embassy_stm32::{
    bind_interrupts,
    exti::ExtiInput,
    gpio::{Input, Level, Output, OutputType, Pull, Speed},
    i2c::{self, I2c},
    peripherals::{self, PB0, PC14},
    spi::{self, Spi},
    time::{khz, Hertz},
    timer::simple_pwm::{PwmPin, SimplePwm},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};

use defmt_rtt as _;
use embassy_time::{Duration, Ticker};
use husb238::Husb238;
use ina226::{DEFAULT_ADDRESS, INA226};
// global logger
use panic_probe as _;

use shared::{BTN_A_STATE_CHANNEL, BTN_B_STATE_CHANNEL, DISPLAY};
use st7789::{self, ST7789};
use static_cell::StaticCell;
use types::{ST7789Display, SpiBus};

mod button;
mod controller;
mod display;
mod font;
mod shared;
mod types;

static SPI_BUS_MUTEX: StaticCell<Mutex<CriticalSectionRawMutex, SpiBus>> = StaticCell::new();

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
    let spi: Mutex<CriticalSectionRawMutex, _> = Mutex::new(spi);
    let spi = SPI_BUS_MUTEX.init(spi);

    // init display

    let cs_pin = Output::new(p.PA4, Level::High, Speed::High);
    let dc_pin = Output::new(p.PA15, Level::Low, Speed::High);
    let rst_pin = Output::new(p.PA12, Level::Low, Speed::High);

    // let cs_pin = ST7789_CS_PIN.init(cs_pin);
    // let dc_pin = ST7789_DC_PIN.init(dc_pin);
    // let rst_pin = ST7789_RST_PIN.init(rst_pin);

    let spi_dev = SpiDevice::new(spi, cs_pin);

    // let spi_dev = ST7789_SPI_DEV.init(spi_dev);

    let st7789: ST7789Display = ST7789::new(st7789::Config::default(), spi_dev, dc_pin, rst_pin);
    let mut _display = Display::new(st7789);

    _display.init().await.unwrap();

    let mut display = DISPLAY.lock().await;
    *display = Some(_display);
    drop(display);

    // init backlight

    let blk_pin = PwmPin::new_ch3(p.PB6, OutputType::PushPull);

    let mut blk_tim = SimplePwm::new(
        p.TIM1,
        None,
        None,
        Some(blk_pin),
        None,
        khz(1),
        embassy_stm32::timer::CountingMode::EdgeAlignedUp,
    );

    blk_tim.enable(embassy_stm32::timer::Channel::Ch3);
    blk_tim.set_duty(
        embassy_stm32::timer::Channel::Ch3,
        blk_tim.get_max_duty() / 2,
    );

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

    // init ina226

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

    // init buttons

    let button_a = ExtiInput::new(Input::new(p.PC14, Pull::Up), p.EXTI14);
    let button_b = ExtiInput::new(Input::new(p.PB0, Pull::Up), p.EXTI0);

    spawner.spawn(controller_exec()).ok();
    spawner.spawn(btns_exec(button_a, button_b)).ok();

    out_ctl_pin.set_high();

    let i2c_dev = I2cDevice::new(&i2c);
    let mut husb238 = Husb238::new(i2c_dev);

    let mut count = 0u8;

    loop {
        let mut display = DISPLAY.lock().await;

        if display.is_none() {
            continue;
        }
        let display = display.as_mut().unwrap();

        display.task().await;

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

#[embassy_executor::task]
async fn btns_exec(mut btn_a: ExtiInput<'static, PC14>, mut btn_b: ExtiInput<'static, PB0>) {
    let mut button_a = Button::new(&BTN_A_STATE_CHANNEL);
    let mut button_b = Button::new(&BTN_B_STATE_CHANNEL);

    loop {
        let btn_a_change = btn_a.wait_for_any_edge();

        let btn_b_change = btn_b.wait_for_any_edge();

        let mut ticker = Ticker::every(Duration::from_millis(100));

        let futures = select3(btn_a_change, btn_b_change, ticker.next());

        match futures.await {
            Either3::First(_) => {
                if btn_a.is_high() {
                    button_a.on_release().await;
                } else {
                    button_a.on_press().await;
                }
            }
            Either3::Second(_) => {
                if btn_b.is_high() {
                    button_b.on_release().await;
                } else {
                    button_b.on_press().await;
                }
            }
            Either3::Third(_) => {
                button_a.update().await;
                button_b.update().await;
            }
        };
    }
}

#[embassy_executor::task]
async fn controller_exec() {
    let mut controller = Controller::new();

    controller.task().await;
}