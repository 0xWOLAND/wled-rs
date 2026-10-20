//! Owns the WS2812 strip. A dedicated thread applies state changes, animates effects and persists
//! the state once it settles.

// ws2812-esp32-rmt-driver 0.14 (the release matching esp-idf-hal 0.46) drives the legacy RMT API.
#![allow(deprecated)]

use std::{
    mem,
    sync::{
        mpsc::{self, RecvTimeoutError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use esp_idf_svc::hal::{gpio::OutputPin, rmt::RmtChannel};
use protocol::{LightState, Mode, Rgb};
use smart_led_effects::strip::{EffectIterator, Rainbow, Twinkle};
use ws2812_esp32_rmt_driver::Ws2812Esp32RmtDriver;

use crate::{
    config::{LED_COUNT, PERSIST_DELAY},
    store::Store,
};

/// Effect frame interval (~33 fps).
const FRAME: Duration = Duration::from_millis(30);

/// Cheap, cloneable handle used by the HTTP handlers and the switch.
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
        let strip = Ws2812Esp32RmtDriver::new(channel, pin)?;
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("led".into())
            .stack_size(8192)
            .spawn(move || run(strip, rx, store, initial))?;

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

fn run(mut strip: Ws2812Esp32RmtDriver<'static>, rx: mpsc::Receiver<LightState>, store: Store, initial: LightState) {
    let mut renderer = Renderer::new(initial);
    let mut next_frame = Instant::now();
    let mut save_at: Option<Instant> = None;

    loop {
        let now = Instant::now();
        if renderer.is_animated() && now >= next_frame {
            show(&mut strip, renderer.frame());
            next_frame = (next_frame + FRAME).max(now);
        }
        if save_at.is_some_and(|t| now >= t) {
            save_at = None;
            if let Err(e) = store.save(&renderer.state) {
                log::error!("saving light state failed: {e}");
            }
        }

        let wake = [renderer.is_animated().then_some(next_frame), save_at].into_iter().flatten().min();
        let received = match wake {
            Some(t) => rx.recv_timeout(t.saturating_duration_since(Instant::now())),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(state) => {
                renderer.set(state);
                if !renderer.is_animated() {
                    show(&mut strip, renderer.frame());
                }
                save_at = Some(Instant::now() + PERSIST_DELAY);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Turns a [`LightState`] into pixel frames.
struct Renderer {
    state: LightState,
    /// Present exactly when `state.mode` is an effect.
    effect: Option<Box<dyn EffectIterator + Send>>,
}

impl Renderer {
    fn new(state: LightState) -> Self {
        Self {
            state,
            effect: effect_for(state.mode),
        }
    }

    fn set(&mut self, state: LightState) {
        // Only restart the animation when switching effects, not when e.g. brightness changes.
        if mem::discriminant(&state.mode) != mem::discriminant(&self.state.mode) {
            self.effect = effect_for(state.mode);
        }
        self.state = state;
    }

    fn is_animated(&self) -> bool {
        self.effect.is_some()
    }

    fn frame(&mut self) -> Vec<Rgb> {
        match &mut self.effect {
            Some(effect) => effect
                .next()
                .unwrap_or_default()
                .into_iter()
                .map(|p| self.state.dim(Rgb::new(p.red, p.green, p.blue)))
                .collect(),
            None => vec![self.state.solid_color().unwrap_or(Rgb::BLACK); LED_COUNT],
        }
    }
}

fn effect_for(mode: Mode) -> Option<Box<dyn EffectIterator + Send>> {
    match mode {
        Mode::Rainbow => Some(Box::new(Rainbow::new(LED_COUNT, None))),
        Mode::Twinkle => Some(Box::new(Twinkle::new(LED_COUNT, None, None, None, None))),
        Mode::Off | Mode::Rgb(_) => None,
    }
}

fn show(strip: &mut Ws2812Esp32RmtDriver<'_>, pixels: Vec<Rgb>) {
    // WS2812 expects GRB byte order.
    let bytes = pixels.into_iter().flat_map(|c| [c.g, c.r, c.b]);
    if let Err(e) = strip.write_blocking(bytes) {
        log::error!("LED write failed: {e}");
    }
}
