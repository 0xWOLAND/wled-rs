//! Build-time device configuration. The LED data pin is chosen in `main.rs` (pins are typed peripherals).

pub const LED_COUNT: usize = 30;

pub const AP_SSID: &str = "wled-rs";
/// Override at build time with `WLED_AP_PASSWORD`.
pub const AP_PASSWORD: &str = match option_env!("WLED_AP_PASSWORD") {
    Some(password) => password,
    None => "caltech!",
};
const _: () = assert!(
    AP_PASSWORD.len() >= 8 && AP_PASSWORD.len() <= 63,
    "WLED_AP_PASSWORD must be 8-63 characters (WPA2)"
);

/// Quiet period after the last change before the light state is written to flash.
pub const PERSIST_DELAY: std::time::Duration = std::time::Duration::from_secs(2);
