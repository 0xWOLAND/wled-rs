# wled-rs

ESP32 WS2812 controller in Rust. The board runs its own Wi-Fi access point and serves a
Leptos web UI with Off / Rainbow / Twinkle / RGB modes, brightness, and over-the-air firmware updates.
A toggle switch on the board flips between Rainbow and Twinkle.

```
protocol/   shared API types + colour model (host-tested)
web/        Leptos (CSR) UI, compiled to wasm and embedded gzipped into the firmware
firmware/   ESP-IDF std app: AP, HTTP server, WS2812 via RMT, NVS persistence, OTA with rollback
xtask/      build orchestration (`cargo xtask …`)
```

## Setup (once)

```bash
cargo install espup ldproxy espflash
espup install            # Xtensa toolchain; writes ~/export-esp.sh (xtask loads it automatically)
cargo install wasm-bindgen-cli --version <version of wasm-bindgen in Cargo.lock> --locked
brew install binaryen    # wasm-opt
```

## Build, flash, update

```bash
cargo xtask flash        # USB: build everything, flash, open serial monitor
```

Join the open `wled-rs` Wi-Fi network (no password) and open http://192.168.71.1.
Because the network is open, anyone in range can change the lights and upload firmware.

To update over Wi-Fi, either upload `target/wled-rs.bin` (written by `cargo xtask build`) under
**Firmware** in the UI, or run `cargo xtask ota 192.168.71.1`. The image goes into the inactive OTA
slot. If the new firmware fails to come up, the bootloader rolls back to the previous one.

## Hardware

- WS2812 data on **GPIO16**, 30 LEDs.
- SPDT toggle switch: common leg to **GND**, one throw to **GPIO32** (Rainbow), the other to **GPIO33**
  (Twinkle). The pins use internal pull-ups, so no resistors are needed. Flipping the switch overrides
  whatever the UI set; the UI can override again until the next flip.

Pins are set in `firmware/src/main.rs`; LED count and switch modes in `firmware/src/config.rs`.
Flash layout (`firmware/partitions.csv`): two 1.94 MB OTA slots on 4 MB flash.

## Tests

```bash
cargo test -p protocol
```
