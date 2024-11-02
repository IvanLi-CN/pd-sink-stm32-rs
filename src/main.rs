#![no_std]
#![no_main]

use button::Button;
use controller::Controller;
use display::Display;
use embassy_embedded_hal::shared_bus::{
    asynch::{i2c::I2cDevice, spi::SpiDevice},
    I2cDeviceError,
};
use embassy_executor::Spawner;
use embassy_futures::select::{select3, Either3};
use embassy_stm32::{
    bind_interrupts,
    exti::ExtiInput,
    gpio::{Level, Output, OutputType, Pull, Speed},
    i2c::{self, I2c},
    mode, peripherals,
    spi::{self, Spi},
    time::{khz, Hertz},
    timer::simple_pwm::{PwmPin, SimplePwm},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};

use defmt_rtt as _;
use embassy_time::{Duration, Ticker};
use exponential_moving_average::ExponentialMovingAverage;
use husb238::{Command, Husb238};
use ina226::{DEFAULT_ADDRESS, INA226};
use output_controller::OutputController;
// global logger
use panic_probe as _;

use shared::{
    AVAILABLE_VOLT_CURR_MUTEX, BTN_A_STATE_CHANNEL, BTN_B_STATE_CHANNEL, DISPLAY, OCP_MUTEX,
    PDO_PUBSUB,
};
use st7789::{self, ST7789};
use static_cell::StaticCell;
use types::{AvailableVoltCurr, ST7789Display, SpiBus};

mod button;
mod controller;
mod display;
mod exponential_moving_average;
mod font;
mod output_controller;
mod shared;
mod types;

static SPI_BUS_MUTEX: StaticCell<Mutex<CriticalSectionRawMutex, SpiBus>> = StaticCell::new();
static HUSB238_I2C_MUTEX: StaticCell<Mutex<CriticalSectionRawMutex, I2c<'_, mode::Async>>> =
    StaticCell::new();

bind_interrupts!(struct Irqs {
    I2C1 => i2c::EventInterruptHandler<peripherals::I2C1>, i2c::ErrorInterruptHandler<peripherals::I2C1>;
});

// This marks the entrypoint of our application.

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    defmt::println!("Hello, world!");

    let out_ctl_pin = Output::new(p.PA8, Level::Low, Speed::Low);
    let mut output_controller = OutputController::new(out_ctl_pin);

    let mut config = spi::Config::default();
    config.frequency = Hertz(16_000_000);
    let spi = Spi::new_txonly(p.SPI1, p.PA5, p.PA7, p.DMA1_CH1, config); // SCK is unused.
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
        embassy_stm32::timer::low_level::CountingMode::EdgeAlignedUp,
    );

    blk_tim.ch3().enable();
    blk_tim.ch3().set_duty_cycle_percent(50);

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

    let i2c = Mutex::new(i2c);
    let i2c = HUSB238_I2C_MUTEX.init(i2c);

    // init ina226

    let i2c_dev = I2cDevice::new(&i2c);
    let mut ina226 = INA226::new(i2c_dev, DEFAULT_ADDRESS);
    ina226
        .set_configuration(&ina226::Config {
            mode: ina226::MODE::ShuntBusVoltageContinuous,
            avg: ina226::AVG::_1,
            vbusct: ina226::VBUSCT::_4156us,
            vshct: ina226::VSHCT::_4156us,
        })
        .await
        .unwrap();

    ina226.callibrate(0.01, 5.0).await.unwrap();

    // init buttons

    let button_a = ExtiInput::new(p.PC14, p.EXTI14, Pull::Up);
    let button_b = ExtiInput::new(p.PB0, p.EXTI0, Pull::Up);

    spawner.spawn(controller_exec()).ok();
    spawner.spawn(btns_exec(button_a, button_b)).ok();

    let i2c_dev = I2cDevice::new(i2c);
    let mut husb238 = Husb238::new(i2c_dev);

    {
        let mut available_volt_curr = AVAILABLE_VOLT_CURR_MUTEX.lock().await;

        *available_volt_curr = get_available_volt_curr(&mut husb238).await.unwrap();
    }

    let mut pdo_sub = PDO_PUBSUB.subscriber().unwrap();

    let mut count = 0u8;

    let mut amps_avg = ExponentialMovingAverage::new(0.1);
    let mut volts_avg = ExponentialMovingAverage::new(0.1);

    loop {
        let mut display = DISPLAY.lock().await;

        if display.is_none() {
            continue;
        }
        let display = display.as_mut().unwrap();

        output_controller.task().await;
        display.task().await;

        match ina226.bus_voltage_millivolts().await {
            Ok(val) => {
                volts_avg.update(val / 1000.0);
                display.update_monitor_volts(volts_avg.get_average()).await;
            }
            Err(_) => {
                display.update_monitor_volts(99999.99999).await;
            }
        }

        let ocp_guard = OCP_MUTEX.lock().await;
        let ocp = *ocp_guard;

        display.update_ocp_amps(ocp).await;


        match ina226.current_amps().await {
            Ok(val) => {
                let amps = val.unwrap_or(0.0).max(0.0);

                if amps > ocp {
                    output_controller.set_output(false).await;
                    display.update_monitor_amps(amps).await;
                } else {
                    amps_avg.update(amps);
                    display.update_monitor_amps(amps_avg.get_average()).await;
                }
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

        let changed_pdo = pdo_sub.try_next_message_pure();

        if changed_pdo.is_none() {
            count += 1;
            if count < 10 {
                continue;
            }
        } else {
            match husb238.set_src_pdo(changed_pdo.unwrap()).await {
                Ok(_) => {
                    match husb238.go_command(Command::Request).await {
                        Ok(_) => {
                            count = 0;
                        }
                        Err(_) => {
                            defmt::error!("go command error");
                        }
                    }
                    defmt::info!("set src_pdo: {:?}", changed_pdo.unwrap());
                }
                Err(_) => {
                    defmt::error!("set src_pdo error");
                }
            }
        }

        count = 0;

        match husb238.get_actual_voltage_and_current().await {
            Ok((volts, amps)) => {
                display.update_target_volts(volts.unwrap_or(0.0)).await;
                display.update_limit_amps(amps).await;
            }
            Err(_) => {
                defmt::error!("get actual voltage and current error");
            }
        }

        // Timer::after(Duration::from_millis(1000)).await;
    }
}

async fn get_available_volt_curr<'a>(
    husb238: &mut Husb238<I2cDevice<'a, CriticalSectionRawMutex, I2c<'static, mode::Async>>>,
) -> Result<AvailableVoltCurr, I2cDeviceError<i2c::Error>> {
    Ok(AvailableVoltCurr {
        _5v: husb238.get_5v_status().await?,
        _9v: husb238.get_9v_status().await?,
        _12v: husb238.get_12v_status().await?,
        _15v: husb238.get_15v_status().await?,
        _18v: husb238.get_18v_status().await?,
        _20v: husb238.get_20v_status().await?,
    })
}

#[embassy_executor::task]
async fn btns_exec(mut btn_a: ExtiInput<'static>, mut btn_b: ExtiInput<'static>) {
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
