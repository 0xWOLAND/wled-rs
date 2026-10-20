# wifi-test

Standalone ESP32 firmware that checks the Wi-Fi radio works.

```sh
source ~/export-esp.sh
cargo run --release        # builds, flashes /dev/cu.usbserial-0001, opens the serial monitor
```

The board hosts `wled-rs-test` (password `wifitest123`) at `192.168.71.1` and pings whichever device
joins it. The log also prints the reset reason at boot, so a brownout when the radio powers up shows there.
