# Release Process

This document describes how to create a new release for AnCheck.

---

## Prerequisites

- [Rust](https://rustup.rs/) 1.70+
- [Node.js](https://nodejs.org/) 18+
- Push access to the GitHub repository
- GitHub Secrets configured:
  - `TAURI_SIGNING_PRIVATE_KEY` — contents of `src-tauri/.tauri-private.key`
  - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — password used when generating the key

---

## Version Numbering

The project follows [Semantic Versioning](https://semver.org/):

| Bump  | When                                                |
|-------|-----------------------------------------------------|
| MAJOR | Breaking changes or fundamental redesigns           |
| MINOR | New features that are backward-compatible           |
| PATCH | Bug fixes, performance improvements, minor tweaks   |

---

## Creating a New Release

### 1. Update the version

```bash
# Bump to a specific version (e.g. 0.2.0)
npm run version:bump 0.2.0
```

This updates the version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.

### 2. Update CHANGELOG.md

Move items from `[Unreleased]` into a new version section:

```markdown
## [0.2.0] - 2026-MM-DD

### Added
- New feature X

### Fixed
- Bug Y
```

### 3. Commit and tag

```bash
git add -A
git commit -m "chore: release v0.2.0"
git tag v0.2.0
git push origin main --tags
```

### 4. GitHub Actions takes over

Pushing the tag triggers the **Release** workflow which:
1. Builds the Tauri app in release mode
2. Creates NSIS and MSI installers
3. Creates a GitHub Release with auto-generated release notes
4. Uploads all installer artifacts + `latest.json` for auto-updates

---

## Pre-Release Checklist

- [ ] All features for this version are merged to `main`
- [ ] `npm run tauri dev` works locally without errors
- [ ] `cargo test` passes all tests
- [ ] `npx tsc --noEmit` shows no TypeScript errors
- [ ] CHANGELOG.md is updated
- [ ] Version bump script has been run
- [ ] No hardcoded debug/dev settings remain

---

## Local Build Testing

Before creating a release tag, test the production build locally:

```bash
# Build production executable + installer
npm run tauri build

# Output locations:
#   NSIS installer: src-tauri/target/release/bundle/nsis/AnCheck_0.2.0_x64-setup.exe
#   MSI installer:  src-tauri/target/release/bundle/msi/AnCheck_0.2.0_x64_en-US.msi
#   Portable exe:   src-tauri/target/release/ancheck.exe
```

Test the installer:
1. Run the NSIS `.exe` installer
2. Verify it installs to `C:\Program Files\AnCheck\` (or user-selected directory)
3. Verify Start Menu shortcut is created
4. Launch from Start Menu
5. Press `Ctrl+Space` — launcher should appear
6. Verify system tray icon is present
7. Run the uninstaller from Add/Remove Programs

---

## Rollback Procedure

If a release has critical issues:

### Option A: Quick fix
```bash
git revert <bad-commit>
npm run version:bump 0.2.1
git add -A
git commit -m "chore: release v0.2.1 (hotfix)"
git tag v0.2.1
git push origin main --tags
```

### Option B: Delete the release
1. Go to GitHub → Releases → Delete the bad release
2. Delete the tag: `git push --delete origin v0.2.0 && git tag -d v0.2.0`
3. Fix the issue, bump version, and re-release

---

## GitHub Secrets Setup

Navigate to **Settings → Secrets and variables → Actions** in the GitHub repository and add:

| Secret Name                        | Value                                            |
|------------------------------------|--------------------------------------------------|
| `TAURI_SIGNING_PRIVATE_KEY`        | Full contents of `src-tauri/.tauri-private.key`  |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password used during `tauri signer generate`   |

> ⚠️ **Never commit the private key to the repository.** The `.gitignore` is configured to exclude it.

---

## Auto-Update Flow

When a new release is published:
1. GitHub Actions uploads `latest.json` as a release asset
2. The running AnCheck app periodically checks the `latest.json` endpoint
3. If a newer version is found, a dialog prompts the user to update
4. The update is downloaded, signature-verified, and installed automatically
