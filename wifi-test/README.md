# wifi-test

Standalone ESP32 firmware that checks the Wi-Fi radio works. The mode is chosen at build time.

## Station mode: join Caltech Secure

Put your credentials in `.env`, which git ignores. `build.rs` reads it, and shell variables of the
same name take precedence.

```sh
cp .env.example .env       # then fill in WIFI_EAP_USERNAME and WIFI_EAP_PASSWORD
source ~/export-esp.sh
cargo run --release        # builds, flashes /dev/cu.usbserial-0001, opens the serial monitor
```

The log prints the board's MAC, then `connected: ESP32 at <ip>`, then pings to the gateway and
to 1.1.1.1 every 10 seconds. With the Mac on the same network, `ping <ip>` tests the other direction.
That only works if the network allows client-to-client traffic.

Optional variables:

| Variable | Default | Use |
| --- | --- | --- |
| `WIFI_SSID` | `Caltech Secure` | Another enterprise network |
| `WIFI_EAP_IDENTITY` | the username | Outer identity, if the network wants e.g. `anonymous@caltech.edu` |
| `WIFI_EAP_CA_CERT` | built-in public CA bundle | Path to a PEM file, if the RADIUS server uses a private CA |

If a join fails, the log names the disconnect reason. `802.1X auth failed` means the credentials
or CA were rejected. `SSID not found` means the network isn't in range.

**The password ends up in plain text** in `target/` and in the board's flash. Anyone with the
board or the build output can read it. Use a device or app password if Caltech IT issues one.

## Access point mode

Leave both credentials empty (or delete `.env`). The board hosts `wled-rs-test` (password `wifitest123`) at
`192.168.71.1` and pings whichever device joins it.
