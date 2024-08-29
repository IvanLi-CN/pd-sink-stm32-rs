//! This example shows a complete project, including file structure, and config
//! needed to flash using an ST-Link. The project structure is based on
//! [Knurling's app-template](https://github.com/knurling-rs/app-template).
//! This file demonstrates an overview of this library's features.

//! See the syntax example in the main STM32-HAL repo for a more detailed example.

#![no_main]
#![no_std]

use cortex_m::{self, asm::delay};
use cortex_m_rt::entry;
use defmt::println;
// These lines are part of our setup for debug printing.
use defmt_rtt as _;
use panic_probe as _;
// Import parts of this library we use. You could use this style, or perhaps import
// less here.
use hal::{self, gpio::{Pin, PinMode, Port}, low_power, pac};

mod init;
mod setup;
mod system_status;

pub struct Config {}

// #[rtic::app(device = pac, peripherals = false)]
// mod app {
//     use super::*;
//
//     #[shared]
//     pub struct Shared {
//         pub config: Config,
//         pub system_status: system_status::SystemStatus,
//     }
//
//     #[local]
//     pub struct Local {}
//
//     #[init]
//     fn init(cx: init::Context) -> (Shared, Local) {
//         crate::init::run(cx)
//     }
//
//     #[idle(shared = [], local = [])]
//     /// In this function, we perform setup code that must occur with interrupts enabled.
//     fn idle(_cx: idle::Context) -> ! {
//         loop {
//             asm::nop();
//         }
//     }
// }

#[entry]
fn main() -> ! {
    // This line is required to prevent the debugger from disconnecting on entering WFI.
    // This appears to be a limitation of many STM32 families. Not required in production code,
    // and significantly increases power consumption in low-power modes. Not required if not using WFI.
    // hal::debug_workaround();

    init::run();

    let mut led = Pin::new(Port::B, 8, PinMode::Output);

    loop {
        // low_power::sleep_now();
        led.toggle();
        delay(1_000_000);
        cortex_m::asm::nop();
    }
}

// same panicking *behavior* as `panic-probe` but doesn't print a panic message
// this prevents the panic message being printed *twice* when `defmt::panic` is invoked
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}
