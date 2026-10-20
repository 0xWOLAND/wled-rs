//! Runs the ESP32 as an open (passwordless) access point.

use anyhow::anyhow;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::modem::Modem,
    nvs::EspDefaultNvsPartition,
    wifi::{AccessPointConfiguration, AuthMethod, BlockingWifi, Configuration, EspWifi},
};

use crate::config::AP_SSID;

/// Starts the access point. The returned driver must be kept alive.
pub fn start_access_point(
    modem: Modem<'static>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;

    wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
        ssid: AP_SSID.try_into().map_err(|_| anyhow!("SSID too long"))?,
        auth_method: AuthMethod::None,
        channel: 1,
        ..Default::default()
    }))?;
    wifi.start()?;
    wifi.wait_netif_up()?;

    let ip = wifi.wifi().ap_netif().get_ip_info()?.ip;
    log::info!("access point \"{AP_SSID}\" up, UI at http://{ip}/");
    Ok(wifi)
}
