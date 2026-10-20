//! Embeds the gzipped web UI (`../web/dist`, produced by `cargo xtask build`) into the firmware, and
//! passes the Wi-Fi settings from `.env` (shell variables win) to `option_env!` in `config.rs`.

use std::{
    collections::HashMap,
    env, fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use flate2::{write::GzEncoder, Compression};

/// Build-time settings read by `config.rs`.
const SETTINGS: [&str; 6] = [
    "WLED_AP_PASSWORD",
    "WIFI_SSID",
    "WIFI_EAP_IDENTITY",
    "WIFI_EAP_USERNAME",
    "WIFI_EAP_PASSWORD",
    "WIFI_EAP_CA_CERT",
];

fn main() {
    embed_web_assets();
    load_settings();
    embuild::espidf::sysenv::output();
}

fn load_settings() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dotenv = dir.join(".env");
    println!("cargo:rerun-if-changed={}", dotenv.display());
    let file = fs::read_to_string(&dotenv)
        .map(|s| parse_dotenv(&s))
        .unwrap_or_default();

    let mut settings = HashMap::new();
    for var in SETTINGS {
        println!("cargo:rerun-if-env-changed={var}");
        if let Some(value) = env::var(var)
            .ok()
            .or_else(|| file.get(var).cloned())
            .filter(|v| !v.is_empty())
        {
            println!("cargo:rustc-env={var}={value}");
            settings.insert(var, value);
        }
    }

    // Enterprise CA certificate, embedded as `eap_ca.pem`. Empty: the built-in CA bundle is used.
    let mut pem = Vec::new();
    if let Some(path) = settings.get("WIFI_EAP_CA_CERT") {
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

fn embed_web_assets() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/dist");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed={}", dist.display());

    let mut files: Vec<_> = fs::read_dir(&dist)
        .unwrap_or_else(|_| {
            panic!(
                "{} not found: build with `cargo xtask build`",
                dist.display()
            )
        })
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    assert!(
        files.iter().any(|p| p.ends_with("index.html")),
        "web/dist has no index.html: build with `cargo xtask build`"
    );

    let mut table = String::from("&[\n");
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path.file_name().unwrap().to_str().unwrap();
        let gz = out.join(format!("{name}.gz"));

        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&fs::read(&path).unwrap()).unwrap();
        fs::write(&gz, encoder.finish().unwrap()).unwrap();

        table += &format!(
            "    Asset {{ path: \"/{name}\", mime: \"{}\", body: include_bytes!({:?}) }},\n",
            mime(&path),
            gz.display().to_string(),
        );
    }
    table += "]\n";
    fs::write(out.join("assets.rs"), table).unwrap();
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        _ => "application/octet-stream",
    }
}
