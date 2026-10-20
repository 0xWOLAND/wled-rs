//! Build orchestration: `cargo xtask <web|build|flash|ota>`.
//!
//! The web UI (wasm32) and the firmware (xtensa, `esp` toolchain) cannot share one cargo invocation,
//! so this drives both and produces the flashable/OTA image.

use std::{
    env, fs,
    io::{IsTerminal as _, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::Duration,
};

type Result<T = ()> = std::result::Result<T, String>;

const CHIP: &str = "esp32";
const TARGET: &str = "xtensa-esp32-espidf";
/// Size of each OTA slot in `firmware/partitions.csv`.
const OTA_SLOT_SIZE: u64 = 0x1F_0000;

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  web                build the egui UI into web/dist
  build              build web UI + firmware, write target/wled-rs.bin (the OTA image)
  flash [--port P]   build, flash over USB and open the serial monitor (when run in a terminal)
  ota <host>         build and upload over Wi-Fi, e.g. `cargo xtask ota 192.168.71.1`

the access point password defaults to `caltech!`; override it with WLED_AP_PASSWORD (8-63 chars)";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["web"] => build_web(),
        ["build"] => build_all().map(drop),
        ["flash"] => flash(None),
        ["flash", "--port", port] => flash(Some(port)),
        ["ota", host] => ota(host),
        _ => Err(USAGE.into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn build_web() -> Result {
    let root = root();
    let dist = root.join("web/dist");

    check_wasm_bindgen_version(&root)?;
    run(cargo().current_dir(&root).args(["build", "-p", "web", "--release", "--target", "wasm32-unknown-unknown"]))?;

    if dist.exists() {
        fs::remove_dir_all(&dist).map_err(|e| format!("cleaning {}: {e}", dist.display()))?;
    }
    run(Command::new("wasm-bindgen")
        .args(["--target", "web", "--no-typescript", "--out-name", "web", "--out-dir"])
        .arg(&dist)
        .arg(root.join("target/wasm32-unknown-unknown/release/web.wasm")))?;

    let wasm = dist.join("web_bg.wasm");
    match Command::new("wasm-opt").arg("--version").output() {
        Ok(_) => run(Command::new("wasm-opt")
            .args(["-Oz", "--enable-bulk-memory", "--enable-nontrapping-float-to-int", "-o"])
            .arg(&wasm)
            .arg(&wasm))?,
        Err(_) => eprintln!("warning: wasm-opt not found (brew install binaryen); the UI will be larger"),
    }

    fs::copy(root.join("web/index.html"), dist.join("index.html")).map_err(|e| e.to_string())?;
    println!("web UI: {} KiB wasm (before gzip)", file_size(&wasm)? / 1024);
    Ok(())
}

/// Builds everything and returns the firmware ELF path. Also writes `target/wled-rs.bin`.
fn build_all() -> Result<PathBuf> {
    if env::var("WLED_AP_PASSWORD").is_ok_and(|p| !(8..=63).contains(&p.len())) {
        return Err("WLED_AP_PASSWORD must be 8-63 characters (WPA2)".into());
    }
    build_web()?;

    let firmware = root().join("firmware");
    let mut build = esp_cargo(&firmware)?;
    run(build.args(["build", "--release"]))?;

    let elf = firmware.join(format!("target/{TARGET}/release/wled-rs"));
    let image = root().join("target/wled-rs.bin");
    run(Command::new("espflash")
        .args(["save-image", "--chip", CHIP, "--partition-table"])
        .arg(firmware.join("partitions.csv"))
        .arg(&elf)
        .arg(&image))?;

    let size = file_size(&image)?;
    println!(
        "firmware image: {} KiB of {} KiB OTA slot ({}%)  -> {}",
        size / 1024,
        OTA_SLOT_SIZE / 1024,
        size * 100 / OTA_SLOT_SIZE,
        image.display()
    );
    if size > OTA_SLOT_SIZE {
        return Err("firmware image does not fit in an OTA slot".into());
    }
    Ok(elf)
}

fn flash(port: Option<&str>) -> Result {
    let elf = build_all()?;
    let firmware = root().join("firmware");
    // The ESP-IDF bootloader built alongside the app (with rollback support), not espflash's bundled one.
    let bootloader = elf.with_file_name("bootloader.bin");

    let mut cmd = Command::new("espflash");
    cmd.args(["flash", "--chip", CHIP, "--partition-table"])
        .arg(firmware.join("partitions.csv"))
        .arg("--bootloader")
        .arg(&bootloader)
        // Reset OTA boot selection so the USB-flashed image (slot ota_0) is the one that boots.
        .args(["--erase-parts", "otadata"]);
    if let Some(port) = port {
        cmd.args(["--port", port]);
    }
    if std::io::stdout().is_terminal() {
        cmd.arg("--monitor");
    }
    run(cmd.arg(&elf))
}

fn ota(host: &str) -> Result {
    build_all()?;
    let image = fs::read(root().join("target/wled-rs.bin")).map_err(|e| e.to_string())?;
    let addr = if host.contains(':') { host.to_string() } else { format!("{host}:80") };
    println!("uploading {} KiB to http://{addr}{} ...", image.len() / 1024, protocol::API_OTA);

    let mut stream = TcpStream::connect(&addr).map_err(|e| format!("connecting to {addr}: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(60))).ok();
    let head = format!(
        "POST {} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        protocol::API_OTA,
        image.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(&image))
        .map_err(|e| format!("upload failed: {e}"))?;

    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|e| format!("reading response: {e}"))?;
    let status = response.lines().next().unwrap_or_default();
    if status.split_whitespace().nth(1) == Some("200") {
        println!("update accepted, device is rebooting");
        Ok(())
    } else {
        Err(format!("device rejected the update: {status}"))
    }
}

/// `cargo` for the workspace: the same toolchain that runs `cargo xtask`. The inherited
/// `RUSTUP_TOOLCHAIN` also stops `rust-toolchain.toml` files shipped inside dependencies from taking over.
fn cargo() -> Command {
    Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

/// `cargo` for the firmware: picks up `firmware/rust-toolchain.toml` (the `esp` toolchain) and the
/// environment from espup's `~/export-esp.sh` if it hasn't been sourced.
fn esp_cargo(dir: &Path) -> Result<Command> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(dir);
    for var in ["RUSTUP_TOOLCHAIN", "CARGO", "RUSTC", "RUSTDOC", "CARGO_TARGET_DIR", "CARGO_MANIFEST_DIR"] {
        cmd.env_remove(var);
    }
    if env::var_os("LIBCLANG_PATH").is_none() {
        let export = home().join("export-esp.sh");
        let script = fs::read_to_string(&export)
            .map_err(|_| format!("{} not found: install the toolchain with `espup install`", export.display()))?;
        for (key, value) in parse_exports(&script) {
            cmd.env(key, value);
        }
    }
    Ok(cmd)
}

/// Parses `export KEY="VALUE"` lines, expanding `$PATH`.
fn parse_exports(script: &str) -> Vec<(String, String)> {
    let path = env::var("PATH").unwrap_or_default();
    script
        .lines()
        .filter_map(|line| line.trim().strip_prefix("export "))
        .filter_map(|assignment| assignment.split_once('='))
        .map(|(key, value)| {
            let value = value.trim_matches('"').replace("$PATH", &path);
            (key.to_string(), value)
        })
        .collect()
}

/// wasm-bindgen's CLI must match the `wasm-bindgen` crate version exactly.
fn check_wasm_bindgen_version(root: &Path) -> Result {
    let lock = fs::read_to_string(root.join("Cargo.lock")).map_err(|e| e.to_string())?;
    let locked = lock
        .split("[[package]]")
        .find(|pkg| pkg.contains("name = \"wasm-bindgen\"\n"))
        .and_then(|pkg| pkg.lines().find_map(|l| l.strip_prefix("version = ")))
        .map(|v| v.trim_matches('"').to_string())
        .ok_or("wasm-bindgen not in Cargo.lock; run `cargo generate-lockfile`")?;

    let installed = Command::new("wasm-bindgen")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    if installed.split_whitespace().nth(1) == Some(locked.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "wasm-bindgen CLI {} does not match crate {locked}; run `cargo install wasm-bindgen-cli --version {locked} --locked`",
            installed.trim()
        ))
    }
}

fn run(cmd: &mut Command) -> Result {
    let status = cmd.status().map_err(|e| format!("running {:?}: {e}", cmd.get_program()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{:?} failed ({status})", cmd.get_program()))
    }
}

fn file_size(path: &Path) -> Result<u64> {
    fs::metadata(path).map(|m| m.len()).map_err(|e| format!("{}: {e}", path.display()))
}

fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}
