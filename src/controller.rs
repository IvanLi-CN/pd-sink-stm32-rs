use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pubsub::ImmediatePublisher};
use embassy_time::{Duration, Instant};

use crate::{
    button::ButtonState,
    shared::{
        BACKLIGHT_MUTEX, BACKLIGHT_PUBSUB, BTN_A_STATE_CHANNEL, BTN_B_STATE_CHANNEL,
        DISPLAY_DIRECTION_MUTEX, DISPLAY_DIRECTION_PUBSUB, MAX_SIMULTANEOUS_PRESS_DELAY, OCP_MAX,
        OCP_MUTEX, OCP_PUBSUB, PAGE_MUTEX, PAGE_PUBSUB, PDO_MUTEX, PDO_PUBSUB, UVP_MUTEX,
        UVP_PUBSUB,
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

pub struct Controller<'a> {
    direction: Direction,

    page_pubsub: ImmediatePublisher<'a, CriticalSectionRawMutex, Page, 2, 2, 1>,
    backlight_pubsub: ImmediatePublisher<'a, CriticalSectionRawMutex, u16, 2, 2, 1>,
    display_direction_pubsub: ImmediatePublisher<'a, CriticalSectionRawMutex, Direction, 2, 2, 1>,
    ocp_pubsub: ImmediatePublisher<'a, CriticalSectionRawMutex, f64, 2, 2, 1>,
    uvp_pubsub: ImmediatePublisher<'a, CriticalSectionRawMutex, f64, 2, 2, 1>,
    pdo_pubsub: ImmediatePublisher<'a, CriticalSectionRawMutex, f64, 2, 2, 1>,
}

impl<'a> Controller<'a> {
    pub fn new() -> Self {
        Self {
            direction: Direction::Normal,

            page_pubsub: PAGE_PUBSUB.immediate_publisher(),
            backlight_pubsub: BACKLIGHT_PUBSUB.immediate_publisher(),
            display_direction_pubsub: DISPLAY_DIRECTION_PUBSUB.immediate_publisher(),
            ocp_pubsub: OCP_PUBSUB.immediate_publisher(),
            uvp_pubsub: UVP_PUBSUB.immediate_publisher(),
            pdo_pubsub: PDO_PUBSUB.immediate_publisher(),
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

                    let _backlight = *backlight;

                    drop(backlight);

                    self.backlight_pubsub.publish_immediate(_backlight);
                }
                BtnsState::Down => {
                    let mut backlight = BACKLIGHT_MUTEX.lock().await;

                    if *backlight < 1 {
                        *backlight = 0;
                    } else {
                        *backlight -= 1;
                    }

                    let _backlight = *backlight;

                    drop(backlight);

                    self.backlight_pubsub.publish_immediate(_backlight);
                }
                BtnsState::UpLong => {}
                BtnsState::DownLong => {
                    let mut backlight = BACKLIGHT_MUTEX.lock().await;

                    *backlight = 0;

                    let _backlight = *backlight;

                    drop(backlight);

                    self.backlight_pubsub.publish_immediate(_backlight);
                }
                BtnsState::UpDbk | BtnsState::DownDbk => {
                    let mut direction = DISPLAY_DIRECTION_MUTEX.lock().await;

                    *direction = match *direction {
                        Direction::Normal => Direction::Reversed,
                        Direction::Reversed => Direction::Normal,
                    };

                    let _direction = *direction;

                    drop(direction);

                    self.display_direction_pubsub.publish_immediate(_direction);
                }
                BtnsState::UpAndDown => {
                    *page = Page::OCP;

                    let _page = *page;

                    drop(page);

                    self.page_pubsub.publish_immediate(_page);
                }
                BtnsState::UpAndDownLong => {
                    *page = Page::Setting(SettingItem::Voltage);

                    let _page = *page;

                    drop(page);

                    self.page_pubsub.publish_immediate(_page);
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

                    let _page = *page;

                    drop(page);

                    self.page_pubsub.publish_immediate(_page);
                }
                BtnsState::Down => {
                    let next_index = SETTING_ITEMS
                        .iter()
                        .enumerate()
                        .find(|(_, ele)| **ele == item)
                        .map(|(i, _)| (i + SETTING_ITEMS.len() - 1) % SETTING_ITEMS.len());

                    *page = Page::Setting(SETTING_ITEMS[next_index.unwrap_or(0)]);

                    let _page = *page;

                    drop(page);

                    self.page_pubsub.publish_immediate(_page);
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
                    };

                    let _page = *page;

                    drop(page);

                    self.page_pubsub.publish_immediate(_page);
                }
                BtnsState::UpAndDownLong => {
                    *page = Page::Monitor;

                    let _page = *page;

                    drop(page);

                    self.page_pubsub.publish_immediate(_page);
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

                    let _pdo = *pdo;

                    drop(pdo);

                    self.pdo_pubsub.publish_immediate(_pdo);
                }
                BtnsState::Down => {
                    let mut pdo = PDO_MUTEX.lock().await;

                    if *pdo < 10.0 {
                        *pdo = 0.0;
                    } else {
                        *pdo -= 0.25;
                    }

                    let _pdo = *pdo;

                    drop(pdo);

                    self.pdo_pubsub.publish_immediate(_pdo);
                }
                BtnsState::UpAndDown => {
                    *page = Page::Setting(SettingItem::UVP);

                    let _page = *page;

                    drop(page);

                    self.page_pubsub.publish_immediate(_page);
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

                    let _uvp = *uvp;

                    drop(uvp);

                    self.uvp_pubsub.publish_immediate(_uvp);
                }
                BtnsState::Down => {
                    let mut uvp = UVP_MUTEX.lock().await;

                    if *uvp < 10.0 {
                        *uvp = 0.0;
                    } else {
                        *uvp -= 0.25;
                    }

                    let _uvp = *uvp;

                    drop(uvp);

                    self.uvp_pubsub.publish_immediate(_uvp);
                }
                BtnsState::UpAndDown => {
                    *page = Page::Setting(SettingItem::UVP);

                    let _page = *page;

                    drop(page);

                    self.page_pubsub.publish_immediate(_page);
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

                    let _ocp = *ocp;

                    drop(ocp);

                    self.ocp_pubsub.publish_immediate(_ocp);
                }
                BtnsState::Down => {
                    let mut ocp = OCP_MUTEX.lock().await;

                    if *ocp < 10.0 {
                        *ocp = 0.0;
                    } else {
                        *ocp -= 0.25;
                    }

                    let _ocp = *ocp;

                    drop(ocp);

                    self.ocp_pubsub.publish_immediate(_ocp);
                }
                BtnsState::UpAndDown => {
                    *page = Page::Setting(SettingItem::OCP);

                    let _page = *page;

                    drop(page);

                    self.page_pubsub.publish_immediate(_page);
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

                    let _page = *page;

                    drop(page);

                    self.page_pubsub.publish_immediate(_page);
                }
            },
        }
    }

    async fn switch_direction(&mut self) {
        let mut direction = DISPLAY_DIRECTION_MUTEX.lock().await;

        *direction = match *direction {
            Direction::Normal => Direction::Reversed,
            Direction::Reversed => Direction::Normal,
        };

        self.direction = *direction;

        let _direction = *direction;

        drop(direction);

        self.display_direction_pubsub.publish_immediate(_direction);
    }
}

fn instant_diff(a: Instant, b: Instant) -> Duration {
    if a > b {
        a - b
    } else {
        b - a
    }
}
