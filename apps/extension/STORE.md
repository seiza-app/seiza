# Store listings

What to paste into the Chrome Web Store and Firefox Add-ons forms, kept here so the listings change with the code. The privacy policy both stores ask for is [PRIVACY.md](../../PRIVACY.md), linked as <https://getseiza.app/privacy> once the website is live (until then, <https://github.com/seiza-app/seiza/blob/main/PRIVACY.md>).

## Building the zips

From the repo root:

```bash
pnpm --filter @lock-in/extension zip          # Chrome Web Store
pnpm --filter @lock-in/extension zip:firefox  # Firefox Add-ons, with the sources zip
```

They land in `apps/extension/.output/`:

- `seiza-<version>-chrome.zip`: for the Chrome Web Store. It's built without the manifest's `key` (the store won't take one), into `.output/chrome-mv3-store`, so a copy loaded unpacked from `.output/chrome-mv3` keeps its id.
- `seiza-<version>-firefox.zip`: for Firefox Add-ons.
- `seiza-<version>-sources.zip`: Firefox Add-ons asks for it with the Firefox zip, since the code is bundled. `SOURCES.md` inside says how to build it.

Raise `version` in `package.json` before each upload; both stores refuse a version they've already had.

## Name and summary

**Name:** Seiza

**Summary** (Chrome's 132-character limit fits both):

> Tame distracting sites instead of blocking them: hide feeds and Shorts, redirect with a pause, or block, during focus sessions.

## Description

> Seiza is a free, open-source focus app. Most blockers treat a site as all or nothing. Seiza gives each site one of four levels, chosen per profile:
>
> • Allowed: nothing changes.
> • Tamed: the site still works, but the distracting parts are hidden: feeds, Shorts and Reels, trending, recommendations. Grayscale too, if you like.
> • Redirected: visits go to a calm Seiza page first. Focus for five minutes, read your own if-then plan, or open one of your links. If you still need the site, take a few minutes on it.
> • Restricted: blocked until the session ends.
>
> Built-in recipes tame Reddit, YouTube, X, Instagram, TikTok, Facebook, LinkedIn, Pinterest, Twitch, Threads and Bluesky. Add any other site by its address, and click away any part of a page you'd rather not see.
>
> Page rules give part of a site its own level, like restricting YouTube Shorts while the rest of YouTube is tamed. Sites can also stay tamed all the time, outside sessions.
>
> Seiza works on its own. With the Seiza desktop app installed, the two stay in step, and your focus time fills a night sky with stars.
>
> No accounts, no analytics, no ads. Nothing leaves your computer. The source code is public: https://github.com/seiza-app/seiza

## Chrome Web Store

**Category:** Productivity › Workflow & Planning (or Tools, if that's what the form offers)

**Single purpose:**

> Seiza helps people focus by changing how distracting websites behave during focus sessions: hiding distracting parts of a site, sending visits to a pause page first, or blocking the site, depending on the level the person chooses.

**Permission justifications:**

| Permission | Justification |
|---|---|
| `storage` | Keeps the person's profiles, the current focus session, site settings and visits to redirected sites in the browser. |
| `activeTab` | When the person opens the popup, it reads the current tab's address to show that site's level and toggles. |
| `alarms` | Ends a site's few-minute pass on time, so the site is redirected again, even while the background worker is asleep. |
| `declarativeNetRequestWithHostAccess` | Sends visits to redirected or restricted sites to the extension's own pause page during a session, using the browser's built-in redirect rules, limited to sites the extension has access to. |
| `nativeMessaging` | Links to the Seiza desktop app on the same computer, when it's installed, so both share profiles and the current session. Nothing is sent anywhere else. |
| `scripting` | Runs the taming stylesheet on sites the person adds themselves, after they grant access to that one site, and starts the "Hide parts of this page" picker on the current tab. |
| Host permissions (the built-in sites) | Hides distracting parts of Reddit, YouTube, X, Instagram, TikTok, Facebook, LinkedIn, Pinterest, Twitch, Threads and Bluesky during a session, and redirects visits to them. |
| Optional host permission (all sites) | Asked for one site at a time, only when the person adds that site to Seiza. Asked for all sites at once only if the person chooses to keep away every site they haven't listed during a session (a profile's "Every other site"), so Seiza can redirect those visits. |

**Remote code:** No. All code is in the package.

**Data usage:** Seiza doesn't collect or transmit any of the listed kinds of data. Everything stays in the browser, or goes to the Seiza desktop app on the same computer over Native Messaging. Tick the three certifications (no selling, no unrelated use, no creditworthiness).

**Screenshots and promo tile:** `pnpm --filter @lock-in/extension screenshots` makes five 1280×800 screenshots and the 440×280 promo tile in `apps/extension/.output/store-screenshots`, as PNGs without transparency. They're real captures of the extension (`store/screenshots.ts`), so run it again when the pages change. Firefox Add-ons takes the same screenshots.

## Firefox Add-ons

**Categories:** Productivity (or Privacy & Security / Social & Communication, up to two in each)

**License:** GNU Affero General Public License v3.0

**Support site:** https://github.com/seiza-app/seiza/issues

**Homepage:** https://getseiza.app (until the website is live, https://github.com/seiza-app/seiza)

**Data collection:** none (declared in the manifest as `data_collection_permissions: { required: ["none"] }`).

**Notes to reviewer:**

> Build instructions are in SOURCES.md at the root of the sources zip: `pnpm install --frozen-lockfile`, then `pnpm --filter @lock-in/extension build:firefox`; the output is apps/extension/.output/firefox-mv2. The innerHTML warning from web-ext lint is in Vue's runtime chunk, not Seiza's code. nativeMessaging links to the optional Seiza desktop app on the same computer (host name io.github.lock_in_focus_app.lock_in); without it, the extension works on its own.
