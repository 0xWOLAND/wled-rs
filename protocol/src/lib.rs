//! HTTP API shared by the firmware and the web UI. All bodies are JSON.

use serde::{Deserialize, Serialize};

/// `GET` returns the current [`LightState`]; `PUT` replaces it.
pub const API_STATE: &str = "/api/state";
/// `GET` returns [`DeviceInfo`].
pub const API_INFO: &str = "/api/info";
/// `POST` a raw ESP-IDF app image (`application/octet-stream`); the device reboots into it on success.
pub const API_OTA: &str = "/api/ota";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Self = Self::new(0, 0, 0);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// CSS hex form, e.g. `#ff8000`.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Parses `#rrggbb` (the value format of `<input type="color">`).
    pub fn from_hex(s: &str) -> Option<Self> {
        let hex = s.strip_prefix('#')?;
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self::new(channel(0)?, channel(2)?, channel(4)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mode {
    Off,
    Warm,
    Cool,
    Rgb(Rgb),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightState {
    pub mode: Mode,
    /// 0 = dark, 255 = full output.
    pub brightness: u8,
}

impl Mode {
    /// ~2700 K incandescent white.
    pub const WARM: Rgb = Rgb::new(255, 147, 41);
    /// ~8000 K daylight white.
    pub const COOL: Rgb = Rgb::new(201, 226, 255);

    /// Full-brightness colour for this mode.
    pub const fn base_color(self) -> Rgb {
        match self {
            Mode::Off => Rgb::BLACK,
            Mode::Warm => Self::WARM,
            Mode::Cool => Self::COOL,
            Mode::Rgb(rgb) => rgb,
        }
    }
}

impl LightState {
    /// Colour to drive every pixel with: the mode's colour scaled by gamma-corrected brightness,
    /// so the slider feels linear to the eye.
    pub fn color(&self) -> Rgb {
        let base = self.mode.base_color();
        let level = (f32::from(self.brightness) / 255.0).powf(2.2);
        let scale = |c: u8| (f32::from(c) * level).round() as u8;
        Rgb::new(scale(base.r), scale(base.g), scale(base.b))
    }
}

impl Default for LightState {
    fn default() -> Self {
        Self {
            mode: Mode::Warm,
            brightness: 128,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Firmware crate version.
    pub version: String,
    pub led_count: u16,
    /// Label of the OTA partition currently running, e.g. `ota_0`.
    pub ota_slot: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_wire_format() {
        let json = serde_json::to_string(&Mode::Rgb(Rgb::new(1, 2, 3))).unwrap();
        assert_eq!(json, r#"{"kind":"rgb","r":1,"g":2,"b":3}"#);
        assert_eq!(serde_json::to_string(&Mode::Off).unwrap(), r#"{"kind":"off"}"#);
    }

    #[test]
    fn state_round_trip() {
        for mode in [Mode::Off, Mode::Warm, Mode::Cool, Mode::Rgb(Rgb::new(255, 0, 128))] {
            let state = LightState { mode, brightness: 42 };
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(serde_json::from_str::<LightState>(&json).unwrap(), state);
        }
    }

    #[test]
    fn color_scales_with_brightness() {
        let full = LightState { mode: Mode::Warm, brightness: 255 };
        assert_eq!(full.color(), Mode::WARM);

        let dark = LightState { brightness: 0, ..full };
        assert_eq!(dark.color(), Rgb::BLACK);

        let half = LightState { brightness: 128, ..full }.color();
        assert!(half.r > 0 && half.r < 128, "gamma keeps mid brightness below linear: {half:?}");
    }

    #[test]
    fn hex_round_trip() {
        let c = Rgb::new(255, 128, 0);
        assert_eq!(c.to_hex(), "#ff8000");
        assert_eq!(Rgb::from_hex("#ff8000"), Some(c));
        assert_eq!(Rgb::from_hex("#FF8000"), Some(c));
        for bad in ["ff8000", "#ff800", "#ff80000", "#gg8000", "#ff80é"] {
            assert_eq!(Rgb::from_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn off_is_black_at_any_brightness() {
        let state = LightState { mode: Mode::Off, brightness: 255 };
        assert_eq!(state.color(), Rgb::BLACK);
    }
}
