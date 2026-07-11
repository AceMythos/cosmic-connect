# Upstream Report: KDE Connect Transfer Progress D-Bus Signals

## Goal
Add 4 D-Bus signals to `org.kde.kdeconnect.device.share` so third-party clients can display live file transfer progress without forking the daemon.

## Background
KDE Connect's stock D-Bus interface only emits `shareReceived` when a file is fully done. The desktop GUI shows progress internally via `KJob::processedAmount`, but this is never exposed over D-Bus.

The patch was tested on `kdeconnect-fork` branch `v23.08.5-patched` and consumed by cosmic-connect. Current upstream master has drifted since v23.08.5 (refactored notification code, `CompositeFileTransferJob` API change, `receivePacket` return type, etc.), so the patch needs rebasing.

## Patch Summary (~30 lines)

### Files to change

**`plugins/share/shareplugin.h`** — 5 additions:
```cpp
#include <QElapsedTimer>
#include <QUuid>

// Inside class, in Q_SIGNALS section:
Q_SCRIPTABLE void transferStarted(const QString &transferId, const QString &fileName, quint64 totalBytes);
Q_SCRIPTABLE void transferProgress(const QString &transferId, quint64 bytesTransferred, quint64 totalBytes, int percent);
Q_SCRIPTABLE void transferFinished(const QString &transferId, const QString &url);
Q_SCRIPTABLE void transferFailed(const QString &transferId, int errorCode, const QString &errorString);

// Member variable:
QElapsedTimer m_progressThrottle;
```

**`plugins/share/shareplugin.cpp`** — in `receivePacket()` payload branch:

1. **Generate transfer ID**: `QUuid::createUuid().toString(QUuid::WithoutBraces)`
2. **Get total bytes**: `np.payloadSize()` (handles `-1` → `0` fallback)
3. **Emit `transferStarted`**: right before connecting job signals
4. **Connect `KJob::processedAmount`** → emit `transferProgress` (throttled to 150ms via `QElapsedTimer`)
5. **Modify `KJob::result` handler**: emit `transferFinished` (with destination URL) or `transferFailed` (with error code + string)
6. **Keep `shareReceived`** emission in `finished()` for backward compat

### What NOT to touch
- `shareReceived` signal — leave it intact
- The text/URL sharing paths — no changes needed
- Notification code — upstream has been refactored since v23.08.5, unrelated

## Upstream Drift: v23.08.5-patched vs current master

| Area | Fork (v23.08.5) | Upstream master | Action needed |
|---|---|---|---|
| `receivePacket` return | `bool` | `void` | Patch must use `void` |
| `CompositeFileTransferJob` constructor | `(device()->id())` | `(device(), this)` | Already matches upstream |
| `setAutoRename...` | Typo: `Destinatinon` | `Destination` | Just don't touch it |
| Text notification handling | Old `KNotification::actions()` | New `KNotificationAction` + `KIO::OpenUrlJob` | Ignore — unrelated |
| `dbusPath()` | `QStringLiteral(...)` | `QLatin1String(...)` | Ignore — cosmetic |
| CMake output | `shareplugin.moc` only | `moc_shareplugin.cpp` + `shareplugin.moc` | Must match upstream |

## Rebase Steps

```bash
# 1. Clone upstream
git clone https://invent.kde.org/network/kdeconnect-kde.git
cd kdeconnect-kde

# 2. Create branch
git checkout -b transfer-progress-dbus

# 3. Make changes to shareplugin.h and shareplugin.cpp
#    (only add the 5 items listed above, adapt to current code)

# 4. Build
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build

# 5. Test
killall kdeconnectd
./build/bin/kdeconnectd

# 6. From another terminal, send test file from phone or use:
#    qdbus to verify signals fire
```

## Submitting Upstream

Push to your fork on invent.kde.org (not GitHub), then open an MR against:
- `invent.kde.org/network/kdeconnect-kde`
- Branch: `master`
- Title: `Add transfer progress D-Bus signals to SharePlugin`

## Alternative: Just the Reddit question

If you want to ask before doing the work, post to `/r/kde`:

> Why doesn't KDE Connect expose file transfer progress over D-Bus?
>
> The desktop GUI shows progress internally (via KJob::processedAmount), but the D-Bus interface (org.kde.kdeconnect.device.share) only emits shareReceived when a file is fully done. Third-party clients can't show progress without forking the daemon and adding signals themselves.
>
> I patched it in ~30 lines — transferStarted/transferProgress/transferFinished/transferFailed signals using the existing KJob hook. Is there a design reason this wasn't done originally, or just nobody asked for it? Would a MR be welcome?

## Key Files in cosmic-connect

| File | Purpose |
|---|---|
| `src/backend/mod.rs` | D-Bus backend — all KDE Connect calls |
| `src/model.rs` | `ReceivedFile` struct with progress tracking |
| `src/app.rs` | `ShareSignalState` (line 1627) — listens for the 4 signals and renders progress bars |

### ShareSignalState (app.rs:1627-1740)

Subscribes to all signals on `org.kde.kdeconnect.device.share` via `MatchRule` (no member filter). Parses `header.member()` to determine signal type and deserializes args from D-Bus message body:

```rust
let rule = MatchRule::builder()
    .msg_type(zbus::message::Type::Signal)
    .interface("org.kde.kdeconnect.device.share")
    .unwrap()
    .build();
MessageStream::for_match_rule(rule, &conn, None).await
```

Signal dispatch:
- `"shareReceived"` → `Message::FileReceived(device_id, file_path)` — legacy fallback
- `"transferStarted"` → `Message::TransferStarted(device_id, transfer_id, file_name, total_bytes)`
- `"transferProgress"` → `Message::TransferProgress(device_id, transfer_id, bytes_transferred, total, percent)`
- `"transferFinished"` → `Message::TransferFinished(device_id, transfer_id, url)`
- `"transferFailed"` → `Message::TransferFailed(device_id, transfer_id, error_code, error_string)`

## KDE AI Contribution Policy (research findings)

There is **no blanket ban** on AI-assisted code at KDE. The proposed (and broadly supported) policy from the kde-core-devel mailing list (Nov 2025):

- Contributor is always responsible for the code, regardless of origin (AI, copy-paste, etc.)
- Contributor must understand the changes and be able to justify them
- Copyright belongs to the contributor, not the tool
- Disclose AI tools used in the MR description
- Do NOT include "Co-authored-by:" or "Assisted-by:" for AI tools
- Individual maintainers have final discretion

Key quote from the KDE Community Wiki: "You're responsible for what you commit. When you make a merge request, it's your code, even if it originates from the internet, an LLM, a friend, or your pet lizard."

## Links
- Patched fork: https://github.com/AceMythos/kdeconnect-fork (branch `v23.08.5-patched`)
- Upstream: https://invent.kde.org/network/kdeconnect-kde
- cosmic-connect: https://github.com/AceMythos/cosmic-connect
