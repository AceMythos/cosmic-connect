[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Version](https://img.shields.io/badge/version-0.3.0-blue)
![Rust](https://img.shields.io/badge/rust-1.70+-orange)

# cosmic-connect

KDE Connect in your COSMIC panel. One click to your phone.

Star the repo if you use it.

## Screenshots

<table>
  <tr>
    <td><b>Applet in the panel</b></td>
    <td><b>Popup with device controls</b></td>
  </tr>
  <tr>
    <td><a href="screenshots/panel.jpeg"><img src="screenshots/panel.jpeg" width="250"></a></td>
    <td><a href="screenshots/popup.jpeg"><img src="screenshots/popup.jpeg" width="250"></a></td>
  </tr>
</table>

## Features

- View paired devices with connection status and battery level
- Ping your phone to find it
- Push and pull clipboard text
- Share files, URLs, and text snippets
- Browse device files over SFTP
- Accept pairing requests

All actions route through D-Bus to the KDE Connect daemon. You need KDE Connect on both ends.

## Install

### Quick start

```bash
git clone https://github.com/AceMythos/cosmic-connect && cd cosmic-connect && make deps && make install
```

`make deps` installs system packages and Rust (first run only).
`make install` clones the patched KDE Connect fork, builds it,
installs it system-wide, builds the applet, and restarts the daemon.

### Manual

```bash
sudo apt install cmake extra-cmake-modules libkf5kio-dev \
    libkf5notifications-dev libkf5dbusaddons-dev libkf5config-dev \
    libkf5coreaddons-dev libkf5i18n-dev qtbase5-dev qttools5-dev \
    git build-essential wl-clipboard

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

git clone https://github.com/AceMythos/cosmic-connect
cd cosmic-connect
make install
```

Add COSMIC Connect to your panel: COSMIC Settings -> Desktop -> Panel.

Verify the patched daemon is running:
```bash
pgrep -a kdeconnectd
```

## Requirements

- Rust 1.70+, libcosmic, D-Bus, wl-paste
- Build deps: `cmake extra-cmake-modules libkf5kio-dev libkf5notifications-dev libkf5dbusaddons-dev libkf5config-dev libkf5coreaddons-dev libkf5i18n-dev qtbase5-dev qttools5-dev`

<details>
<summary>Patched KDE Connect fork internals</summary>

The Makefile clones a [patched fork](https://github.com/AceMythos/kdeconnect-fork/tree/v23.08.5-patched) and installs it. The patches add:

### Transfer progress D-Bus signals

KDE Connect's stock D-Bus interface emits `shareReceived` when a transfer finishes. The patch adds four signals:

| Signal | Args |
|---|---|
| `transferStarted` | `transferId, fileName, totalBytes` |
| `transferProgress` | `transferId, bytesTransferred, totalBytes, percent` |
| `transferFinished` | `transferId, url` |
| `transferFailed` | `transferId, errorCode, errorString` |

### Native notification suppression

The fork blocks KDE Connect's desktop notifications for file transfers and pairing requests. cosmic-connect handles those inline.

</details>

## Project structure

`src/backend/mod.rs` -- D-Bus client for `org.kde.kdeconnect`. Async calls for device list, actions, subscriptions.

`src/model.rs` -- data shapes: `Device`, `DeviceType`, `ActionType`.

`src/app.rs` -- applet lifecycle. Renders popup UI, manages form state per device, polls every 2s for device updates.

`src/main.rs` -- runs the COSMIC applet loop.

## Comparison

| Project | Approach | Depends on | Transfer progress | Notification suppression | SMS merge |
|---|---|---|---|---|---|
| **cosmic-connect** | D-Bus wrapper | Patched KDE Connect fork | Yes | Yes | No |
| [cosmic-ext-connected](https://github.com/nwxnw/cosmic-ext-connected) | D-Bus wrapper | Stock KDE Connect daemon | No | No | Yes |
| [cosmic-utils/kdeconnect](https://github.com/cosmic-utils/kdeconnect) | Native Rust reimplementation | None | No | N/A | No |
| [olafkfreund/cosmic-ext-connect](https://github.com/olafkfreund/cosmic-ext-connect-desktop-app) | Native Rust + Android app | Own protocol | No | N/A | No |

cosmic-connect is a 2k-line D-Bus client wrapping the proven KDE Connect C++ daemon.

## Known issues

- Transfer progress and notification suppression need the [patched KDE Connect fork](#install) (built by `make install`)
- File chooser needs libcosmic built with `cosmic::dialog` feature
- Wayland-only (uses wl-paste)
- No SMS/conversation UI

## Contributing

Bug reports and PRs welcome. Test against live D-Bus before submitting.
