//! Wi-Fi bring-up test for the ESP32: hosts "wled-rs-test" and pings whoever joins. See `ap.rs`.
//! The serial log shows the ESP32's side of the test.

mod ap;

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

    ap::run(wifi, sysloop)
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
