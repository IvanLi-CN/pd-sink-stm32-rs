use embassy_stm32::gpio::{Level, Output, Pin};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pubsub::Subscriber};

use crate::shared::OUTPUT_PUBSUB;

pub(crate) struct OutputController<'a> {
    output_sub: Subscriber<'a, CriticalSectionRawMutex, bool, 2, 2, 1>,
    ctrl_pin: Output<'a>,
}

impl<'a> OutputController<'a> {
    pub fn new(ctrl_pin: Output<'a>) -> Self {
        Self {
            output_sub: OUTPUT_PUBSUB.subscriber().unwrap(),
            ctrl_pin,
        }
    }

    pub async fn set_output(&mut self, enable: bool) {
        self.ctrl_pin
            .set_level(if enable { Level::High } else { Level::Low });
    }

    pub async fn task(&mut self) {
        let output = self.output_sub.try_next_message_pure();

        if let Some(output) = output {
            self.set_output(output).await;
        }
    }
}
