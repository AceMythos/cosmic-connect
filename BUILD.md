# Build & Update

These steps follow the COSMIC applet flow described in Bryan Hyland's "COSMIC Applet Tutorial" and adapt it to this project by using `install` instead of `cp`.

## Run for local testing

```bash
cargo run
```

## Build release

```bash
cargo build --release
```

## Install / Update

```bash
pkexec install -m 755 "$PWD/target/release/cosmic-connect" /usr/bin/cosmic-connect
pkexec install -m 644 "$PWD/io.github.acemythos.Connect.desktop" /usr/share/applications/
```

The desktop entry must remain installed in `/usr/share/applications/` with the COSMIC applet metadata:

- `X-CosmicApplet=true`
- `X-CosmicHoverPopup=End`

## Quick rebuild + update

```bash
cargo build --release && \
pkexec install -m 755 "$PWD/target/release/cosmic-connect" /usr/bin/cosmic-connect && \
pkexec install -m 644 "$PWD/io.github.acemythos.Connect.desktop" /usr/share/applications/
```

## Notes

- `io.github.acemythos.Connect.desktop` already contains the required COSMIC applet keys
- Log out and back in, or restart the COSMIC panel after updating
- Add to panel: **COSMIC Settings → Desktop → Panel → Add applet**
