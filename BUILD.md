# Build & Update

## Build release

```bash
cargo build --release
```

## Install / Update

```bash
pkexec install -m 755 "$PWD/target/release/cosmic-connect" /usr/bin/cosmic-connect
pkexec install -m 644 "$PWD/io.github.acemythos.Connect.desktop" /usr/share/applications/
```

## Quick rebuild + update

```bash
cargo build --release && \
pkexec install -m 755 "$PWD/target/release/cosmic-connect" /usr/bin/cosmic-connect && \
pkexec install -m 644 "$PWD/io.github.acemythos.Connect.desktop" /usr/share/applications/
```

## Notes

- Log out and back in, or restart the COSMIC panel after updating
- Add to panel: **COSMIC Settings → Desktop → Panel → Add applet**
