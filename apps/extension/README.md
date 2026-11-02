# Seiza — browser extension

Tames, blocks, or redirects distracting sites during focus sessions. It works on its own, and links to the desktop app when that's installed.

## Develop

From the repo root:

```bash
pnpm dev:extension          # Chrome-family, with hot reload
pnpm dev:extension:firefox  # Firefox-family
```

Or build and load it by hand:

1. `pnpm --filter @lock-in/extension build`
2. Open `chrome://extensions`, turn on **Developer mode**, click **Load unpacked**, and pick `apps/extension/.output/chrome-mv3`.
3. After each new build, click the extension's reload button on `chrome://extensions`. Chrome keeps running the old background script until you do, even across restarts; the popup and other pages pick up the new build straight away, so a half-updated extension can look fine at first.

The icon is drawn in `icon/icon.svg` (48px and up) and `icon/icon-small.svg` (the 16 and 32px toolbar sizes). After changing either, run `pnpm --filter @lock-in/extension icons` to render the PNGs in `public/icon/`.

## Test

```bash
pnpm test:e2e
```

This builds the extension and runs the browser tests in `e2e/` against Chrome for Testing. The first time, download that browser with `pnpm --filter @lock-in/extension exec puppeteer browsers install chrome`.

`e2e/desktop.e2e.ts` links the extension to a fake desktop app (`e2e/fakeDesktopApp.ts`), registered as the Native Messaging host in the test browser's profile folder.

The tests never touch the real sites. They load a copy of the build with its site patterns rewritten to `.test` domains (`www.reddit.test`, `x.test`), which Chrome resolves to a local server serving the pages in `e2e/fixtures/`.

## How it works

- `utils/state.ts`: the active session, profiles, redirect visits and the redirect page's running waits, kept in `storage.local` and shared by every part of the extension.
- `utils/actions.ts`: every change to that state goes through here.
- `utils/desktopLink.ts`: the link to the desktop app over Native Messaging. While linked, the profiles and the session stay in step with the app (and every other linked browser): each side sends its changes, stamped with when they were made, and the newest wins. Gate visits go to the app's log. Without the app it does nothing. The extension's ids are fixed (a public `key` in the Chrome manifest, a gecko id for Firefox) so the app knows which extension may connect.
- `entrypoints/popup/`: start and end sessions, set the current site's level for a profile between sessions, and change its toggles for this session or for a profile.
- `entrypoints/options/`: the settings page. Create, rename, duplicate, reorder and delete profiles; set each site to Allowed, Tamed, Redirected or Restricted, and every site the profile doesn't list to Allowed, Redirected or Restricted (an allowlist); and choose how redirected sites behave: the wait, your if-then plan, passes and your links.
- `entrypoints/content.ts`: runs on recipe sites at `document_start` and injects the recipe CSS for the active session. It updates live when a session starts or ends. It also catches single-page apps moving onto a redirected or restricted page (e.g. clicking a Short on YouTube), which the browser's redirect rules can't see. On redirected sites it also handles tabs that were already open, or whose pass ran out: it pauses them with an overlay (`utils/pauseOverlay.ts`) or sends them back, depending on the profile.
- `entrypoints/background.ts` and `utils/redirects.ts`: turn the session's *Redirected* and *Restricted* sites into `declarativeNetRequest` rules, so the browser itself sends those visits to the redirect page with no extension code running per request. The rules are rebuilt when the session or profiles change and when a pass runs out. A profile that keeps every other site away gets one rule for every address under its own, and tabs already open on unlisted sites (which have no content script) are sent to the redirect page from here. The redirect page is web-accessible from every site (Chrome only redirects to it if the page a link was on may load it), with `use_dynamic_url` so websites can't load it to detect Seiza.
- `entrypoints/redirect/`: where redirected visits land. It's one page that always looks the same: today's attempts, "Focus for 5 minutes first" with the time left (it keeps counting if you leave), the profile's if-then plan, your links, the smallest next step when you answer "Stuck", then "Not now" or a pass to that one site once the wait is up, and "what brought you here?". Restricted sites get a page with no way through.
- Recipes live in `packages/recipes`. While a site is tamed, the content script keeps the page's path in `data-lock-in-path` on `<html>`, so recipe selectors can target some pages only (e.g. `html[data-lock-in-path^="/explore"] main`). The logic for matching, CSS, redirects, passes and settings lives in `packages/core`.
