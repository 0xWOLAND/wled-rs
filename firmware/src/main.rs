mod config;
mod http;
mod led;
mod store;
mod wifi;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop, hal::peripherals::Peripherals, log::EspLogger,
    nvs::EspDefaultNvsPartition, ota::EspOta,
};
use protocol::DeviceInfo;

use crate::{led::Light, store::Store};

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let store = Store::new(nvs.clone())?;
    let initial = store.load().unwrap_or_default();
    // LED data line on GPIO16. Legacy RMT channel: see led.rs.
    #[allow(deprecated)]
    let light = Light::start(
        peripherals.rmt.channel0,
        peripherals.pins.gpio16,
        store,
        initial,
    )?;

    let _wifi = wifi::start_access_point(peripherals.modem, sysloop, nvs)?;

    let mut ota = EspOta::new()?;
    let info = DeviceInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        led_count: config::LED_COUNT as u16,
        ota_slot: ota.get_running_slot()?.label.to_string(),
    };
    log::info!("wled-rs {} running from {}", info.version, info.ota_slot);
    let _server = http::start(light, info)?;

    // Everything came up: keep this image. Without this, a freshly OTA'd image is rolled back on the next reset.
    ota.mark_running_slot_valid()?;

    loop {
        std::thread::park();
    }
}
