//! Persists the light state in NVS so it survives reboots and OTA updates.

use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use protocol::LightState;

const NAMESPACE: &str = "wled";
const KEY: &str = "light";

pub struct Store(EspNvs<NvsDefault>);

impl Store {
    pub fn new(partition: EspDefaultNvsPartition) -> anyhow::Result<Self> {
        Ok(Self(EspNvs::new(partition, NAMESPACE, true)?))
    }

    /// The saved state, or `None` if nothing (valid) has been saved yet.
    pub fn load(&self) -> Option<LightState> {
        let mut buf = [0; 128];
        match self.0.get_blob(KEY, &mut buf) {
            Ok(Some(bytes)) => serde_json::from_slice(bytes)
                .inspect_err(|e| log::warn!("ignoring corrupt saved state: {e}"))
                .ok(),
            Ok(None) => None,
            Err(e) => {
                log::warn!("failed to read saved state: {e}");
                None
            }
        }
    }

    pub fn save(&self, state: &LightState) -> anyhow::Result<()> {
        self.0.set_blob(KEY, &serde_json::to_vec(state)?)?;
        Ok(())
    }
}
