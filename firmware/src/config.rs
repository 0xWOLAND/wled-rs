//! Build-time device configuration. The LED data pin is chosen in `main.rs` (pins are typed peripherals).

pub const LED_COUNT: usize = 30;

pub const AP_SSID: &str = "wled-rs";
/// Override at build time with `WLED_AP_PASSWORD`.
pub const AP_PASSWORD: &str = match setting(option_env!("WLED_AP_PASSWORD")) {
    Some(password) => password,
    None => "caltech!",
};
const _: () = assert!(
    AP_PASSWORD.len() >= 8 && AP_PASSWORD.len() <= 63,
    "WLED_AP_PASSWORD must be 8-63 characters (WPA2)"
);

/// Credentials for joining an existing WPA2-Enterprise network instead of hosting the access point.
pub struct Station {
    pub ssid: &'static str,
    /// Outer (unencrypted) identity. Usually the username, sometimes "anonymous@domain".
    pub identity: &'static str,
    pub username: &'static str,
    pub password: &'static str,
    /// PEM, NUL-terminated as mbedTLS requires. Empty: verify against the built-in public CA bundle.
    pub ca_cert: &'static [u8],
}

/// Set `WIFI_EAP_USERNAME` and `WIFI_EAP_PASSWORD` (in `firmware/.env` or the shell) to join a network
/// such as "Caltech Secure"; leave both unset to run the access point. `build.rs` reads `.env`.
pub const STATION: Option<Station> = match (setting(option_env!("WIFI_EAP_USERNAME")), setting(option_env!("WIFI_EAP_PASSWORD"))) {
    (Some(username), Some(password)) => Some(Station {
        ssid: match setting(option_env!("WIFI_SSID")) {
            Some(ssid) => ssid,
            None => "Caltech Secure",
        },
        identity: match setting(option_env!("WIFI_EAP_IDENTITY")) {
            Some(identity) => identity,
            None => username,
        },
        username,
        password,
        ca_cert: include_bytes!(concat!(env!("OUT_DIR"), "/eap_ca.pem")),
    }),
    (None, None) => None,
    _ => panic!("set both WIFI_EAP_USERNAME and WIFI_EAP_PASSWORD to join a network, or neither for the access point"),
};

/// An empty variable (e.g. `export WIFI_EAP_PASSWORD=`) counts as unset.
const fn setting(value: Option<&'static str>) -> Option<&'static str> {
    match value {
        Some(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

/// Quiet period after the last change before the light state is written to flash.
pub const PERSIST_DELAY: std::time::Duration = std::time::Duration::from_secs(2);
