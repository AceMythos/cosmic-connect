# COSMIC Connect

A powerful **COSMIC applet** for **KDE Connect** integration on Linux. Control your connected devices directly from the COSMIC desktop environment with an intuitive popup interface.

## Features

🔌 **Device Management**
- Pair, unpair, and manage device connections
- View device type, connection status, and battery level
- Real-time device discovery with polling

📱 **Device Control**
- **Ping**: Send a test ping to verify connectivity
- **Ring**: Locate your device with a ring/alarm
- **Clipboard**: Push text to device clipboard or read from local clipboard
- **File Sharing**: Send files to your device via SFTP
- **URL & Text Sharing**: Share URLs and text snippets instantly
- **File Browser**: Browse device files remotely via SFTP

🔐 **Pairing Controls**
- Request pairing from new devices
- Accept or cancel incoming pair requests
- Unpair devices with one click
- View detailed pairing state

📊 **Status Monitoring**
- Connection status indicators
- Battery percentage and charging state
- Real-time action feedback
- Error reporting for failed operations

## Project Structure

```
cosmic-connect/
├── src/
│   ├── main.rs              # Entry point for the applet
│   ├── lib.rs               # Module exports
│   ├── app.rs               # Main UI and event loop (27KB)
│   ├── model.rs             # Data structures (Device, ActionType)
│   ├── backend/
│   │   └── mod.rs           # KDE Connect D-Bus interface (11KB)
│   ├── bin/
│   │   └── test_backend.rs  # CLI tool for backend testing
│   └── widgets/
│       └── mod.rs           # Future: custom UI widgets
├── Cargo.toml               # Rust dependencies
├── Cargo.lock               # Locked dependency versions
├── io.github.acemythos.Connect.desktop  # Desktop entry file
└── WORK_SUMMARY.md          # Development notes
```

## Architecture

### Core Components

**Backend (`src/backend/mod.rs`)**
- D-Bus interface to KDE Connect daemon (`org.kde.kdeconnect`)
- Async device enumeration and status retrieval
- Method calls for ping, ring, clipboard, sharing, SFTP, and pairing
- Returns structured `Device` objects with live state

**Model (`src/model.rs`)**
- `Device`: Represents a paired or available device with properties (name, type, battery, plugins)
- `DeviceType`: Phone, tablet, laptop, desktop with icon mapping
- `ActionType`: Enum of all supported device actions
- `BatteryInfo`: Charge percentage and charging status

**App (`src/app.rs`)**
- COSMIC applet lifecycle management
- Popup UI with device list and action buttons
- Per-device draft state for forms (clipboard, URLs, files)
- 2-second polling subscription for live device updates
- Async action execution with result feedback

### Data Flow

```
COSMIC Applet (app.rs)
    ↓
Message Router
    ↓
KdeConnectBackend (async tasks)
    ↓
D-Bus Session Bus
    ↓
KDE Connect Daemon (org.kde.kdeconnect)
    ↓
Connected Devices
```

## Building

### Requirements
- Rust 1.70+ (edition 2021)
- Linux with D-Bus session
- libcosmic (from pop-os/libcosmic)
- libzbus for D-Bus communication
- wl-paste for clipboard access

### Compile
```bash
cargo build --release
```

### Test Backend Connection
```bash
cargo run --bin test_backend
```

This checks that KDE Connect is running and lists available devices.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `libcosmic` | COSMIC applet framework and UI widgets |
| `zbus` | D-Bus async client |
| `tokio` | Async runtime |
| `futures-util` | Stream utilities for subscriptions |
| `ashpd` | XDG portal for file chooser dialog |
| `serde` | Data serialization |
| `log` | Logging infrastructure |

## Usage

### As an Applet
1. Build the project: `cargo build --release`
2. The applet will auto-discover via the `.desktop` file
3. Click the COSMIC Connect icon in the panel
4. Popup displays paired devices and quick actions

### As a CLI
Test the backend connection:
```bash
cargo run --bin test_backend
```

Output shows device names, types, battery, and available plugins.

## Key Implementation Details

### Device Discovery
- Uses KDE Connect D-Bus service at `org.kde.kdeconnect`
- Fetches all device IDs via the daemon's `devices()` method
- Polls every 2 seconds for real-time updates
- Falls back gracefully if KDE Connect is not running

### Plugin Detection
- Checks `loadedPlugins` (preferred) over `supportedPlugins`
- Only shows actions if the plugin is actually loaded
- Supports: ping, findmyphone, clipboard, share, sftp, battery

### Action Execution
- All backend methods are async and non-blocking
- Results return `Result<String>` for success/error messages
- UI shows "Working..." status during action execution
- Automatic refresh after each action completes

### UI State Management
- Per-device draft state: clipboard text, share text, URLs, file paths
- Status line displays action results (success/error)
- Form inputs are cleared per-device, not globally
- Drafts sync with device list (orphaned drafts removed)

## Known Limitations

- **File Chooser**: Requires libcosmic with `cosmic::dialog` feature enabled
- **Wayland Only**: Uses wl-paste for clipboard access (Wayland requirement)
- **Compilation**: The `app.rs` rewrite is still resolving compile errors
- **No SMS**: SMS/conversation UI is not yet implemented
- **No Mount UI**: SFTP mount/unmount state not displayed

## Current Status

The applet is in **active development**. The backend is fully functional, but the app frontend compile errors need resolution:

1. `device_id` moved into async block and reused (needs clone)
2. `view_window()` temporary borrow lifetime issue (needs restructuring)

See `WORK_SUMMARY.md` for detailed development progress.

## Testing

### Manual Testing Steps
1. Ensure KDE Connect daemon is running: `systemctl --user start kdeconnect`
2. Pair a device using the KDE Connect app or CLI
3. Run `cargo run --bin test_backend` to verify connectivity
4. Launch the applet and test each action type

## License

This project is maintained by @AceMythos. Check the repository for license details.

## Contributing

Issues and pull requests are welcome. Please follow the existing code style and test any changes with live D-Bus.

---

**Built with:** Rust · COSMIC · KDE Connect · D-Bus
