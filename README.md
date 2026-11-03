# ✦ Seiza

A free, open-source focus app. Social media is light pollution: when you lock in, the noise fades and you can finally see the stars.

**Website:** [getseiza.app](https://getseiza.app)

![YouTube's video page before and after Seiza tames it: the recommendations and Shorts are gone, and the video and its comments stay.](.github/readme/tamed.png)

> **Status:** 0.2.1. The app updates itself from 0.2.0 on. The desktop app for macOS and Linux is on [Releases](https://github.com/seiza-app/seiza/releases/latest), and the browser extension is on the [Chrome Web Store](https://chromewebstore.google.com/detail/seiza/nencjadggelocffidegejpekamaeopjc) and [Firefox Add-ons](https://addons.mozilla.org/en-US/firefox/addon/seiza-app/). See [PLAN.md](PLAN.md) for the roadmap.

## Tame, don't just block

Most blockers treat a site as all or nothing. Seiza gives every site one of four levels, chosen per session profile:

| Level | What happens |
|---|---|
| **Allowed** | Nothing changes. |
| **Tamed** | The site still works, but the distracting parts are hidden: feeds, trending, suggested and promoted posts. Optionally grayscale. |
| **Redirected** | Visits go to a calm Seiza page first: focus for five minutes, read your if-then plan, or open one of your own links. If you still need the site once the wait is up, take a few minutes on it. |
| **Restricted** | Blocked, with no way through until the session ends. |

**Page rules** give part of a site its own level, like redirecting YouTube Shorts while the rest of YouTube is tamed, or allowing `reddit.com/r/writing` while the rest of Reddit is redirected.

What "tamed" means for a site is a **recipe**: a small JSON file of toggles, each mapped to CSS selectors. There are built-in recipes for Reddit, YouTube, X, Instagram, TikTok, Facebook, LinkedIn, Pinterest, Twitch, Threads and Bluesky, and any other site can be added by its address. You can flip toggles from the extension popup for one session or save them to a profile, click away any other part of a page with **Hide parts of this page**, and keep a site tamed all the time, outside sessions too.

## The desktop app

The extension works on its own. The desktop app adds:

- **A night sky that fills with your focus.** Each session becomes a star in your profile's color, each week's stars form a constellation you name, and each month with focus grows a planet. Comets, meteor showers and auroras turn up now and then.
- **Satellites:** a distracting thought goes into orbit with Alt+P (Option+P on a Mac) from anywhere, and comes back when the session ends.
- **Apps too:** a profile can close or hide apps like Discord, Steam or Slack during its sessions.
- **Strictness:** Gentle sessions can end any time, Medium ones ask you to type a random string first, and Hard ones last until the timer runs out, with a few emergency passes a week.
- **A quiet timer** that can end without a sound, and sessions that end by themselves when you've walked away.
- **A weekly report**, and backups you can bring to another computer.
- **Blocking without the extension** (macOS, turned on in Settings): restricted sites are shut for every browser and app during a session.
- **Sync between your computers**, through a server you run yourself (below).

Seiza has no accounts and no telemetry. Everything stays on your device unless you turn on sync, and then it's encrypted before it leaves. See [PRIVACY.md](PRIVACY.md).

## Sync between your computers

Optional, and off until you set it up. Your finished sessions and their goals, the session running now, visits to redirected sites, constellation names, profiles and each computer's name reach every computer in your sync group, so each one draws the same sky, streaks and weekly report, and a session started on one runs on all of them. Settings stay per computer.

- **End-to-end encrypted.** The app encrypts everything before it's sent, with a key the server never gets. The server sees which computers are in a group, when they sync or are connected, how much they store and the IP addresses they connect from, and nothing else.
- **No accounts.** Your first computer starts a group, and each other computer joins with a short one-time pairing code shown on one already in it.
- **Your own server.** Seiza has no sync server of its own. You run the open-source one (one small program and one SQLite file), or use one a friend runs. Every computer keeps the whole log, so losing the server loses nothing.

### Running your own sync server

The full guide, with every setting, command and the API, is in [apps/sync-server/README.md](apps/sync-server/README.md). In short:

1. **Run it.** On any Linux machine with Docker, such as a Proxmox LXC or a Raspberry Pi, copy [`compose.example.yml`](apps/sync-server/compose.example.yml) into a folder as `compose.yml` and run `docker compose up -d`. The image is `ghcr.io/seiza-app/seiza-sync`, a few megabytes, for amd64 and arm64. It listens on port 8080 and uses about 5 MB of memory.
2. **Put it behind HTTPS.** The app only uses plain `http://` on your own computer or local network. To sync from anywhere, give it an address over HTTPS, for example with a Cloudflare Tunnel (the guide walks through it) or any reverse proxy.
3. **Decide who can start a group.** By default the server takes exactly one group, the first one started, and then closes, so a personal server needs no setup. To share it with friends, set `SEIZA_SYNC_NEW_GROUPS=invites` and give each of them a code from `docker compose exec seiza-sync seiza-sync invite`.
4. **Connect your computers.** On the first one, open Settings › Sync, type the server's address under **Use your own server**, and choose **Start a new sync group**. On that computer, choose **Add a device** for a pairing code; on the next one, enter the same address, choose **Join with a pairing code** and type it in. The first computer then asks whether to add it.

To update, run `docker compose pull && docker compose up -d`.

## Install

- **Desktop app:** download it from the [latest release](https://github.com/seiza-app/seiza/releases/latest): a `.dmg` for macOS, or a `.deb` or `.AppImage` for Linux. On macOS, it isn't signed with a paid Apple Developer account, so macOS asks you to allow it once: open it, click **Done**, then **System Settings › Privacy & Security › Open Anyway**. The release notes walk through it.
- **Browser extension:** from the [Chrome Web Store](https://chromewebstore.google.com/detail/seiza/nencjadggelocffidegejpekamaeopjc) for Chrome, Brave, Edge and Arc, or [Firefox Add-ons](https://addons.mozilla.org/en-US/firefox/addon/seiza-app/) for Firefox and Zen.
- **Sync server (optional):** see [Running your own sync server](#running-your-own-sync-server).

The app and the extension each work on their own. Install both and they link up by themselves, in every browser you use it in.

Windows isn't supported yet. On Linux, the idle check needs GNOME, and blocking without the extension is macOS only for now.

## What's here

```
apps/
  extension/     Browser extension (WXT + Vue). Chrome, Brave, Edge, Arc, Firefox, Zen.
  desktop/       Desktop app (Tauri 2 + Vue). macOS and Linux.
  sync-server/   Self-hosted sync server (Rust, axum + SQLite, one Docker image).
  website/       Product website (Nuxt, static, on Cloudflare Workers).
packages/
  core/          Shared types and logic: matching, recipes, profiles, sessions, the sky.
  recipes/       Built-in site recipes.
  quotes/        Bundled public-domain quotes for the app's Sky.
prototypes/      Design mock-ups the app and website were built from.
PLAN.md          The roadmap and the reasoning behind each feature.
```

Each app has its own README on how it works inside: [extension](apps/extension/README.md), [desktop](apps/desktop/README.md), [sync server](apps/sync-server/README.md) and [website](apps/website/README.md).

How the parts talk to each other:

- **Extension ↔ desktop app:** over the browser's Native Messaging, which never leaves the computer. The app installs the bridge for each browser it finds, and both keep profiles, the running session, site settings and visits to redirected sites in step, the newest change winning.
- **Desktop app ↔ sync server:** HTTPS, only once sync is set up. Records are compressed, then sealed with the group's key (XChaCha20-Poly1305), and sent in batches; the key reaches a new computer through a SPAKE2 pairing the server only relays.
- **Everything else stays put:** the extension makes no network requests, and the app's only other one is a daily update check to GitHub.

## Getting started

You'll need Node 22 or newer and pnpm (`corepack enable` sets it up).

```bash
pnpm install
pnpm dev:extension
```

`pnpm dev:extension` opens a browser with the extension loaded and reloads it as you edit. Use `pnpm dev:extension:firefox` for Firefox. To load a build by hand instead, see [apps/extension/README.md](apps/extension/README.md). The store listings' text, zips and screenshots are in [apps/extension/STORE.md](apps/extension/STORE.md).

Other useful commands:

```bash
pnpm test          # unit tests
pnpm test:e2e      # browser tests for the extension (see apps/extension/README.md)
pnpm typecheck
pnpm lint          # ESLint
pnpm format        # Prettier (pnpm format:check only reports)
pnpm build
```

The desktop app also needs the [Rust toolchain](https://rustup.rs) and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Run it with `pnpm dev:desktop`, and its Rust tests with `cargo test` in `apps/desktop/src-tauri`.

The website runs with `pnpm dev:website`.

The sync server is plain Rust. To try the app against one locally:

```bash
cd apps/sync-server
cargo run -- serve    # listens on 0.0.0.0:8080, database in ./seiza-sync.db
```

Then use `http://localhost:8080` as the address in the app's Settings › Sync. Its tests are `cargo test` in the same folder.

## CI and releases

CI ([`.github/workflows`](.github/workflows)) checks formatting, lint, types and tests for every part, runs the extension's browser tests, checks dependency licenses and advisories, and builds the sync server's Docker image.

- **Desktop app:** raise the version in `apps/desktop/src-tauri/tauri.conf.json`, `Cargo.toml` and `apps/desktop/package.json`, then push a matching tag like `v0.2.0`. That builds the macOS and Linux apps onto a draft release ([`release.yml`](.github/workflows/release.yml)), which is published by hand. Installed apps from 0.2.0 on find it within a day and offer to update.
- **Sync server:** every change merged to `main` publishes `ghcr.io/seiza-app/seiza-sync:latest` (and `:sha-<commit>`) once its checks pass ([`sync-server.yml`](.github/workflows/sync-server.yml)).
- **Extension:** `pnpm --filter @lock-in/extension zip` and `zip:firefox` build the store zips; [apps/extension/STORE.md](apps/extension/STORE.md) has the rest.
- **Website:** deployed by Cloudflare Workers Builds whenever `main` changes.

Dependencies are only taken once they've been out for at least two weeks (`minimumReleaseAge` in `pnpm-workspace.yaml`, and Dependabot's cooldown), which gives a bad release time to be caught.

## Contributing

Recipes break when sites change their HTML, so fixes to the selectors in `packages/recipes` are especially welcome. Each recipe is plain JSON, and `pnpm test` checks that every selector is safe to inject.

Before opening a pull request, run `pnpm format`, `pnpm lint` and `pnpm test`; CI checks all three. For Rust changes, also run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` in the crate you changed.

Bugs and ideas go to [the issues](https://github.com/seiza-app/seiza/issues).

## License

[AGPL-3.0](LICENSE). You're free to use, study, change and share Seiza. If you run a modified version for other people, including as a hosted service (a sync server you've changed, say), you must share your changes under the same license.
