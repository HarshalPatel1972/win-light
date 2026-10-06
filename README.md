# Matchstick — strike a match, find anything on your PC

[![Build](https://github.com/HarshalPatel1972/win-light/actions/workflows/build.yml/badge.svg)](https://github.com/HarshalPatel1972/win-light/actions/workflows/build.yml)
[![Release](https://img.shields.io/github/v/release/HarshalPatel1972/win-light?label=release)](https://github.com/HarshalPatel1972/win-light/releases/latest)
[![Microsoft Store](https://img.shields.io/badge/Microsoft%20Store-Matchstick%20Launcher-0078D4)](https://apps.microsoft.com/detail/9PJ64FKJ8C40)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A fast, native search launcher for Windows, built with **Rust + Tauri v2** and **React + TypeScript**. Press `Ctrl+Space` from anywhere to find and open apps, files, documents by their contents, open windows, system commands and quick answers.

> **Beta.** Available in the [Microsoft Store](https://apps.microsoft.com/detail/9PJ64FKJ8C40) as *Matchstick Launcher*.

---

## Installation

### Microsoft Store (recommended)

**[Get Matchstick Launcher from the Microsoft Store](https://apps.microsoft.com/detail/9PJ64FKJ8C40)**

The Store version is signed by Microsoft, installs without any security warning, and updates itself through the Store.

### Download Installer

Prefer a classic installer? It is the same app. Because it is not code-signed, Windows shows a "Windows protected your PC" prompt on first run: choose **More info → Run anyway**.

Go to the [**Releases**](https://github.com/HarshalPatel1972/win-light/releases/latest) page and download:

| File | Description |
|------|-------------|
| `Matchstick_x.x.x_x64-setup.exe` | **NSIS Installer** (recommended) — installs to Program Files, creates Start Menu shortcut |
| `Matchstick_x.x.x_x64_en-US.msi` | **MSI Installer** — standard Windows installer |

### System Requirements

- **Windows 10** (21H2+) or **Windows 11**
- [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (pre-installed on most modern Windows systems; the installer will download it automatically if missing)

---

## Features

- **Global Hotkey** — `Ctrl+Space` toggles the launcher from any application (changeable in Settings)
- **Fast File Indexing** — Indexes apps (Start Menu and Store), your user folder and other internal drives, in seconds
- **Live Index** — A file-system watcher picks up new, renamed and deleted files within a second
- **Search Inside Documents** — Finds files by what is written in them, through the Windows Search index, and shows the matching passage
- **Fuzzy Search** — Multi-strategy matching: exact → prefix → substring → fuzzy, in memory
- **Smart Ranking** — Apps first, then what you use most and touched recently; each file name listed once
- **Answers** — Maths (`2^10`, `15% of 240`), units (`5 km to miles`, `100 f in c`) and currency (`120 usd in inr`)
- **Preview Pane** — Thumbnail, details and the opening lines of text files for the selected result
- **Actions** — Show in folder, run as administrator, open with, copy path
- **System Commands** — Lock, sleep, restart, shut down, empty recycle bin, and Windows Settings pages
- **Open Windows** — Switch to something already running instead of starting a second copy
- **Web** — Keyword shortcuts (`yt lofi beats`), typed addresses, and a web search when nothing local matches
- **System Tray** — Runs quietly in the tray with right-click menu; starts with Windows
- **Keyboard-First** — Full navigation with ↑↓, Enter, Esc, Ctrl+1-9 quick-launch
- **Auto-Updates** — Checks GitHub Releases at startup and every 6 hours; one-click install
- **Polished UI** — Frameless overlay with blur effect, real app icons, dark and light themes
- **Localized** — English, Spanish, French, German, Portuguese, Hindi, Chinese, Japanese

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
| `Ctrl+Enter` | Show the selected item in its folder |
| `Ctrl+Shift+Enter` | Run as administrator |
| `Ctrl+O` | Open with… |
| `Ctrl+Shift+C` | Copy the selected item's path |
| `Ctrl+,` | Open Settings |
| Right-click result | Open containing folder |

---

## Settings

Open with `Ctrl+,`, the ⚙ button in the footer, or the tray menu.

- **Launcher shortcut** — click the button and press a new combination. If another app already owns it, the previous shortcut stays active.
- **Start with Windows** — on by default for installed builds; the app starts hidden in the tray.
- **Theme** — System, Dark or Light.
- **Language** — Automatic (follows Windows) or a fixed language.
- **Web search and keyword shortcuts** — pick the search engine; edit shortcuts such as `yt`, `gh`, `wiki`.
- **Extra folders / Folders to leave out** — decide what is searchable.
- **Index** — rebuild on demand.
- **Updates** — check and install.

Settings are stored in `%LOCALAPPDATA%\Matchstick\settings.json`.

---

## Privacy

Your files and searches stay on your PC. There is no account, no analytics and no telemetry. The only network requests are the exchange-rate table (when you type a currency conversion), the update check, and web searches you choose to open. Details: [PRIVACY.md](PRIVACY.md).

## Coming from AnCheck?

Matchstick is AnCheck under a new name. Installing Matchstick removes AnCheck and keeps your launch history; you do not need both.

---

## System Tray

When the launcher window is hidden, Matchstick stays in the system tray:

- **Left click** — Show launcher
- **Right click → Show Launcher** — Show launcher
- **Right click → Settings…** — Open the settings screen
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
#   Executable     → src-tauri/target/release/matchstick.exe
```

---

## Architecture

```
matchstick/
├── src/                          # React frontend
│   ├── App.tsx                   # Main app — wires search, navigation, events
│   ├── main.tsx                  # Entry point
│   ├── i18n.ts                   # Translations and the useT() hook
│   ├── components/
│   │   ├── SearchInput.tsx       # Search bar with auto-focus and clear button
│   │   ├── ResultsList.tsx       # Scrollable results with math display
│   │   ├── ResultItem.tsx        # Single result row with icon, highlight, badge
│   │   └── Settings.tsx          # Settings screen
│   ├── hooks/
│   │   ├── useSearch.ts          # Debounced search + math eval via Tauri invoke
│   │   ├── useKeyboardNav.ts     # Arrow, Enter, Esc, Tab, Ctrl+N navigation
│   │   ├── useSettings.ts        # Settings state, theme application
│   │   └── useIcon.ts            # Cached shell icons for results
│   └── styles/
│       └── main.css              # Complete styling with Tailwind + custom CSS
│
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── lib.rs                # Tauri setup, commands, tray, hotkey, updater, background tasks
│   │   ├── db.rs                 # SQLite persistence: schema, upsert, reconcile, usage data
│   │   ├── indexer.rs            # Indexing rules, full scan, file-system watcher
│   │   ├── apps.rs               # Microsoft Store / packaged app enumeration
│   │   ├── searcher.rs           # In-memory index, ranking, fuzzy matching, math eval
│   │   ├── launcher.rs           # Launching through ShellExecuteW, reveal in Explorer
│   │   ├── icons.rs              # Shell icon extraction to PNG
│   │   ├── settings.rs           # User settings (JSON)
│   │   └── win.rs                # Shared Win32 helpers
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
| Index updates | Live (file watcher); full rescan every 6 hours |

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
Another application may have registered `Ctrl+Space` (it is also the input-method toggle for Chinese keyboards). Open the launcher from the tray icon and pick a different shortcut in Settings.

### No search results
Wait for initial indexing to complete (watch the status bar). Force re-index from the tray menu.

### Build fails
Ensure the latest Rust toolchain: `rustup update stable`

---

## License

[MIT](LICENSE) © 2026 Harshal Patel
