//! Access point mode: the ESP32 hosts "wled-rs-test" and pings every station that joins it.
//!
//! Join "wled-rs-test" (password "wifitest123") from the Mac, then `ping 192.168.71.1`.

use std::{net::Ipv4Addr, sync::mpsc, time::Duration};

use anyhow::anyhow;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    netif::IpEvent,
    wifi::{AccessPointConfiguration, AuthMethod, BlockingWifi, Configuration, EspWifi, WifiEvent},
};

use crate::{fmt_mac, ping_once};

const SSID: &str = "wled-rs-test";
const PASSWORD: &str = "wifitest123";
const CHANNEL: u8 = 1;

pub fn run(
    mut wifi: BlockingWifi<EspWifi<'static>>,
    sysloop: EspSystemEventLoop,
) -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
        ssid: SSID.try_into().map_err(|_| anyhow!("SSID too long"))?,
        password: PASSWORD
            .try_into()
            .map_err(|_| anyhow!("password too long"))?,
        auth_method: AuthMethod::WPA2Personal,
        channel: CHANNEL,
        ..Default::default()
    }))?;
    log::info!("starting radio");
    wifi.start()?;
    wifi.wait_netif_up()?;

    let ip = wifi.wifi().ap_netif().get_ip_info()?.ip;
    let mac = wifi.wifi().ap_netif().get_mac()?;
    log::info!(
        "AP \"{SSID}\" up on channel {CHANNEL}, ESP32 at {ip}, mac {}",
        fmt_mac(&mac)
    );
    log::info!("join it with password \"{PASSWORD}\", then run: ping {ip}");

    // Some(ip) when a station gets a DHCP lease, None when a station leaves.
    let (stations, station_change) = mpsc::channel::<Option<Ipv4Addr>>();
    let left = stations.clone();
    let _wifi_sub = sysloop.subscribe::<WifiEvent, _>(move |event| match event {
        WifiEvent::ApStaConnected(sta) => log::info!("station joined: {}", fmt_mac(&sta.mac())),
        WifiEvent::ApStaDisconnected(sta) => {
            log::warn!("station left: {}", fmt_mac(&sta.mac()));
            let _ = left.send(None);
        }
        _ => {}
    })?;
    let _ip_sub = sysloop.subscribe::<IpEvent, _>(move |event| {
        if let IpEvent::ApStaIpAssigned(lease) = event {
            log::info!("DHCP lease: {}", lease.ip());
            let _ = stations.send(Some(lease.ip()));
        }
    })?;

    // Ping the most recent station every few seconds until it leaves the network.
    let mut target: Option<Ipv4Addr> = None;
    loop {
        if let Ok(change) = station_change.recv_timeout(Duration::from_secs(5)) {
            target = change;
        }
        match target {
            Some(ip) => ping_once(ip),
            None => log::info!("waiting for a station to join \"{SSID}\"..."),
        }
    }
}
