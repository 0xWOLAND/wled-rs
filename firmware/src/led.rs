//! Owns the WS2812 strip. A dedicated thread applies state changes and persists them once they settle.

// ws2812-esp32-rmt-driver 0.14 (the release matching esp-idf-hal 0.46) drives the legacy RMT API.
#![allow(deprecated)]

use std::{
    sync::{
        mpsc::{self, RecvTimeoutError},
        Arc, Mutex,
    },
    thread,
};

use esp_idf_svc::hal::{gpio::OutputPin, rmt::RmtChannel};
use protocol::LightState;
use ws2812_esp32_rmt_driver::Ws2812Esp32RmtDriver;

use crate::{
    config::{LED_COUNT, PERSIST_DELAY},
    store::Store,
};

/// Cheap, cloneable handle used by the HTTP handlers.
#[derive(Clone)]
pub struct Light {
    current: Arc<Mutex<LightState>>,
    tx: mpsc::Sender<LightState>,
}

impl Light {
    pub fn start(
        channel: impl RmtChannel + 'static,
        pin: impl OutputPin + 'static,
        store: Store,
        initial: LightState,
    ) -> anyhow::Result<Self> {
        let mut strip = Ws2812Esp32RmtDriver::new(channel, pin)?;
        write(&mut strip, &initial)?;

        let (tx, rx) = mpsc::channel::<LightState>();
        thread::Builder::new()
            .name("led".into())
            .stack_size(4096)
            .spawn(move || {
                let mut unsaved = None;
                loop {
                    let next = match unsaved {
                        Some(_) => rx.recv_timeout(PERSIST_DELAY),
                        None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                    };
                    match next {
                        Ok(state) => {
                            if let Err(e) = write(&mut strip, &state) {
                                log::error!("LED write failed: {e}");
                            }
                            unsaved = Some(state);
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            if let Some(state) = unsaved.take() {
                                if let Err(e) = store.save(&state) {
                                    log::error!("saving light state failed: {e}");
                                }
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })?;

        Ok(Self {
            current: Arc::new(Mutex::new(initial)),
            tx,
        })
    }

    pub fn get(&self) -> LightState {
        *self.current.lock().unwrap()
    }

    pub fn set(&self, state: LightState) {
        *self.current.lock().unwrap() = state;
        // The LED thread never exits while a `Light` exists, so sending cannot fail.
        let _ = self.tx.send(state);
    }
}

fn write(strip: &mut Ws2812Esp32RmtDriver<'_>, state: &LightState) -> anyhow::Result<()> {
    let c = state.color();
    // WS2812 expects GRB byte order.
    let pixel = [c.g, c.r, c.b];
    strip.write_blocking(pixel.into_iter().cycle().take(3 * LED_COUNT))?;
    Ok(())
}
