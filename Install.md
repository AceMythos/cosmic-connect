## Build

```bash
cargo build --release
```

## Install

```bash
sudo install -m 755 target/release/cosmic-connect /usr/bin/cosmic-connect
sudo cp io.github.acemythos.Connect.desktop /usr/share/applications/
```

Then add the applet via COSMIC Settings → Desktop → Panel.
