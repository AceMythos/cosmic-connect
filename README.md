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

## Requirements

- Rust 1.70+
- libcosmic (from pop-os/libcosmic)
- Linux with D-Bus
- wl-paste (for reading clipboard contents on Wayland)
- Stock KDE Connect packages (`kdeconnect` or `kdeconnect-kde`)
- Optional: [Patched KDE Connect fork](#patched-kde-connect-fork) for transfer progress display and native notification suppression

## Building

```bash
cargo build --release
```

The applet discovers itself via the `.desktop` file and appears in the panel automatically after install.

## Patched KDE Connect fork

The applet works with stock KDE Connect for basic pairing, clipboard, file sharing, and notifications. Two optional patches add **live transfer progress** and **suppress duplicate native notifications**.

Install the stock `kdeconnect` package first (for libraries and dependencies), then build from source:

```bash
# System dependencies
sudo apt build-dep kdeconnect
sudo apt install cmake extra-cmake-modules libkf5kio-dev libkf5notifications-dev \
                 libkf5dbusaddons-dev libkf5config-dev libkf5coreaddons-dev \
                 libkf5i18n-dev qtbase5-dev qttools5-dev

# Build and install patched KDE Connect
git clone -b v23.08.5-patched https://github.com/AceMythos/kdeconnect-fork.git
cd kdeconnect-fork
mkdir build && cd build
cmake .. -DCMAKE_INSTALL_PREFIX=/usr
make -j$(nproc)
sudo make install
```

This replaces the system `kdeconnectd` and plugins with three patches on top of v23.08.5.

### 1. Transfer progress D-Bus signals

KDE Connect's stock D-Bus interface (`org.kde.kdeconnect.device.share`) only emits `shareReceived` when a file is **done** — no progress. The `SharePlugin` was patched with four extra signals:

| Signal | Args |
|---|---|
| `transferStarted` | `transferId (s), fileName (s), totalBytes (t)` |
| `transferProgress` | `transferId (s), bytesTransferred (t), totalBytes (t), percent (i)` |
| `transferFinished` | `transferId (s), url (s)` |
| `transferFailed` | `transferId (s), errorCode (i), errorString (s)` |

The patch is ~30 lines across two files:

- `plugins/share/shareplugin.h` — adds `QElapsedTimer` throttle member and four `Q_SCRIPTABLE` signals
- `plugins/share/shareplugin.cpp` — in `receivePacket`'s payload branch: generates a UUID transfer ID, emits `transferStarted`, connects `KJob::processedAmount` (throttled to 150ms) to `transferProgress`, and emits `transferFinished`/`transferFailed` in the `KJob::result` handler. `shareReceived` is preserved unchanged in `finished()` for backward compatibility.

The applet subscribes to all signals on the share interface via `MatchRule` (no member filter). Active transfers show a progress bar + percentage in the popup, and the system notification updates every 5%.

### 2. Native notification suppression

Stock KDE Connect shows its own desktop notifications for file transfers and pairing requests. Since cosmic-connect already handles these, the fork suppresses them to avoid duplicates.

**KJobTracker transfer notifications** (`plugins/share/shareplugin.cpp`):

Stock KDE Connect registers incoming file transfers with `KJobTracker`, which emits native progress/complete notifications. The patch:
- Comments out `#include <KJobTrackerInterface>`
- Skips `Daemon::instance()->jobTracker()->registerJob(m_compositeJob)`

With no tracker registration, the daemon stays silent — cosmic-connect owns the entire transfer UI.

**Pairing request notifications** (`daemon/kdeconnectd.cpp`):

Stock KDE Connect creates a `KNotification` with Accept/Reject/View Key actions when a device requests pairing. The patch replaces `askPairingConfirmation()` with a no-op:

```cpp
// Notification suppressed — cosmic-connect handles pairing UI
Q_UNUSED(device);
```

cosmic-connect handles pairing inline via D-Bus (`AcceptPairing`/`CancelPairing` actions in the device card).

To verify the patched daemon is running:

```bash
killall kdeconnectd
/usr/lib/x86_64-linux-gnu/libexec/kdeconnectd
```

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

## Comparison to other COSMIC KDE Connect applets

Several projects integrate KDE Connect with the COSMIC desktop. Here is how they approach the problem:

| Project | Approach | Depends on | Progress D-Bus signals | Notification suppression | SMS thread merging |
|---|---|---|---|---|---|
| **cosmic-connect** (this) | D-Bus wrapper | Stock KDE Connect daemon | ✅ (patched fork) | ✅ (patched fork) | ❌ |
| [cosmic-ext-connected](https://github.com/nwxnw/cosmic-ext-connected) | D-Bus wrapper | Stock KDE Connect daemon | ❌ | ❌ (documents dupes as known issue) | ✅ |
| [cosmic-utils/kdeconnect](https://github.com/cosmic-utils/kdeconnect) | Native Rust reimplementation | None (own daemon) | ❌ | N/A (no stock daemon) | ❌ |
| [olafkfreund/cosmic-ext-connect](https://github.com/olafkfreund/cosmic-ext-connect-desktop-app) | Native Rust + Android app | Own protocol (CConnect) | ❌ | N/A (own daemon) | ❌ |

**cosmic-connect** and **cosmic-ext-connected** both wrap the stock KDE Connect daemon over D-Bus. The main differences:

- **Patched daemon** — cosmic-connect's [fork](#patched-kde-connect-fork) adds D-Bus signals for transfer progress and suppresses native notifications, giving a unified UI without duplicates. cosmic-ext-connected documents "you may see duplicate notifications" as a known issue.
- **SMS merging** — cosmic-ext-connected detects iOS reaction-over-SMS split threads and merges them automatically.

**cosmic-utils/kdeconnect** and **olafkfreund/cosmic-ext-connect** are full protocol reimplementations in Rust. They do not need the stock KDE Connect daemon at all, but require significantly more code (protocol stack, encryption, discovery, pairing, their own daemon). cosmic-connect is a thin ~2k-line D-Bus client that reuses the battle-tested KDE Connect C++ daemon.

## Known issues

- Transfer progress and native notification suppression require the [patched KDE Connect fork](#patched-kde-connect-fork)
- File chooser requires libcosmic built with `cosmic::dialog` feature
- Wayland-only (uses wl-paste)
- No SMS/conversation UI yet

## Contributing

Bug reports and PRs welcome. Test against live D-Bus before submitting.
