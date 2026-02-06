# AnCheck — Spotlight-like Search Launcher for Windows

[![Build](https://github.com/HarshalPatel1972/win-light/actions/workflows/build.yml/badge.svg)](https://github.com/HarshalPatel1972/win-light/actions/workflows/build.yml)
[![Release](https://github.com/HarshalPatel1972/win-light/releases/latest)](https://github.com/HarshalPatel1972/win-light/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A fast, native desktop search launcher built with **Rust + Tauri v2** and **React + TypeScript**. Press `Ctrl+Space` from anywhere to instantly search and launch apps, files, and folders.

---

## Installation

### Download Installer

Go to the [**Releases**](https://github.com/HarshalPatel1972/win-light/releases/latest) page and download:

| File | Description |
|------|-------------|
| `AnCheck_x.x.x_x64-setup.exe` | **NSIS Installer** (recommended) — installs to Program Files, creates Start Menu shortcut |
| `AnCheck_x.x.x_x64_en-US.msi` | **MSI Installer** — standard Windows installer |

### System Requirements

- **Windows 10** (21H2+) or **Windows 11**
- [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (pre-installed on most modern Windows systems; the installer will download it automatically if missing)

---

## Features

- **Global Hotkey** — `Ctrl+Space` toggles the launcher from any application
- **Fast File Indexing** — Indexes Start Menu, Program Files, Desktop, Documents, Downloads
- **Fuzzy Search** — Multi-strategy matching: exact → prefix → substring → fuzzy
- **Smart Ranking** — Boosts apps, frequently-used items, and recently-opened files
- **Calculator** — Type math expressions like `2+2` or `(100/5)*3` for instant results
- **System Tray** — Runs quietly in the tray with right-click menu
- **Keyboard-First** — Full navigation with ↑↓, Enter, Esc, Ctrl+1-9 quick-launch
- **Auto-Updates** — Automatically checks for new versions from GitHub Releases
- **Polished UI** — Frameless overlay with blur effect, dark theme, smooth animations

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+Space` | Toggle launcher (global, works from any app) |
| `↑` / `↓` | Navigate results |
| `Enter` | Open selected item |
| `Esc` | Close launcher |
| `Tab` / `Shift+Tab` | Cycle through results |
| `Ctrl+1` – `Ctrl+9` | Quick-launch first 9 results |
| Right-click result | Open containing folder |

---

## System Tray

When the launcher window is hidden, AnCheck stays in the system tray:

- **Left click** — Show launcher
- **Right click → Show Launcher** — Show launcher
- **Right click → Rebuild Index** — Force full re-index
- **Right click → Exit** — Quit the application

---

## Building from Source

### Prerequisites

| Tool    | Minimum Version | Check Command      |
|---------|----------------|--------------------|
| Rust    | 1.70+          | `rustc --version`  |
| Node.js | 18+            | `node --version`   |
| npm     | 9+             | `npm --version`    |

### Development

```bash
# Clone the repository
git clone https://github.com/HarshalPatel1972/win-light.git
cd win-light

# Install dependencies
npm install

# Run in development mode (hot-reload)
npm run tauri dev
```

### Production Build

```bash
# Build release executable + installers
npm run tauri build

# Output:
#   NSIS installer → src-tauri/target/release/bundle/nsis/
#   MSI installer  → src-tauri/target/release/bundle/msi/
#   Executable     → src-tauri/target/release/ancheck.exe
```

---

## Architecture

```
ancheck/
├── src/                          # React frontend
│   ├── App.tsx                   # Main app — wires search, navigation, events
│   ├── main.tsx                  # Entry point
│   ├── components/
│   │   ├── SearchInput.tsx       # Search bar with auto-focus and clear button
│   │   ├── ResultsList.tsx       # Scrollable results with math display
│   │   └── ResultItem.tsx        # Single result row with icon, highlight, badge
│   ├── hooks/
│   │   ├── useSearch.ts          # Debounced search + math eval via Tauri invoke
│   │   └── useKeyboardNav.ts     # Arrow, Enter, Esc, Tab, Ctrl+N navigation
│   └── styles/
│       └── main.css              # Complete styling with Tailwind + custom CSS
│
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── lib.rs                # Tauri setup, commands, tray, hotkey, background tasks
│   │   ├── db.rs                 # SQLite database: schema, upsert, search, metadata
│   │   ├── indexer.rs            # File system walker: scans directories, classifies files
│   │   ├── searcher.rs           # Multi-strategy search: SQL + fuzzy + scoring + math eval
│   │   └── launcher.rs           # File/app launching: exe, lnk, shell open, explorer
│   ├── Cargo.toml                # Rust dependencies + release optimizations
│   └── tauri.conf.json           # Window config, bundle settings, NSIS config, updater
│
├── .github/workflows/
│   ├── release.yml               # CI/CD: build + publish on tag push
│   └── build.yml                 # CI: check + test on push/PR
│
├── scripts/
│   └── version-bump.js           # Bump version across all manifests
│
├── CHANGELOG.md
├── RELEASE.md                    # Release process documentation
├── LICENSE                       # MIT License
└── package.json
```

### Search Scoring

Results are ranked by a composite score:

1. **Match quality**: Exact (1000) > Prefix (800) > Substring (600) > Path (300) > Fuzzy (variable)
2. **File type boost**: Apps (+50) > Shortcuts (+40) > Documents (+20) > Folders (+15)
3. **Usage boost**: Logarithmic click count + recency decay
4. **Maximum 15 results** returned per query

---

## Performance

| Metric | Target |
|--------|--------|
| Search response | <100ms (50ms typical) |
| Initial index (50K files) | <30 seconds |
| Window show/hide | <50ms |
| Memory (idle) | <80MB |
| Background re-index | Every 5 minutes |

---

## Releasing a New Version

```bash
# 1. Bump version everywhere
npm run version:bump 0.2.0

# 2. Commit and tag
git add -A
git commit -m "chore: release v0.2.0"
git tag v0.2.0
git push origin main --tags

# GitHub Actions automatically builds, creates the release,
# and uploads installers + auto-update manifest.
```

See [RELEASE.md](RELEASE.md) for full details.

---

## Troubleshooting

### "WebView2 not found"
The NSIS installer downloads WebView2 automatically. If installing manually, get it from [Microsoft](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).

### Global hotkey doesn't work
Another application may have registered `Ctrl+Space`. Close conflicting apps and restart AnCheck.

### No search results
Wait for initial indexing to complete (watch the status bar). Force re-index from the tray menu.

### Build fails
Ensure the latest Rust toolchain: `rustup update stable`

---

## License

[MIT](LICENSE) © 2026 Harshal Patel
