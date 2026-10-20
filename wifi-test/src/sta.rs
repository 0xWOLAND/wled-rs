//! Station mode: the ESP32 joins a WPA2-Enterprise network (PEAP / TTLS) such as "Caltech Secure",
//! then pings the gateway and an internet host to show it has a working route out.
//!
//! The Mac stays on the same network and can `ping` the address printed in the log, unless the
//! network isolates clients from each other.

use std::{net::Ipv4Addr, thread, time::Duration};

use anyhow::anyhow;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    sys::{self, esp},
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi, WifiEvent},
};

use crate::{fmt_mac, ping_once};

/// Build-time credentials for the enterprise network.
pub struct Eap {
    pub ssid: &'static str,
    /// Outer (unencrypted) identity. Usually the username, sometimes "anonymous@domain".
    pub identity: &'static str,
    pub username: &'static str,
    pub password: &'static str,
    /// PEM, NUL-terminated as mbedTLS requires. Empty: verify against the built-in public CA bundle.
    pub ca_cert: &'static [u8],
}

/// Reached over the internet, not the campus network, to show the board has a route out.
const INTERNET_HOST: Ipv4Addr = Ipv4Addr::new(1, 1, 1, 1);

pub fn run(
    mut wifi: BlockingWifi<EspWifi<'static>>,
    sysloop: EspSystemEventLoop,
    eap: &Eap,
) -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: eap.ssid.try_into().map_err(|_| anyhow!("SSID too long"))?,
        auth_method: AuthMethod::WPA2Enterprise,
        ..Default::default()
    }))?;
    configure_eap(eap)?;

    let _wifi_sub = sysloop.subscribe::<WifiEvent, _>(|event| {
        if let WifiEvent::StaDisconnected(info) = event {
            log::warn!(
                "disconnected: reason {} ({})",
                info.reason(),
                disconnect_reason_name(info.reason())
            );
        }
    })?;

    let mac = wifi.wifi().sta_netif().get_mac()?;
    log::info!(
        "starting radio; station mac {} (register this if the network asks)",
        fmt_mac(&mac)
    );
    wifi.start()?;
    connect(&mut wifi, eap.ssid);

    loop {
        if !wifi.is_connected().unwrap_or(false) {
            log::warn!("link lost, reconnecting");
            connect(&mut wifi, eap.ssid);
        }
        let info = wifi.wifi().sta_netif().get_ip_info()?;
        ping_once(info.subnet.gateway);
        ping_once(INTERNET_HOST);
        thread::sleep(Duration::from_secs(10));
    }
}

/// Hands the credentials to the ESP-IDF supplicant. Must run after the driver is initialised, before start.
fn configure_eap(eap: &Eap) -> anyhow::Result<()> {
    unsafe {
        esp!(sys::esp_eap_client_set_identity(
            eap.identity.as_ptr(),
            eap.identity.len() as i32
        ))?;
        esp!(sys::esp_eap_client_set_username(
            eap.username.as_ptr(),
            eap.username.len() as i32
        ))?;
        esp!(sys::esp_eap_client_set_password(
            eap.password.as_ptr(),
            eap.password.len() as i32
        ))?;
        if eap.ca_cert.is_empty() {
            esp!(sys::esp_eap_client_use_default_cert_bundle(true))?;
        } else {
            esp!(sys::esp_eap_client_set_ca_cert(
                eap.ca_cert.as_ptr(),
                eap.ca_cert.len() as i32
            ))?;
        }
        // There is no RTC or NTP before the link is up, so the clock reads 1970 and every
        // certificate would look not-yet-valid. The chain and signature are still checked.
        esp!(sys::esp_eap_client_set_disable_time_check(true))?;
        esp!(sys::esp_wifi_sta_enterprise_enable())?;
    }
    let ca = if eap.ca_cert.is_empty() {
        "built-in CA bundle"
    } else {
        "WIFI_EAP_CA_CERT"
    };
    log::info!(
        "WPA2-Enterprise for \"{}\" as {}, server checked against {ca}",
        eap.ssid,
        eap.identity
    );
    Ok(())
}

/// Retries until associated, authenticated and holding a DHCP lease. The delay doubles from 5 s to
/// 5 min so a wrong password does not hammer (and possibly lock) the account on the RADIUS server.
fn connect(wifi: &mut BlockingWifi<EspWifi<'static>>, ssid: &str) {
    let mut delay = Duration::from_secs(5);
    loop {
        log::info!("joining \"{ssid}\"...");
        match wifi.connect().and_then(|()| wifi.wait_netif_up()) {
            Ok(()) => break,
            Err(err) => log::error!("join failed: {err}; retrying in {}s", delay.as_secs()),
        }
        let _ = wifi.disconnect();
        thread::sleep(delay);
        delay = (delay * 2).min(Duration::from_secs(300));
    }
    if let Ok(info) = wifi.wifi().sta_netif().get_ip_info() {
        log::info!(
            "connected: ESP32 at {}, gateway {}, dns {:?}. From the Mac: ping {}",
            info.ip,
            info.subnet.gateway,
            info.dns,
            info.ip
        );
    }
}

/// The reasons that come up when joining an enterprise network (`wifi_err_reason_t`).
fn disconnect_reason_name(reason: u16) -> &'static str {
    match reason as u32 {
        sys::wifi_err_reason_t_WIFI_REASON_AUTH_EXPIRE => "auth expired",
        sys::wifi_err_reason_t_WIFI_REASON_ASSOC_LEAVE => "left",
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
