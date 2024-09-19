use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::Mutex};
use embassy_time::Duration;

use crate::{
    button::ButtonState,
    display::Display,
    types::{ST7789DCPin, ST7789RstPin, ST7789SpiDev},
};

pub const MIN_PRESS_DURATION: Duration = Duration::from_millis(50);
pub const SHORT_PRESS_DURATION: Duration = Duration::from_millis(200);
pub const DOUBLE_CLICK_TIMEOUT: Duration = Duration::from_millis(300);
pub const MAX_SIMULTANEOUS_PRESS_DELAY: Duration = Duration::from_millis(100);

pub static DISPLAY: Mutex<
    CriticalSectionRawMutex,
    Option<Display<ST7789SpiDev, ST7789DCPin, ST7789RstPin>>,
> = Mutex::new(None);

pub(crate) static BTN_A_STATE_CHANNEL: Channel<CriticalSectionRawMutex, ButtonState, 10> =
    Channel::new();
pub(crate) static BTN_B_STATE_CHANNEL: Channel<CriticalSectionRawMutex, ButtonState, 10> =
    Channel::new();
