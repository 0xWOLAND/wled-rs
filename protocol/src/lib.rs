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
    /// Animated: hues cycling along the strip.
    Rainbow,
    /// Animated: random coloured sparkles fading out.
    Twinkle,
    Rgb(Rgb),
}

impl Mode {
    /// Whether the mode is an animation rather than one colour on every pixel.
    pub const fn is_effect(self) -> bool {
        matches!(self, Mode::Rainbow | Mode::Twinkle)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightState {
    pub mode: Mode,
    /// 0 = dark, 255 = full output.
    pub brightness: u8,
}

impl LightState {
    /// Scales a full-brightness colour by the gamma-corrected brightness, so the slider feels
    /// linear to the eye. Applies to effect frames as well as solid colours.
    pub fn dim(&self, c: Rgb) -> Rgb {
        let level = (f32::from(self.brightness) / 255.0).powf(2.2);
        let scale = |v: u8| (f32::from(v) * level).round() as u8;
        Rgb::new(scale(c.r), scale(c.g), scale(c.b))
    }

    /// The colour every pixel shows, or `None` for animated modes.
    pub fn solid_color(&self) -> Option<Rgb> {
        match self.mode {
            Mode::Off => Some(Rgb::BLACK),
            Mode::Rgb(c) => Some(self.dim(c)),
            Mode::Rainbow | Mode::Twinkle => None,
        }
    }
}

impl Default for LightState {
    fn default() -> Self {
        Self {
            mode: Mode::Rainbow,
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
        assert_eq!(serde_json::to_string(&Mode::Rainbow).unwrap(), r#"{"kind":"rainbow"}"#);
    }

    #[test]
    fn state_round_trip() {
        for mode in [Mode::Off, Mode::Rainbow, Mode::Twinkle, Mode::Rgb(Rgb::new(255, 0, 128))] {
            let state = LightState { mode, brightness: 42 };
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(serde_json::from_str::<LightState>(&json).unwrap(), state);
        }
    }

    #[test]
    fn dim_scales_with_brightness() {
        let c = Rgb::new(255, 128, 0);
        let full = LightState { mode: Mode::Rgb(c), brightness: 255 };
        assert_eq!(full.solid_color(), Some(c));

        let dark = LightState { brightness: 0, ..full };
        assert_eq!(dark.solid_color(), Some(Rgb::BLACK));

        let half = LightState { brightness: 128, ..full }.dim(c);
        assert!(half.r > 0 && half.r < 128, "gamma keeps mid brightness below linear: {half:?}");
    }

    #[test]
    fn effects_have_no_solid_color() {
        for mode in [Mode::Rainbow, Mode::Twinkle] {
            assert!(mode.is_effect());
            assert_eq!(LightState { mode, brightness: 255 }.solid_color(), None);
        }
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
        assert_eq!(state.solid_color(), Some(Rgb::BLACK));
    }
}
