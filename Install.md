## Build

```bash
cargo build --release
```

## Install

```bash
pkexec install -m 755 target/release/cosmic-connect /usr/bin/cosmic-connect
pkexec install -m 644 io.github.acemythos.Connect.desktop /usr/share/applications/
```

Then add the applet via COSMIC Settings → Desktop → Panel.
