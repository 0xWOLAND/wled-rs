//! Typed client for the device's HTTP API (same origin as the page).

use gloo_net::http::{Request, Response};
use protocol::{DeviceInfo, LightState, API_INFO, API_OTA, API_STATE};
use serde::de::DeserializeOwned;

pub type Result<T> = std::result::Result<T, String>;

pub async fn state() -> Result<LightState> {
    get_json(API_STATE).await
}

pub async fn info() -> Result<DeviceInfo> {
    get_json(API_INFO).await
}

pub async fn set_state(state: &LightState) -> Result<()> {
    let request = Request::put(API_STATE).json(state).map_err(|e| e.to_string())?;
    check(request.send().await).await.map(drop)
}

/// Streams the file straight from disk as the request body; it is never copied into wasm memory.
pub async fn upload_firmware(file: web_sys::File) -> Result<()> {
    let request = Request::post(API_OTA)
        .header("Content-Type", "application/octet-stream")
        .body(file)
        .map_err(|e| e.to_string())?;
    check(request.send().await).await.map(drop)
}

async fn get_json<T: DeserializeOwned>(url: &str) -> Result<T> {
    let response = check(Request::get(url).send().await).await?;
    response.json().await.map_err(|e| e.to_string())
}

async fn check(response: std::result::Result<Response, gloo_net::Error>) -> Result<Response> {
    let response = response.map_err(|e| e.to_string())?;
    if response.ok() {
        Ok(response)
    } else {
        let body = response.text().await.unwrap_or_default();
        Err(format!("HTTP {} {}", response.status(), body.trim()))
    }
}
