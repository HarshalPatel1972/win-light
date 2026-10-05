# Privacy

Matchstick is a launcher that runs on your PC. It has no account, no analytics and no telemetry, and it never uploads anything about your files.

## What it reads

- **Names and locations of your files and apps.** To search them, Matchstick keeps an index of file names, paths, sizes and dates in `%LOCALAPPDATA%\Matchstick\index.db`. It does not read or store what is inside your files.
- **What you open through it.** It counts how often and how recently you open each item, so that the things you use most come first. This stays in the same local database.
- **The Windows Search index.** When you search, Matchstick asks Windows which documents contain your words and shows the passage Windows returns. The search runs inside Windows on your PC; Matchstick does not keep those passages.
- **Titles of open windows**, while you are searching, so it can offer to switch to one. They are not stored.

You choose what is indexed: add or exclude folders under **Settings → Indexed folders**.

## What leaves your PC

Only these requests, and only to do what you asked:

| When | Where it goes | What is sent |
|------|---------------|--------------|
| You type a currency conversion (e.g. `120 usd in inr`) | `open.er-api.com` | A request for the public exchange-rate table. Not your query, not the amount. At most twice a day. |
| The app checks for updates (at start and every 6 hours) | `github.com` | A request for the latest version number. |
| You choose a web result or a keyword shortcut | Your browser, then the site you chose | The words you typed for that search. |

Nothing else is sent. Searching your files never touches the network.

## What is stored, and where

Everything lives in `%LOCALAPPDATA%\Matchstick`:

- `index.db` — the index and your usage counts
- `settings.json` — your settings
- `rates.json` — the last exchange-rate table
- `matchstick.log` — a diagnostic log (file paths may appear in it); it is only ever sent anywhere if you attach it to a problem report yourself

Uninstalling Matchstick and deleting that folder removes all of it.

## Questions

Open an issue at <https://github.com/HarshalPatel1972/win-light/issues>.
