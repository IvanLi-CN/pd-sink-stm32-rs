use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant};

use crate::{
    button::ButtonState,
    shared::{BTN_A_STATE_CHANNEL, BTN_B_STATE_CHANNEL, MAX_SIMULTANEOUS_PRESS_DELAY},
};

#[derive(PartialEq, Clone, Copy, Debug, defmt::Format)]
pub enum BtnsState {
    Up,
    Down,
    UpLong,
    DownLong,
    UpDbk,
    DownDbk,
    UpAndDown,
    UpAndDownLong,
}

pub enum Direction {
    Normal,
    Reversed,
}

pub struct Controller {
    direction: Direction,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            direction: Direction::Normal,
        }
    }

    pub async fn task(&mut self) {
        let mut btn_up_state = ButtonState::Released;
        let mut btn_down_state = ButtonState::Released;
        let mut up_last = true;

        loop {
            let futures = select(BTN_A_STATE_CHANNEL.receive(), BTN_B_STATE_CHANNEL.receive());

            match futures.await {
                Either::First(s) => {
                    if matches!(self.direction, Direction::Normal) {
                        btn_up_state = s;
                        up_last = true;
                    } else {
                        btn_down_state = s;
                        up_last = false;
                    }
                }
                Either::Second(s) => {
                    if matches!(self.direction, Direction::Normal) {
                        btn_down_state = s;
                        up_last = false;
                    } else {
                        btn_up_state = s;
                        up_last = true;
                    }
                }
            }

            if btn_down_state == ButtonState::Pressed || btn_up_state == ButtonState::Pressed {
                continue;
            }

            if let ButtonState::LongPressed(up_at) = btn_up_state {
                if let ButtonState::LongPressed(down_at) = btn_down_state {
                    if instant_diff(up_at, down_at) < MAX_SIMULTANEOUS_PRESS_DELAY {
                        self.handle_input(BtnsState::UpAndDownLong).await;
                        continue;
                    }
                }
            }

            if let ButtonState::Click(up_at) = btn_up_state {
                if let ButtonState::Click(down_at) = btn_down_state {
                    if instant_diff(up_at, down_at) < MAX_SIMULTANEOUS_PRESS_DELAY {
                        self.handle_input(BtnsState::UpAndDown).await;
                        continue;
                    }
                }
            }

            if up_last {
                if matches!(
                    btn_down_state,
                    ButtonState::Pressed | ButtonState::LongPressed(_)
                ) {
                    continue;
                }

                match btn_up_state {
                    ButtonState::LongPressed(_) => {
                        self.handle_input(BtnsState::UpLong).await;
                    }
                    ButtonState::Click(_) => {
                        self.handle_input(BtnsState::Up).await;
                    }
                    ButtonState::DoubleClick(_) => {
                        self.handle_input(BtnsState::UpDbk).await;
                    }
                    _ => {}
                }
            } else {
                if matches!(
                    btn_up_state,
                    ButtonState::Pressed | ButtonState::LongPressed(_)
                ) {
                    continue;
                }

                match btn_down_state {
                    ButtonState::LongPressed(_) => {
                        self.handle_input(BtnsState::DownLong).await;
                    }
                    ButtonState::Click(_) => {
                        self.handle_input(BtnsState::Down).await;
                    }
                    ButtonState::DoubleClick(_) => {
                        self.handle_input(BtnsState::DownDbk).await;
                    }
                    _ => {}
                }
            }
        }
    }

    async fn handle_input(&mut self, btns: BtnsState) {
        defmt::info!("btns: {:?}", btns);
    }
}

fn instant_diff(a: Instant, b: Instant) -> Duration {
    if a > b {
        a - b
    } else {
        b - a
    }
}
