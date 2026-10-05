# Publishing to the Microsoft Store

Matchstick is in the Store as **Matchstick Launcher**
(`HarshalPatel.MatchstickLauncher`, Store ID `9PJ64FKJ8C40`). The Store signs
the package, so people who install from there never see a SmartScreen warning.

## How the Store edition differs

It is the same program, packaged as MSIX. At run time it notices it is inside
a package (`store::is_packaged`) and:

- does not check for or install updates itself; the Store does that
- uses the package's startup task for "Start with Windows" instead of a registry entry
- cannot remove an old AnCheck install (only the classic installer does that)

## Each release

1. Tag the release as usual. The release workflow builds
   `Matchstick_<version>_x64.msix` and attaches it to the GitHub release.
2. In [Partner Center](https://partner.microsoft.com/dashboard) open
   **Apps and games → Matchstick Launcher** and start a submission (or an update).
3. Under **Packages**, upload the `.msix` from the GitHub release.
4. Submit. Certification usually takes a few days.

The package version is the app version with the first number raised by one
(0.2.2 → 1.2.2.0), because the Store does not accept versions starting with 0.

## First submission: what Partner Center asks for

**Pricing and availability:** Free, all markets.

**Properties:** category *Utilities & tools*. Privacy policy URL:
`https://github.com/HarshalPatel1972/win-light/blob/main/PRIVACY.md`.
Support contact: the GitHub issues page.

**Age ratings:** answer the questionnaire; the app has no user-generated
content, no purchases and no communication features.

**Restricted capability `runFullTrust`:** Partner Center asks why. Suggested answer:

> Matchstick is a desktop launcher. It needs full trust to read file names in
> the user's folders, register a global keyboard shortcut, show the icons of
> installed apps, and start the app or file the user picks. It does not run in
> the background beyond that and sends no file data anywhere.

**Store listing (English):**

- *Description:*

  > Strike a match, find anything on your PC.
  >
  > Press Ctrl+Space from anywhere and type a few letters. Matchstick finds
  > your apps, files and folders, documents by what is written inside them,
  > windows you already have open, and Windows settings. It answers sums,
  > unit and currency conversions on the spot, and remembers what you open so
  > the things you use most come first.
  >
  > • Fast: results as you type, with real icons and a preview pane
  > • Personal: shows what you come back to, and tells you why a result ranks first
  > • Private: your files and searches stay on your PC. No account, no tracking
  > • Yours: choose the shortcut, theme, language and which folders are searched

- *Short description:* A fast, private launcher: find apps, files, documents and answers with a few keystrokes.
- *Search terms:* launcher, search, spotlight, app launcher, file search, productivity, quick launch
- *Screenshots:* at least one, 1366×768 or larger.

## Trying the package locally

With Developer Mode on, a build can be registered without packing or signing:

```powershell
scripts\build-msix.ps1 -Version 0.2.2 -Exe src-tauri\target\release\matchstick.exe -OutDir msix
Add-AppxPackage -Register msix\layout\AppxManifest.xml
# ... try it from the Start menu ...
Get-AppxPackage HarshalPatel.MatchstickLauncher | Remove-AppxPackage
```
