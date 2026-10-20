//! Build-time device configuration. Pins are chosen in `main.rs` (they are typed peripherals).

use protocol::Mode;

pub const LED_COUNT: usize = 30;

/// Open network: no password.
pub const AP_SSID: &str = "wled-rs";

/// Modes selected by the switch: position A (GPIO32 to GND), position B (GPIO33 to GND).
pub const SWITCH_MODES: [Mode; 2] = [Mode::Rainbow, Mode::Twinkle];

/// Quiet period after the last change before the light state is written to flash.
pub const PERSIST_DELAY: std::time::Duration = std::time::Duration::from_secs(2);
