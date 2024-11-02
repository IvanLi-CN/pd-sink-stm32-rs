use core::convert::Infallible;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pubsub::Subscriber};
use embedded_graphics::{pixelcolor::Rgb565, prelude::WebColors};
use embedded_hal::digital::OutputPin;
use embedded_hal_async::spi::SpiDevice;
use husb238::SrcPdo;
use st7789::ST7789;

use crate::{
    font::{
        get_index_by_char, ARIAL_ROUND_16_24, ARIAL_ROUND_16_24_INDEX, GROTESK_24_48,
        GROTESK_24_48_INDEX,
    },
    shared::{
        AVAILABLE_VOLT_CURR_MUTEX, COLOR_AMPERAGE, COLOR_BACKGROUND, COLOR_BASE, COLOR_ON_TEXT,
        COLOR_PRIMARY, COLOR_PRIMARY_CONTENT, COLOR_TEXT, COLOR_TEXT_DISABLED, COLOR_VOLTAGE,
        COLOR_WATTAGE, DISPLAY_DIRECTION_PUBSUB, OUTPUT_PUBSUB, PAGE_PUBSUB,
    },
    types::{
        Direction, Page, PowerInfo, SettingItem, StatusInfo, OCP_ITEMS, SETTING_ITEMS,
        VOLTAGE_ITEMS,
    },
};

const MONITOR_PRIMARY_X: u16 = 14;
const MONITOR_PRIMARY_Y1: u16 = 6;
const MONITOR_PRIMARY_Y2: u16 = 62;
const MONITOR_PRIMARY_Y3: u16 = 118;
const MONITOR_PRIMARY_UINT_X: u16 = 138;
const MONITOR_PRIMARY_UINT_Y1: u16 = 30;
const MONITOR_PRIMARY_UINT_Y2: u16 = 86;
const MONITOR_PRIMARY_UINT_Y3: u16 = 142;
const MONITOR_SECONDARY_X1: u16 = 164;
const MONITOR_SECONDARY_X2: u16 = 222;
const MONITOR_SECONDARY_X3: u16 = 286;
const MONITOR_SECONDARY_Y1: u16 = 4;
const MONITOR_SECONDARY_Y2: u16 = 32;
const MONITOR_SECONDARY_Y3: u16 = 60;
const MONITOR_SECONDARY_Y4: u16 = 88;
const MONITOR_SECONDARY_Y5: u16 = 116;
const MONITOR_SECONDARY_Y6: u16 = 144;

pub struct Display<'a, SPI, DC, RST>
where
    SPI: SpiDevice,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
{
    st7789: ST7789<SPI, DC, RST>,
    power_info: PowerInfo,
    status_info: StatusInfo,
    ryu_buffer: ryu::Buffer,
    prev_ryu_buffer: ryu::Buffer,
    force_render: bool,

    page: Page,
    direction: Direction,
    output: bool,

    page_sub: Subscriber<'a, CriticalSectionRawMutex, Page, 2, 2, 1>,
    direction_sub: Subscriber<'a, CriticalSectionRawMutex, Direction, 2, 2, 1>,
    output_sub: Subscriber<'a, CriticalSectionRawMutex, bool, 2, 2, 1>,
}

impl<'a, SPI, DC, RST> Display<'a, SPI, DC, RST>
where
    SPI: SpiDevice,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
{
    pub fn new(st7789: ST7789<SPI, DC, RST>) -> Self {
        Self {
            st7789,
            power_info: PowerInfo::default(),
            status_info: StatusInfo::default(),
            ryu_buffer: ryu::Buffer::new(),
            prev_ryu_buffer: ryu::Buffer::new(),
            force_render: true,

            page: Page::Monitor,
            direction: Direction::Normal,
            output: false,

            page_sub: PAGE_PUBSUB.subscriber().unwrap(),
            direction_sub: DISPLAY_DIRECTION_PUBSUB.subscriber().unwrap(),
            output_sub: OUTPUT_PUBSUB.subscriber().unwrap(),
        }
    }

    async fn reinit(&mut self) -> Result<(), ()> {
        self.force_render = true;

        self.st7789
            .set_orientation(match self.direction {
                Direction::Normal => st7789::Orientation::Landscape,
                Direction::Reversed => st7789::Orientation::LandscapeSwapped,
            })
            .await
            .map_err(|_| ())?;

        self.update_layout().await;

        self.force_render = false;
        Ok(())
    }

    pub async fn init(&mut self) -> Result<(), ()> {
        self.st7789.init().await.map_err(|_| ())?;

        self.reinit().await
    }

    pub async fn update_monitor_volts(&mut self, volts: f64) {
        if !matches!(self.page, Page::Monitor) {
            return;
        }

        let curr = self.ryu_buffer.format(volts);
        let prev = self.prev_ryu_buffer.format(self.power_info.volts);

        Self::render_monitor(
            &mut self.st7789,
            curr,
            prev,
            MONITOR_PRIMARY_Y1,
            COLOR_BACKGROUND,
            COLOR_VOLTAGE,
            self.force_render,
        )
        .await;

        self.power_info.volts = volts;
    }

    pub async fn update_monitor_amps(&mut self, amps: f64) {
        if !matches!(self.page, Page::Monitor) {
            return;
        }

        let amps = if amps < 0.0001 { 0.0 } else { amps };
        let curr = self.ryu_buffer.format(amps);
        let prev = self.prev_ryu_buffer.format(self.power_info.amps);

        Self::render_monitor(
            &mut self.st7789,
            curr,
            prev,
            MONITOR_PRIMARY_Y2,
            COLOR_BACKGROUND,
            COLOR_AMPERAGE,
            self.force_render,
        )
        .await;

        self.power_info.amps = amps;
    }

    pub async fn update_monitor_watts(&mut self, watts: f64) {
        if !matches!(self.page, Page::Monitor) {
            return;
        }

        let curr = self.ryu_buffer.format(watts);
        let prev = self.prev_ryu_buffer.format(self.power_info.watts);

        Self::render_monitor(
            &mut self.st7789,
            curr,
            prev,
            MONITOR_PRIMARY_Y3,
            COLOR_BACKGROUND,
            COLOR_WATTAGE,
            self.force_render,
        )
        .await;

        self.power_info.watts = watts;
    }

    pub async fn update_target_volts(&mut self, volts: f64) {
        if !matches!(self.page, Page::Monitor) {
            return;
        }

        if self.status_info.target_volts == volts && !self.force_render {
            return;
        }

        self.status_info.target_volts = volts;

        let curr = self.ryu_buffer.format(self.status_info.target_volts);

        Self::render_status(
            &mut self.st7789,
            curr,
            MONITOR_SECONDARY_X2,
            MONITOR_SECONDARY_Y1,
            COLOR_BACKGROUND,
            COLOR_TEXT,
            4,
        )
        .await;
    }

    pub async fn update_limit_amps(&mut self, amps: f64) {
        if !matches!(self.page, Page::Monitor) {
            return;
        }

        if self.status_info.limit_amps == amps && !self.force_render {
            return;
        }

        self.status_info.limit_amps = amps;

        let curr: &str = self.ryu_buffer.format(self.status_info.limit_amps);

        Self::render_status(
            &mut self.st7789,
            curr,
            MONITOR_SECONDARY_X2,
            MONITOR_SECONDARY_Y2,
            COLOR_BACKGROUND,
            COLOR_TEXT,
            4,
        )
        .await;
    }

    pub async fn update_ocp_amps(&mut self, amps: f64) {
        if !matches!(self.page, Page::Monitor) {
            return;
        }

        if self.status_info.ocp_amps == amps && !self.force_render {
            return;
        }

        self.status_info.ocp_amps = amps;

        let curr: &str = self.ryu_buffer.format(self.status_info.ocp_amps);

        Self::render_status(
            &mut self.st7789,
            curr,
            MONITOR_SECONDARY_X2,
            MONITOR_SECONDARY_Y3,
            COLOR_BACKGROUND,
            COLOR_TEXT,
            4,
        )
        .await;
    }

    pub async fn update_output(&mut self, output: bool) {
        if !matches!(self.page, Page::Monitor) {
            return;
        }

        if self.output == output && !self.force_render {
            return;
        }

        self.status_info.output = output;

        Self::render_status(
            &mut self.st7789,
            if output { "ON " } else { "OFF" },
            MONITOR_SECONDARY_X2,
            MONITOR_SECONDARY_Y5,
            COLOR_BACKGROUND,
            if output { COLOR_ON_TEXT } else { COLOR_TEXT },
            3,
        )
        .await;
    }

    pub async fn update_layout(&mut self) {
        self.st7789.fill_color(COLOR_BACKGROUND).await.unwrap();

        match self.page {
            Page::Monitor => {
                self.update_monitor_layout().await;

                self.force_render = true;

                self.update_monitor_amps(self.power_info.amps).await;
                self.update_monitor_volts(self.power_info.volts).await;
                self.update_monitor_watts(self.power_info.watts).await;

                self.update_target_volts(self.status_info.target_volts)
                    .await;
                self.update_limit_amps(self.status_info.limit_amps).await;
                self.update_ocp_amps(self.status_info.ocp_amps).await;
                self.update_output(self.output).await;

                self.force_render = false;
            }
            Page::Setting(setting_item) => self.update_setting_layout(setting_item).await,
            Page::Voltage(selected) => {
                self.update_setting_layout(SettingItem::Voltage).await;
                self.update_voltage_layout(selected).await;
            }
            Page::UVP => self.update_monitor_layout().await,
            Page::OCP(selected) => {
                self.update_setting_layout(SettingItem::Voltage).await;
                self.update_ocp_layout(selected).await;
            }
            Page::About => {
                self.update_setting_layout(SettingItem::About).await;
                self.update_about_layout().await;
            }
        }
    }

    pub async fn update_monitor_layout(&mut self) {
        Self::render_status(
            &mut self.st7789,
            "V",
            MONITOR_PRIMARY_UINT_X,
            MONITOR_PRIMARY_UINT_Y1,
            COLOR_BACKGROUND,
            COLOR_VOLTAGE,
            1,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "A",
            MONITOR_PRIMARY_UINT_X,
            MONITOR_PRIMARY_UINT_Y2,
            COLOR_BACKGROUND,
            COLOR_AMPERAGE,
            1,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "W",
            MONITOR_PRIMARY_UINT_X,
            MONITOR_PRIMARY_UINT_Y3,
            COLOR_BACKGROUND,
            COLOR_WATTAGE,
            1,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "PDO",
            MONITOR_SECONDARY_X1,
            MONITOR_SECONDARY_Y1,
            COLOR_BACKGROUND,
            COLOR_BASE,
            3,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "OCP",
            MONITOR_SECONDARY_X1,
            MONITOR_SECONDARY_Y3,
            COLOR_BACKGROUND,
            COLOR_BASE,
            3,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "Out",
            MONITOR_SECONDARY_X1,
            MONITOR_SECONDARY_Y5,
            COLOR_BACKGROUND,
            COLOR_BASE,
            3,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "V",
            MONITOR_SECONDARY_X3,
            MONITOR_SECONDARY_Y1,
            COLOR_BACKGROUND,
            COLOR_BASE,
            1,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "A",
            MONITOR_SECONDARY_X3,
            MONITOR_SECONDARY_Y2,
            COLOR_BACKGROUND,
            COLOR_BASE,
            1,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "A",
            MONITOR_SECONDARY_X3,
            MONITOR_SECONDARY_Y3,
            COLOR_BACKGROUND,
            COLOR_BASE,
            1,
        )
        .await;
    }

    pub async fn update_setting_layout(&mut self, setting_item: SettingItem) {
        let line_bytes = [0xff_u8; 43];
        self.st7789
            .write_area(
                160,
                0,
                2,
                &line_bytes,
                Rgb565::CSS_DARK_GRAY,
                Rgb565::CSS_DARK_GRAY,
            )
            .await
            .unwrap();

        let offset = SETTING_ITEMS
            .iter()
            .enumerate()
            .find(|(_, ele)| **ele == setting_item)
            .map(|(i, _)| i)
            .unwrap_or(0);

        for i in 0..SETTING_ITEMS.len().min(5) {
            let idx = (offset + i + SETTING_ITEMS.len() - 2) % SETTING_ITEMS.len();
            let item = SETTING_ITEMS[idx];

            let (color, bg_color) = if item == setting_item {
                (COLOR_PRIMARY_CONTENT, COLOR_PRIMARY)
            } else {
                (COLOR_TEXT, COLOR_BACKGROUND)
            };

            let text = match item {
                SettingItem::Voltage => "  PDO  ",
                SettingItem::UVP => "  UVP  ",
                SettingItem::OCP => "  OCP  ",
                SettingItem::About => " About ",
            };

            let x = 10;
            let y = (i as u16) * 34;

            Self::render_status(
                &mut self.st7789,
                text,
                x,
                y,
                bg_color,
                color,
                text.len() as u16,
            )
            .await;
        }
    }

    pub async fn update_about_layout(&mut self) {
        Self::render_status(
            &mut self.st7789,
            "Author:",
            170,
            10,
            COLOR_BACKGROUND,
            COLOR_TEXT,
            7,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "  Ivan Li",
            170,
            30,
            COLOR_BACKGROUND,
            COLOR_TEXT,
            9,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "Version:",
            170,
            60,
            COLOR_BACKGROUND,
            COLOR_TEXT,
            8,
        )
        .await;

        Self::render_status(
            &mut self.st7789,
            "  0.1.0",
            170,
            90,
            COLOR_BACKGROUND,
            COLOR_TEXT,
            7,
        )
        .await;
    }

    pub async fn update_voltage_layout(&mut self, selected: SrcPdo) {
        defmt::info!("selected: {:?}", selected);

        let available_volt_curr = AVAILABLE_VOLT_CURR_MUTEX.lock().await;

        let offset = VOLTAGE_ITEMS
            .iter()
            .enumerate()
            .find(|(_, ele)| **ele == selected)
            .map(|(i, _)| i)
            .unwrap_or(0);

        for i in 0..VOLTAGE_ITEMS.len().min(5) {
            let idx = (offset + i + VOLTAGE_ITEMS.len() - 2) % VOLTAGE_ITEMS.len();
            let item = VOLTAGE_ITEMS[idx];

            let (color, bg_color) = if item == selected {
                (COLOR_PRIMARY_CONTENT, COLOR_PRIMARY)
            } else {
                let available = match item {
                    SrcPdo::_5v => true,
                    SrcPdo::_9v => available_volt_curr._9v.is_some(),
                    SrcPdo::_12v => available_volt_curr._12v.is_some(),
                    SrcPdo::_15v => available_volt_curr._15v.is_some(),
                    SrcPdo::_18v => available_volt_curr._18v.is_some(),
                    SrcPdo::_20v => available_volt_curr._20v.is_some(),
                    _ => false,
                };

                if available {
                    (COLOR_TEXT, COLOR_BACKGROUND)
                } else {
                    (COLOR_TEXT_DISABLED, COLOR_BACKGROUND)
                }
            };

            let text = match item {
                SrcPdo::_5v => "  5V  ",
                SrcPdo::_9v => "  9V  ",
                SrcPdo::_12v => " 12V  ",
                SrcPdo::_15v => " 15V  ",
                SrcPdo::_18v => " 18V  ",
                SrcPdo::_20v => " 20V  ",
                _ => "MISSING",
            };

            let x = 170;
            let y = (i as u16) * 38;

            Self::render_status(
                &mut self.st7789,
                text,
                x,
                y,
                bg_color,
                color,
                text.len() as u16,
            )
            .await;
        }
    }

    pub async fn update_ocp_layout(&mut self, selected: f64) {
        let offset = OCP_ITEMS
            .iter()
            .enumerate()
            .find(|(_, ele)| **ele == selected)
            .map(|(i, _)| i)
            .unwrap_or(0);

        for i in 0..OCP_ITEMS.len().min(5) {
            let idx = (offset + i + OCP_ITEMS.len() - 2) % OCP_ITEMS.len();
            let item = OCP_ITEMS[idx];

            let (color, bg_color) = if item == selected {
                (COLOR_PRIMARY_CONTENT, COLOR_PRIMARY)
            } else {
                (COLOR_TEXT, COLOR_BACKGROUND)
            };

            let text = self.ryu_buffer.format(item);

            let x = 170;
            let y = (i as u16) * 38;

            Self::render_status(
                &mut self.st7789,
                text,
                x,
                y,
                bg_color,
                color,
                text.len() as u16,
            )
            .await;

            // Unit
            Self::render_status(
                &mut self.st7789,
                "A",
                x + (text.len() as u16) * 16 + 6,
                y,
                bg_color,
                color,
                1,
            )
            .await;
        }
    }

    pub async fn task(&mut self) {
        let direction = self.direction_sub.try_next_message_pure();

        if let Some(direction) = direction {
            self.direction = direction;

            self.reinit().await.unwrap();
        }

        let page = self.page_sub.try_next_message_pure();

        if let Some(page) = page {
            self.page = page;

            self.update_layout().await;
        }

        let output = self.output_sub.try_next_message_pure();

        if let Some(output) = output {
            self.update_output(output).await;
            self.output = output;
        }
    }

    async fn render_monitor(
        st7789: &mut ST7789<SPI, DC, RST>,
        curr: &str,
        prev: &str,
        y: u16,
        bg_color: Rgb565,
        color: Rgb565,
        force_render: bool,
    ) {
        let mut chars = curr.chars();
        let mut chars_prev = prev.chars();

        for idx in 0..5 {
            let char = chars.next();
            if char == chars_prev.next() {
                if !force_render {
                    continue;
                }
            }

            let char = match char {
                Some(c) => c,
                None => '0',
            };

            st7789
                .write_area(
                    MONITOR_PRIMARY_X + idx * 24,
                    y,
                    24,
                    GROTESK_24_48[get_index_by_char(GROTESK_24_48_INDEX, char)],
                    color,
                    bg_color,
                )
                .await
                .unwrap();
        }
    }

    async fn render_status(
        st7789: &mut ST7789<SPI, DC, RST>,
        curr: &str,
        x: u16,
        y: u16,
        bg_color: Rgb565,
        color: Rgb565,
        len: u16,
    ) {
        let mut chars = curr.chars();

        for idx in 0..len {
            let char = chars.next();

            let char = match char {
                Some(c) => c,
                None => '0',
            };

            st7789
                .write_area(
                    x + idx * 16,
                    y,
                    16,
                    ARIAL_ROUND_16_24[get_index_by_char(ARIAL_ROUND_16_24_INDEX, char)],
                    color,
                    bg_color,
                )
                .await
                .unwrap();
        }
    }
}
