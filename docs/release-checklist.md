# Release checklist

Things automated tests cannot cover. Run through this on a real PC before
announcing a release. It takes about ten minutes.

## Install

- [ ] Download the setup `.exe` from the latest release and run it
- [ ] If AnCheck was installed, it is gone afterwards (Start Menu, Program Files)
- [ ] Matchstick starts and plays the first-run story
- [ ] The tray icon is the match, and its menu is in your language

## Everyday use

- [ ] `Ctrl+Space` opens and closes the launcher from another app
- [ ] With two monitors: it opens on the one the mouse is on
- [ ] Typing an app name and pressing Enter starts it
- [ ] `Ctrl+Enter` on a file opens its folder with the file selected
- [ ] `Ctrl+Shift+C` on a file, then pasting somewhere, gives its path
- [ ] `Ctrl+O` on a document shows the "Open with" dialog
- [ ] `Ctrl+Shift+Enter` on an app shows the Windows permission prompt
- [ ] Typing the name of an open window and pressing Enter switches to it
- [ ] `yt some words` opens a YouTube search in the browser

## Commands (these act on your PC: save your work first)

- [ ] `lock` locks the PC
- [ ] `sleep` puts it to sleep
- [ ] `empty recycle bin` asks for a second Enter, then empties the bin
- [ ] `sign out`, `restart`, `shut down` each ask for a second Enter, then do it
- [ ] `display`, `wifi`, `bluetooth` open the matching Settings page

## Settings

- [ ] Changing the shortcut works, and the old one stops working
- [ ] "Start with Windows" survives a restart (Matchstick is in the tray, window hidden)
- [ ] Adding a folder makes its files searchable; excluding one removes them
- [ ] Switching language changes the interface, the tray menu and command names

## Updates

- [ ] An install of the previous version shows "Update available" after this release is published
- [ ] Clicking Install updates and restarts into the new version (Settings shows the new number)
