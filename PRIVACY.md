# Privacy

Seiza collects nothing. It has no accounts, no analytics, no ads, and no servers of its own. Nothing you do in it is sent to us or anyone else. If you turn on sync, your data goes to a sync server you choose, encrypted so that server can't read it.

This covers the Seiza desktop app and the Seiza browser extension.

## What stays on your computer

- **Your profiles, sessions and settings:** the sites and apps each profile tames, redirects or restricts, the parts of pages you've hidden, and your other settings.
- **Your history:** when your sessions ran, and each visit to a redirected site: which site, when, how many times today, and the reason you gave, if you gave one. This is what the night sky and the weekly report are drawn from.
- **Your satellites and your own quotes.**

The extension keeps its copy in your browser's extension storage. The app keeps its copy in its data folder on your computer, along with sync's key and this computer's sign-in for the sync server if you use sync. Uninstalling either removes its copy. A backup from the app's Settings is a file you save wherever you choose.

## What the extension and the app share

When both are installed, the extension and the app talk to each other over your browser's Native Messaging, which never leaves your computer. They share your profiles, the current session, site settings and visits to redirected sites, so each one shows the same thing.

## What the extension reads

On sites it tames, the extension adds a stylesheet that hides parts of the page, and pauses any video it hides. When you use "Hide parts of this page", it notes which part you clicked so it can hide it again later. It doesn't read or keep what you type or see on any page, and it doesn't keep your browsing history: the popup checks the address of the tab you open it on, so it can show that site's settings.

## What the app reads

- **How long since your last keyboard or mouse input**, only if you turn on ending a session when you're away, so one left running ends by itself.
- **Which programs are running**, only during a session, to close or hide the apps your profile lists and to notice a browser running without the extension.
- **The first day of the week set on your computer**, for the weekly report.

On macOS, if you turn on blocking sites without the extension, the app lists your session's restricted sites in your computer's hosts file while the session runs, and takes them out after.

## Network

The extension makes no network requests.

The app makes two kinds, and you can turn off both.

**Update checks.** Unless you turn them off in Settings › Updates, the app asks GitHub once a day whether there's a new version of Seiza, by downloading a small file (`latest.json`) from the project's releases. The request carries nothing about you or what you do in Seiza, though like any web request it shows GitHub your IP address ([GitHub's privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement) covers what GitHub keeps). If you update, the new version downloads from GitHub too, and the app checks its signature before installing it.

**Sync, only if you set it up.** Sync is off until you set it up in Settings › Sync, and Seiza has no sync server of its own: you use one you run yourself, or one someone you trust runs ([how to run one](https://github.com/seiza-app/seiza/tree/main/apps/sync-server)). Once it's on, the app sends that server your finished sessions and their goals, the session running now (with its goal and its profile's rules, so it runs on your other computers too), your visits to redirected sites, your constellation names, your profiles and this computer's name, and fetches your other computers' changes. It keeps one connection open to the server, so changes arrive within seconds. Before a session ends because you're away, the app asks your other computers how long it's been since their keyboard or mouse was used; the server passes the question and the answers on without keeping them. All of it is encrypted on your computer with a key the server never gets, so whoever runs the server can't read it. They can see which computers are in your group, when each last synced or was connected, how much it stores, and the IP addresses it syncs from. Your settings stay on each computer. **Leave sync** in Settings › Sync stops this computer syncing and keeps everything on it, and the server deletes your group once its last computer leaves. The key and the sign-in are left out of backups.

The quotes are bundled with the app. Links you add yourself, like the support link or your own links on the redirect page, open in your browser like any other link.

## Changes

If this ever changes, this page changes first, with the date below. The source code is public at <https://github.com/seiza-app/seiza>, so you can check any of it.

## Contact

Questions go to the repository's issues: <https://github.com/seiza-app/seiza/issues>.

Last updated: 2 November 2026.
