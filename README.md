# STM32-HAL quickstart for STM32G071GBUx

This repo provides a starting point for new [STM32-HAL](https://github.com/David-OConnor/stm32-hal)
projects. It's based on the [Knurling app template](https://github.com/knurling-rs/app-template).

**Now, it's modified to be adaptable to STM32G071G8Ux.**
Based on the [stm32-hal-quickstart1](https://github.com/David-OConnor/stm32-hal-quickstart1) project.

## Quickstart

- [Install Rust](https://www.rust-lang.org/tools/install).
- Install the compilation target for your MCU. For example: `rustup target add thumbv6m-none-eabi`.
- Install flash and debug tools: `cargo install flip-link`, `cargo install probe-run`.
- Clone this repo: `git clone git@github.com:IvanLi-CN/stm32-hal-stm32g071gbux-quickstart.git`
- Connect your device. Run `cargo run --release` to compile and flash.
