# Changelog

All notable changes to Matchstick will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Settings screen (`Ctrl+,`, footer button or tray menu): launcher shortcut, start with Windows, theme, language
- Microsoft Store / packaged apps (Calculator, Settings, …) are indexed and launchable
- Real file and app icons in results
- Light theme, following the system setting by default
- Interface languages: English, Spanish, French, German, Portuguese, Hindi, Chinese, Japanese
- Update banner with one-click install; update checks at startup and every 6 hours
- Starts with Windows (in the tray) by default on installed builds
- Launch errors are shown in the status bar
- Home view: a time-of-day greeting and the items you open most, ready to launch before you type
- Results show their history with you ("Opened 14× · 2 hours ago")
- First-run story: an animated introduction that explains the name (a match struck in the dark lights up your own apps) and shows how to use the launcher with a real app from your PC
- New visual identity: ink-and-amber "lamp" theme, a spark that pulses while searching, selection lit in the colour of the item's kind

- **Search inside documents.** Results now include files whose *contents* match, with the matching passage, by querying the index Windows already keeps (no extra indexing, no files read by Matchstick)
- **Conversions and smarter maths:** units (`5 km to miles`, `100 f in c`), currency (`120 usd in inr`, rates cached for offline use and fetched only when a currency is typed), percentages (`15% of 240`) and powers (`2^10`). Click an answer to copy it
- **Preview pane** for the selected result: a real thumbnail, size, date, how often you open it, and the matching passage
- **Actions on a result:** show in folder (`Ctrl+Enter`), run as administrator (`Ctrl+Shift+Enter`), open with (`Ctrl+O`), copy path (`Ctrl+Shift+C`)
- **Web:** every search ends with "Search the web for…", and a typed address (`github.com`) opens directly. The search engine is a setting

- **System commands:** lock, sleep, sign out, restart, shut down, empty the recycle bin, and 18 pages of Windows Settings (`display`, `wifi`, `bluetooth`, …). Commands that cannot be undone ask for a second Enter
- **Switch to open windows:** a window that is already open and matches the query is offered first
- **Keyword shortcuts:** `yt lofi beats` searches YouTube; `gh`, `wiki` and `maps` are built in and the list is editable in Settings
- **Text preview:** the opening lines of text and code files are shown in the preview pane
- New app icon

### Changed
- The whole user folder and other internal drives are indexed, not just Desktop, Documents and Downloads
- A program file is hidden when its Start Menu shortcut is already listed (no more `POWERPNT.EXE` next to PowerPoint)
- Build and package folders (`target`, `dist`, `venv`, …) and hidden folders are skipped
- **Renamed from AnCheck to Matchstick.** Data now lives in `%LOCALAPPDATA%\Matchstick`; the index and launch history of an existing AnCheck install are imported on first run
- Search runs against an in-memory index instead of scanning SQLite on every keystroke
- Fuzzy matching uses `nucleo-matcher`; multi-word queries match words in any order
- The index is kept current by a file-system watcher; the full rescan now runs every 6 hours instead of every 5 minutes
- Program Files is indexed for executables only, which shrinks the index considerably
- Desktop, Documents and Downloads are located through the Known Folders API (works with OneDrive redirection)
- Hidden and system files are no longer indexed
- Files are opened through `ShellExecuteW` instead of `cmd /C start`

### Fixed
- Auto-update was configured but never checked for updates
- File names containing `&`, `%` or `^` could be interpreted as shell commands when opened
- A second launch started a second instance with a dead hotkey and a duplicate tray icon
- Match highlighting was misaligned for non-ASCII file names
- A console window flashed when opening documents and shortcuts

### Security
- Content Security Policy enabled; the global Tauri object is no longer exposed
- The backend only opens paths that are in the index

## [0.1.0] - 2026-02-06

### Added
- Initial release
- Global hotkey `Ctrl+Space` to toggle the launcher from any application
- File system indexing of Start Menu, Program Files, Desktop, Documents, Downloads
- Multi-strategy search: exact → prefix → substring → fuzzy
- Smart ranking with file-type boost, usage frequency, and recency decay
- Built-in calculator for math expressions (e.g. `2+2`, `(100/5)*3`)
- System tray with right-click menu (Show, Rebuild Index, Exit)
- Full keyboard navigation: ↑↓, Enter, Esc, Tab, Ctrl+1-9
- Right-click context menu to open containing folder
- Background incremental re-indexing every 5 minutes
- NSIS installer for Windows
- Auto-update support via GitHub Releases
- Frameless dark-theme UI with blur effect and smooth animations

[Unreleased]: https://github.com/HarshalPatel1972/win-light/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/HarshalPatel1972/win-light/releases/tag/v0.1.0
