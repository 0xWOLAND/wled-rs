//! Wi-Fi bring-up test for the ESP32, in one of two modes chosen at build time:
//!
//! - Station (`WIFI_EAP_USERNAME` and `WIFI_EAP_PASSWORD` set): joins a WPA2-Enterprise network such
//!   as "Caltech Secure" and pings the gateway and the internet. See `sta.rs`.
//! - Access point (otherwise): hosts "wled-rs-test" and pings whoever joins. See `ap.rs`.
//!
//! The serial log shows the ESP32's side of either test.

mod ap;
mod sta;

use std::net::Ipv4Addr;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripherals::Peripherals,
    log::EspLogger,
    nvs::EspDefaultNvsPartition,
    ping::{self, EspPing},
    sys,
    wifi::{BlockingWifi, EspWifi},
};

const EAP_USERNAME: Option<&str> = setting(option_env!("WIFI_EAP_USERNAME"));
const EAP_PASSWORD: Option<&str> = setting(option_env!("WIFI_EAP_PASSWORD"));
const _: () = assert!(
    EAP_USERNAME.is_some() == EAP_PASSWORD.is_some(),
    "set both WIFI_EAP_USERNAME and WIFI_EAP_PASSWORD for station mode, or neither for access point mode"
);

/// An empty variable (e.g. `export WIFI_EAP_PASSWORD=`) counts as unset.
const fn setting(value: Option<&'static str>) -> Option<&'static str> {
    match value {
        Some(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn main() -> anyhow::Result<()> {
    sys::link_patches();
    EspLogger::initialize_default();

    // A brownout here means the board resets when the radio powers up: a supply problem, not software.
    let reason = unsafe { sys::esp_reset_reason() };
    log::info!("reset reason: {}", reset_reason_name(reason));

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs))?,
        sysloop.clone(),
    )?;

    match (EAP_USERNAME, EAP_PASSWORD) {
        (Some(username), Some(password)) => {
            let eap = sta::Eap {
                ssid: setting(option_env!("WIFI_SSID")).unwrap_or("Caltech Secure"),
                identity: setting(option_env!("WIFI_EAP_IDENTITY")).unwrap_or(username),
                username,
                password,
                ca_cert: include_bytes!(concat!(env!("OUT_DIR"), "/eap_ca.pem")),
            };
            sta::run(wifi, sysloop, &eap)
        }
        _ => ap::run(wifi, sysloop),
    }
}

/// Sends four pings and logs the summary.
fn ping_once(ip: Ipv4Addr) {
    let conf = ping::Configuration {
        count: 4,
        ..Default::default()
    };
    match EspPing::default().ping(ip, &conf) {
        Ok(summary) => log::info!(
            "ping {ip}: {}/{} replies in {:?}",
            summary.received,
            summary.transmitted,
            summary.time
        ),
        Err(err) => log::error!("ping {ip} failed: {err}"),
    }
}

fn fmt_mac(mac: &[u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn reset_reason_name(reason: sys::esp_reset_reason_t) -> &'static str {
    match reason {
        sys::esp_reset_reason_t_ESP_RST_POWERON => "power-on",
        sys::esp_reset_reason_t_ESP_RST_EXT => "external pin",
        sys::esp_reset_reason_t_ESP_RST_SW => "software restart",
        sys::esp_reset_reason_t_ESP_RST_PANIC => "panic",
        sys::esp_reset_reason_t_ESP_RST_INT_WDT => "interrupt watchdog",
        sys::esp_reset_reason_t_ESP_RST_TASK_WDT => "task watchdog",
        sys::esp_reset_reason_t_ESP_RST_WDT => "other watchdog",
        sys::esp_reset_reason_t_ESP_RST_DEEPSLEEP => "deep sleep wake",
        sys::esp_reset_reason_t_ESP_RST_BROWNOUT => "BROWNOUT (supply voltage dipped)",
        _ => "unknown",
    }
}
