//! Loads the Wi-Fi settings from `.env` (shell variables win), passes them to `option_env!` in the
//! firmware, and copies the optional CA certificate (`WIFI_EAP_CA_CERT`, a PEM path) into OUT_DIR.
//! Without a certificate the file is empty and the built-in CA bundle is used.

use std::{collections::HashMap, env, fs, path::PathBuf};

const VARS: [&str; 5] = [
    "WIFI_SSID",
    "WIFI_EAP_IDENTITY",
    "WIFI_EAP_USERNAME",
    "WIFI_EAP_PASSWORD",
    "WIFI_EAP_CA_CERT",
];

fn main() {
    let dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let dotenv = dir.join(".env");
    println!("cargo:rerun-if-changed={}", dotenv.display());
    let file = fs::read_to_string(&dotenv)
        .map(|s| parse_dotenv(&s))
        .unwrap_or_default();

    let mut vars = HashMap::new();
    for var in VARS {
        println!("cargo:rerun-if-env-changed={var}");
        if let Some(value) = env::var(var)
            .ok()
            .or_else(|| file.get(var).cloned())
            .filter(|v| !v.is_empty())
        {
            println!("cargo:rustc-env={var}={value}");
            vars.insert(var, value);
        }
    }

    let mut pem = Vec::new();
    if let Some(path) = vars.get("WIFI_EAP_CA_CERT") {
        let path = dir.join(path); // relative paths are relative to this crate
        println!("cargo:rerun-if-changed={}", path.display());
        pem =
            fs::read(&path).unwrap_or_else(|e| panic!("WIFI_EAP_CA_CERT {}: {e}", path.display()));
        assert!(
            pem.starts_with(b"-----BEGIN"),
            "WIFI_EAP_CA_CERT must be a PEM file"
        );
        pem.push(0); // mbedTLS parses PEM only when the length includes a NUL terminator
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("eap_ca.pem"), pem).unwrap();

    embuild::espidf::sysenv::output();
}

/// `KEY=value` lines; blank lines and `#` comments are skipped, surrounding quotes are stripped.
fn parse_dotenv(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| {
            let value = value.trim();
            let unquoted = ['"', '\'']
                .iter()
                .find_map(|q| value.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)))
                .unwrap_or(value);
            (
                key.trim().trim_start_matches("export ").to_string(),
                unquoted.to_string(),
            )
        })
        .collect()
}
