# cosmic-connect

A COSMIC desktop applet that wraps KDE Connect over D-Bus. Lets you control paired phones/tablets/laptops from the panel.

## Why

KDE Connect is usually launched as a separate app. This puts it in the panel as an always-available quick popup—useful if you frequently ping your phone, send files, or check battery status without opening a full window.

## What it does

- Lists paired and available devices
- Shows connection status and battery level  
- Sends ping/ring to locate devices
- Pushes/pulls clipboard text
- Shares files, URLs, and text snippets to device
- Browse device files over SFTP
- Accept/cancel/request pairing

All actions run through D-Bus to the KDE Connect daemon. No direct device connection; you still need KDE Connect running on both ends.

## Screenshots

<p align="center">
  <a href="screenshots/full.jpeg">
    <img src="screenshots/full.jpeg" width="250">
  </a>

  <a href="screenshots/connected.jpeg">
    <img src="screenshots/connected.jpeg" width="250">
  </a>

  <a href="screenshots/compact.jpeg">
    <img src="screenshots/compact.jpeg" width="250">
  </a>
</p>

## Building

```bash
cargo build --release
```

Requires:
- Rust 1.70+
- libcosmic (from pop-os/libcosmic)
- Linux with D-Bus
- wl-paste (for clipboard)

The applet discovers itself via the `.desktop` file and appears in the panel automatically after install.

## Testing the backend

```bash
cargo run --bin test_backend
```

Outputs device list. Useful for checking if KDE Connect is reachable over D-Bus.

## How it's structured

**Backend** (`src/backend/mod.rs`): Speaks D-Bus to `org.kde.kdeconnect`. Async calls for device list, actions, subscriptions.

**Model** (`src/model.rs`): Data shapes—`Device`, `DeviceType`, `ActionType`.

**App** (`src/app.rs`): Applet lifecycle. Renders popup UI, manages form state per-device (drafts), polls every 2s for device updates.

**Entry** (`src/main.rs`): Runs the COSMIC applet loop.

## Known issues

- File chooser requires libcosmic built with `cosmic::dialog` feature
- Wayland-only (uses wl-paste)
- No SMS/conversation UI yet

## Transfer progress (incoming files)

KDE Connect's stock D-Bus interface (`org.kde.kdeconnect.device.share`) only emits `shareReceived` when a file is **done** — no progress. To get live progress, the `SharePlugin` was patched with four extra signals:

| Signal | Args |
|---|---|
| `transferStarted` | `transferId (s), fileName (s), totalBytes (t)` |
| `transferProgress` | `transferId (s), bytesTransferred (t), totalBytes (t), percent (i)` |
| `transferFinished` | `transferId (s), url (s)` |
| `transferFailed` | `transferId (s), errorCode (i), errorString (s)` |

**Patched fork**: https://github.com/AceMythos/kdeconnect-fork (branch `v23.08.5-patched`).

The patch is ~30 lines across two files:

- `plugins/share/shareplugin.h` — adds `QElapsedTimer` throttle member and four `Q_SCRIPTABLE` signals
- `plugins/share/shareplugin.cpp` — in `receivePacket`'s payload branch: creates a UUID transfer ID, emits `transferStarted`, connects `KJob::processedAmount` (throttled to 150ms) to `transferProgress`, and emits `transferFinished`/`transferFailed` in the result lambda. `shareReceived` is preserved unchanged in `finished()` for backward compatibility.

The applet subscribes to all signals on the share interface via `MatchRule` (no member filter). The `ShareSignalState` loop parses each message's `header.member()` and dispatches to the appropriate `Message` variant. Active transfers show a progress bar + percentage in the popup, and the system notification updates every 5%.

To test:

```bash
# kill system daemon, run patched one
killall kdeconnectd
/path/to/kdeconnect-fork/build/bin/kdeconnectd

# or after make install
~/.local/lib/x86_64-linux-gnu/libexec/kdeconnectd
```

## Contributing

Bug reports and PRs welcome. Test against live D-Bus before submitting.
