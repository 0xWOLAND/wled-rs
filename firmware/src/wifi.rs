//! Brings up Wi-Fi: either the ESP32's own WPA2 access point, or a station on an existing
//! WPA2-Enterprise network (PEAP / TTLS) such as "Caltech Secure". Chosen at build time by `config::STATION`.

use std::{sync::mpsc, thread, time::Duration};

use anyhow::anyhow;
use esp_idf_svc::{
    eventloop::{EspSubscription, EspSystemEventLoop, System},
    hal::modem::Modem,
    netif::IpEvent,
    nvs::EspDefaultNvsPartition,
    sys::{self, esp},
    wifi::{
        AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration,
        EspWifi, WifiEvent,
    },
};

use crate::config::{Station, AP_PASSWORD, AP_SSID, STATION};

/// The running Wi-Fi driver and its event handlers. Must be kept alive.
#[allow(dead_code)] // held only for its Drop
pub enum Wifi {
    AccessPoint(BlockingWifi<EspWifi<'static>>),
    Station(EspWifi<'static>, [EspSubscription<'static, System>; 2]),
}

pub fn start(
    modem: Modem<'static>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<Wifi> {
    let driver = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;
    match &STATION {
        Some(station) => start_station(driver, sysloop, station),
        None => start_access_point(driver, sysloop),
    }
}

fn start_access_point(
    driver: EspWifi<'static>,
    sysloop: EspSystemEventLoop,
) -> anyhow::Result<Wifi> {
    let mut wifi = BlockingWifi::wrap(driver, sysloop)?;

    wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
        ssid: AP_SSID.try_into().map_err(|_| anyhow!("SSID too long"))?,
        password: AP_PASSWORD
            .try_into()
            .map_err(|_| anyhow!("password too long"))?,
        auth_method: AuthMethod::WPA2Personal,
        channel: 1,
        ..Default::default()
    }))?;
    wifi.start()?;
    wifi.wait_netif_up()?;

    let ip = wifi.wifi().ap_netif().get_ip_info()?.ip;
    log::info!("access point \"{AP_SSID}\" up, UI at http://{ip}/");
    Ok(Wifi::AccessPoint(wifi))
}

/// Retry delay after a failed join; doubles on each failure so a wrong password does not hammer
/// (and possibly lock) the account on the RADIUS server.
const RETRY_MIN: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(300);

/// Link changes reported by the event handlers to the reconnect thread.
enum Link {
    Up,
    Down,
}

/// Starts joining in the background and returns at once; a thread reconnects after every drop, so
/// the device keeps serving (and keeps trying) while the network is unreachable.
fn start_station(
    mut driver: EspWifi<'static>,
    sysloop: EspSystemEventLoop,
    station: &Station,
) -> anyhow::Result<Wifi> {
    driver.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: station
            .ssid
            .try_into()
            .map_err(|_| anyhow!("SSID too long"))?,
        auth_method: AuthMethod::WPA2Enterprise,
        ..Default::default()
    }))?;
    configure_eap(station)?;

    let ssid = station.ssid;
    let (link, link_change) = mpsc::channel();
    let down = link.clone();
    let wifi_events = sysloop.subscribe::<WifiEvent, _>(move |event| match event {
        WifiEvent::StaStarted => connect(ssid),
        WifiEvent::StaDisconnected(info) => {
            log::warn!(
                "disconnected from \"{ssid}\": reason {} ({})",
                info.reason(),
                disconnect_reason(info.reason())
            );
            let _ = down.send(Link::Down);
        }
        _ => {}
    })?;
    let ip_events = sysloop.subscribe::<IpEvent, _>(move |event| {
        if let IpEvent::DhcpIpAssigned(lease) = event {
            log::info!(
                "connected, UI at http://{}/ (gateway {})",
                lease.ip(),
                lease.gateway()
            );
            let _ = link.send(Link::Up);
        }
    })?;
    thread::Builder::new()
        .name("wifi-reconnect".into())
        .stack_size(4096)
        .spawn(move || reconnect(ssid, link_change))?;

    let mac = driver.sta_netif().get_mac()?;
    log::info!(
        "station mac {} (register it if the network asks)",
        mac.iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":")
    );
    driver.start()?;
    Ok(Wifi::Station(driver, [wifi_events, ip_events]))
}

/// Hands the credentials to the ESP-IDF supplicant. Must run after the driver is initialised, before start.
fn configure_eap(station: &Station) -> anyhow::Result<()> {
    let Station {
        identity,
        username,
        password,
        ca_cert,
        ..
    } = station;
    unsafe {
        esp!(sys::esp_eap_client_set_identity(
            identity.as_ptr(),
            identity.len() as i32
        ))?;
        esp!(sys::esp_eap_client_set_username(
            username.as_ptr(),
            username.len() as i32
        ))?;
        esp!(sys::esp_eap_client_set_password(
            password.as_ptr(),
            password.len() as i32
        ))?;
        if ca_cert.is_empty() {
            esp!(sys::esp_eap_client_use_default_cert_bundle(true))?;
        } else {
            esp!(sys::esp_eap_client_set_ca_cert(
                ca_cert.as_ptr(),
                ca_cert.len() as i32
            ))?;
        }
        // There is no RTC or NTP before the link is up, so the clock reads 1970 and every
        // certificate would look not-yet-valid. The chain and signature are still checked.
        esp!(sys::esp_eap_client_set_disable_time_check(true))?;
        esp!(sys::esp_wifi_sta_enterprise_enable())?;
    }
    let ca = if ca_cert.is_empty() {
        "built-in CA bundle"
    } else {
        "WIFI_EAP_CA_CERT"
    };
    log::info!(
        "joining \"{}\" as {identity}, server checked against {ca}",
        station.ssid
    );
    Ok(())
}

/// Waits out the backoff after each drop, then retries. Ends when the event handlers are dropped.
fn reconnect(ssid: &'static str, link_change: mpsc::Receiver<Link>) {
    let mut delay = RETRY_MIN;
    loop {
        match link_change.recv() {
            Ok(Link::Up) => delay = RETRY_MIN,
            Ok(Link::Down) => {
                log::info!("retrying \"{ssid}\" in {}s", delay.as_secs());
                thread::sleep(delay);
                while link_change.try_recv().is_ok() {} // drops that arrived meanwhile share this retry
                connect(ssid);
                delay = (delay * 2).min(RETRY_MAX);
            }
            Err(_) => return,
        }
    }
}

/// Runs outside the driver's owner (event loop or reconnect thread), hence the raw call.
fn connect(ssid: &str) {
    if let Err(err) = esp!(unsafe { sys::esp_wifi_connect() }) {
        log::error!("connecting to \"{ssid}\": {err}");
    }
}

/// The reasons that come up when joining an enterprise network (`wifi_err_reason_t`).
fn disconnect_reason(reason: u16) -> &'static str {
    match reason as u32 {
        sys::wifi_err_reason_t_WIFI_REASON_AUTH_EXPIRE => "auth expired",
        sys::wifi_err_reason_t_WIFI_REASON_4WAY_HANDSHAKE_TIMEOUT => "4-way handshake timeout",
        sys::wifi_err_reason_t_WIFI_REASON_802_1X_AUTH_FAILED => {
            "802.1X auth failed: check username/password"
        }
        sys::wifi_err_reason_t_WIFI_REASON_BEACON_TIMEOUT => "beacon timeout: weak signal",
        sys::wifi_err_reason_t_WIFI_REASON_NO_AP_FOUND => "SSID not found",
        sys::wifi_err_reason_t_WIFI_REASON_AUTH_FAIL => "auth failed",
        sys::wifi_err_reason_t_WIFI_REASON_ASSOC_FAIL => "association failed",
        sys::wifi_err_reason_t_WIFI_REASON_HANDSHAKE_TIMEOUT => "handshake timeout",
        sys::wifi_err_reason_t_WIFI_REASON_CONNECTION_FAIL => "connection failed",
        _ => "see wifi_err_reason_t",
    }
}
