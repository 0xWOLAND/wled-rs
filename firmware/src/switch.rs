//! SPDT toggle switch: common leg to GND, one throw on each input pin (internal pull-ups).
//! Flipping it selects the matching mode; the web UI can still override until the next flip.

use std::{thread, time::Duration};

use esp_idf_svc::hal::gpio::{InputPin, PinDriver, Pull};
use protocol::LightState;

use crate::{config::SWITCH_MODES, led::Light};

const POLL: Duration = Duration::from_millis(10);
/// Consecutive identical readings required before a flip counts (50 ms).
const DEBOUNCE_POLLS: u8 = 5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Position {
    A,
    B,
}

pub fn start(pin_a: impl InputPin + 'static, pin_b: impl InputPin + 'static, light: Light) -> anyhow::Result<()> {
    let a = PinDriver::input(pin_a, Pull::Up)?;
    let b = PinDriver::input(pin_b, Pull::Up)?;
    let read = move || match (a.is_low(), b.is_low()) {
        (true, false) => Some(Position::A),
        (false, true) => Some(Position::B),
        // Between contacts mid-flip, or not wired.
        _ => None,
    };

    thread::Builder::new()
        .name("switch".into())
        .stack_size(4096)
        .spawn(move || {
            // The position at boot is not an action: keep the restored state until the switch moves.
            let mut settled = read();
            let mut candidate = settled;
            let mut stable = 0u8;
            loop {
                thread::sleep(POLL);
                let Some(position) = read() else { continue };
                if Some(position) == candidate {
                    stable = stable.saturating_add(1);
                } else {
                    candidate = Some(position);
                    stable = 0;
                }
                if stable >= DEBOUNCE_POLLS && candidate != settled {
                    settled = candidate;
                    let mode = match position {
                        Position::A => SWITCH_MODES[0],
                        Position::B => SWITCH_MODES[1],
                    };
                    log::info!("switch -> {position:?}: {mode:?}");
                    light.set(LightState { mode, ..light.get() });
                }
            }
        })?;
    Ok(())
}
