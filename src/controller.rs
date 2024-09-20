use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant};

use crate::{
    button::ButtonState,
    shared::{
        BACKLIGHT_MUTEX, BTN_A_STATE_CHANNEL, BTN_B_STATE_CHANNEL, DISPLAY_DIRECTION_MUTEX,
        MAX_SIMULTANEOUS_PRESS_DELAY, OCP_MAX, OCP_MUTEX, PAGE_MUTEX, PDO_MUTEX,
        UVP_MUTEX,
    },
    types::{Direction, Page, SettingItem, SETTING_ITEMS},
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

        let mut page = PAGE_MUTEX.lock().await;

        match *page {
            Page::Monitor => match btns {
                BtnsState::Up => {
                    let mut backlight = BACKLIGHT_MUTEX.lock().await;

                    if *backlight > 10 {
                        *backlight = 10;
                    } else {
                        *backlight += 1;
                    }
                }
                BtnsState::Down => {
                    let mut backlight = BACKLIGHT_MUTEX.lock().await;

                    if *backlight < 1 {
                        *backlight = 0;
                    } else {
                        *backlight -= 1;
                    }
                }
                BtnsState::UpLong => {}
                BtnsState::DownLong => {
                    let mut backlight = BACKLIGHT_MUTEX.lock().await;

                    *backlight = 0;
                }
                BtnsState::UpDbk | BtnsState::DownDbk => {
                    let mut direction = DISPLAY_DIRECTION_MUTEX.lock().await;

                    *direction = match *direction {
                        Direction::Normal => Direction::Reversed,
                        Direction::Reversed => Direction::Normal,
                    };
                }
                BtnsState::UpAndDown => {
                    *page = Page::OCP;
                }
                BtnsState::UpAndDownLong => {
                    *page = Page::Setting(SettingItem::Voltage);
                }
            },
            Page::Setting(item) => match btns {
                BtnsState::Up => {
                    let next_index = SETTING_ITEMS
                        .iter()
                        .enumerate()
                        .find(|(_, ele)| **ele == item)
                        .map(|(i, _)| (i + 1) % SETTING_ITEMS.len());

                    *page = Page::Setting(SETTING_ITEMS[next_index.unwrap_or(0)]);
                }
                BtnsState::Down => {
                    let next_index = SETTING_ITEMS
                        .iter()
                        .enumerate()
                        .find(|(_, ele)| **ele == item)
                        .map(|(i, _)| (i + SETTING_ITEMS.len() - 1) % SETTING_ITEMS.len());

                    *page = Page::Setting(SETTING_ITEMS[next_index.unwrap_or(0)]);
                }
                BtnsState::UpLong => {}
                BtnsState::DownLong => {}
                BtnsState::UpDbk | BtnsState::DownDbk => {
                    self.switch_direction().await;
                }
                BtnsState::UpAndDown => {
                    *page = match item {
                        SettingItem::Voltage => Page::Voltage,
                        SettingItem::UVP => Page::UVP,
                        SettingItem::OCP => Page::OCP,
                        SettingItem::About => Page::About,
                    }
                }
                BtnsState::UpAndDownLong => {
                    *page = Page::Monitor;
                }
            },
            Page::Voltage => match btns {
                BtnsState::Up => {
                    let mut pdo = PDO_MUTEX.lock().await;

                    if *pdo > OCP_MAX {
                        *pdo = 10.0;
                    } else {
                        *pdo += 0.25;
                    }
                }
                BtnsState::Down => {
                    let mut ocp = PDO_MUTEX.lock().await;

                    if *ocp < 10.0 {
                        *ocp = 0.0;
                    } else {
                        *ocp -= 0.25;
                    }
                }
                BtnsState::UpAndDown => {
                    *page = Page::Setting(SettingItem::UVP);
                }
                BtnsState::UpDbk | BtnsState::DownDbk => {
                    self.switch_direction().await;
                }
                _ => {}
            },
            Page::UVP => match btns {
                BtnsState::Up => {
                    let mut uvp = UVP_MUTEX.lock().await;

                    if *uvp > OCP_MAX {
                        *uvp = 10.0;
                    } else {
                        *uvp += 0.25;
                    }
                }
                BtnsState::Down => {
                    let mut ocp = UVP_MUTEX.lock().await;

                    if *ocp < 10.0 {
                        *ocp = 0.0;
                    } else {
                        *ocp -= 0.25;
                    }
                }
                BtnsState::UpAndDown => {
                    *page = Page::Setting(SettingItem::UVP);
                }
                BtnsState::UpDbk | BtnsState::DownDbk => {
                    self.switch_direction().await;
                }
                _ => {}
            },
            Page::OCP => match btns {
                BtnsState::Up => {
                    let mut ocp = OCP_MUTEX.lock().await;

                    if *ocp > OCP_MAX {
                        *ocp = 10.0;
                    } else {
                        *ocp += 0.25;
                    }
                }
                BtnsState::Down => {
                    let mut ocp = OCP_MUTEX.lock().await;

                    if *ocp < 10.0 {
                        *ocp = 0.0;
                    } else {
                        *ocp -= 0.25;
                    }
                }
                BtnsState::UpAndDown => {
                    *page = Page::Setting(SettingItem::OCP);
                }
                BtnsState::UpDbk | BtnsState::DownDbk => {
                    self.switch_direction().await;
                }
                _ => {}
            },
            Page::About => match btns {
                BtnsState::UpDbk | BtnsState::DownDbk => {
                    self.switch_direction().await;
                }
                _ => {
                    *page = Page::Setting(SettingItem::About);
                }
            },
        }

        let backlight = BACKLIGHT_MUTEX.lock().await;

        let ocp = OCP_MUTEX.lock().await;
        let uvp = UVP_MUTEX.lock().await;
        let pdo = PDO_MUTEX.lock().await;

        defmt::info!(
            "page: {:?}, direction: {:?}, backlight: {:?}, ocp: {:?}, uvp: {:?}, pdo: {:?}",
            *page,
            self.direction,
            *backlight,
            *ocp,
            *uvp,
            *pdo
        );
    }

    async fn switch_direction(&mut self) {
        let mut direction = DISPLAY_DIRECTION_MUTEX.lock().await;

        *direction = match *direction {
            Direction::Normal => Direction::Reversed,
            Direction::Reversed => Direction::Normal,
        };

        self.direction = *direction;
    }
}

fn instant_diff(a: Instant, b: Instant) -> Duration {
    if a > b {
        a - b
    } else {
        b - a
    }
}
