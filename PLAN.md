# Seiza — Product & Build Plan

> **Status:** Phases 1–4 built and most of Phase 5: the extension and the macOS desktop app both work, with recipes for eleven sites. Next is the first release (Phase 5's last item), which still needs each recipe checked on the real sites while logged in.

## Context

Existing focus apps each get part of the way there:
- **Freedom**: good allow/block control, but it's heavy, and key features sit behind a paywall that may keep growing.
- **Focus Friend**: a fun reward loop, but it's phone-only and blocks everything.
- **SocialFocus**: hides distracting parts of social sites, but you have to toggle it by hand every time.

**Seiza** is a free-forever, open-source (**AGPL-3.0**) desktop focus app plus browser extension. It lets you:
- block, *tame*, or allow each site per session
- redirect distractions somewhere useful
- park distracting thoughts for later
- reward focus with a night sky that fills with stars

**Name:** Seiza (星座) is Japanese for *constellation*, and sounds the same as 正座 *seiza*, the calm kneeling posture of meditation and tea ceremony. The app was first called Lock In (renamed October 2026). "Lock in" stays as the verb for starting a session, and hidden IDs keep the old name so installs, backups and store listings keep working: the bundle ID and data folder, the browser link's name, the Firefox add-on ID, the hosts file's markers, the backup file's `lockIn` key, and the extension's element and message names.

**Theme:** social media is *light pollution*. When you lock in, the noise fades and you can finally see the stars. The app, the reward system and the website all share this theme.

**Principles:**
- Lightweight and battery-friendly
- Private: no account, no telemetry, local-first
- Every default is editable

**Out of scope for now:** mobile apps. Revisit later; iOS blocking requires a paid Apple Developer account plus Apple's Family Controls approval, and AltStore/free signing can't use those APIs.

---

## 1. Core idea: "Tame, don't just block"

Each site has one of four levels, set per session profile:

| Level | What happens | Example |
|---|---|---|
| **Allowed** | Normal | Google Docs, Wikipedia |
| **Tamed** | The site works, but feeds, recommendations and Shorts are hidden; can be grayscale | Reddit threads and search work, the home feed is gone |
| **Redirected** | Sent to the redirect page (§3), with a wait before the way through | x.com → "Focus for 5 minutes first" |
| **Restricted** | Blocked outright: no countdown, no pass, no way through until the session ends | Reddit during an exam-prep session |

**Page rules** give part of a site its own level (e.g. `reddit.com/r/writing` allowed while the rest of Reddit is redirected, or YouTube tamed with `/shorts` redirected).
- **Matching:** `/` is the home page only; any other path also covers everything under it (`/r/writing` includes its posts, but not `/r/writingprompts`). `*` matches within one part of the path. Matching ignores case, and the most specific (longest) rule wins.
- **Why it matters for video:** a recipe can hide the links to Shorts and Reels, but hiding a playing video with CSS doesn't stop its sound. Redirecting `/shorts`, `/reels` or TikTok's For You page stops it properly.
- **Two ways in:** full page loads are caught by the browser's redirect rules (an allow rule ranks above a site redirect, longer paths first). Single-page apps like YouTube change pages without loading one, so the content script also checks each in-site navigation, using the event the browser fires on URL changes. It costs one comparison per click, only on recipe sites, with no polling.
- **Suggestions:** each recipe lists common pages (Shorts, Reels, Explore, For You…) offered as one-click "Redirect" buttons in settings while the rest of the site is open.

### Site Recipes (JSON) + live visual editor
A **recipe** defines what "tamed" means for one site: a list of toggles, each mapped to CSS selectors and effects. Built-in recipes cover Reddit, YouTube, X, Instagram, TikTok, Facebook, LinkedIn, Pinterest, Twitch, Threads and Bluesky. Users can edit them or create their own, and the community can contribute fixes when sites change their HTML.

Any other site can be added to a profile by its address ("site:example.com"), from the extension's settings, its popup or the app's Profiles › Sites. With no recipe there's nothing to hide, so an added site is allowed, redirected or restricted. The extension asks the browser for each added site as it's added (an optional host permission for that site alone), so it never asks to read every site; one added in the app shows "Let Seiza reach it" in the extension until the browser says yes. During a session the popup offers to keep the site you're on away (Redirect it, Restrict it), which is saved to the profile. While a Hard session runs, sites can only get stricter, in the app and the extension alike.

```json
{
  "id": "x",
  "name": "X",
  "matches": ["*://*.x.com/*", "*://*.twitter.com/*"],
  "source": "Where the selectors came from, with the license",
  "toggles": [
    { "id": "hideHomeTimeline", "label": "Hide home timeline", "selectors": ["html[data-lock-in-path=\"/home\"] [data-testid=\"primaryColumn\"] section > h1 + div[aria-label]"], "default": true },
    { "id": "hideWhoToFollow", "label": "Hide who to follow", "selectors": ["[data-testid=\"sidebarColumn\"] aside[role=\"complementary\"]"], "default": true },
    { "id": "grayscale", "label": "Grayscale", "effect": "grayscale", "default": false }
  ]
}
```

**How recipes are written:**
- **Plain CSS only.** Recipes are injected as one stylesheet at `document_start`, so there's no flash and no script running per element. Selectors are checked so they can't break out of the stylesheet.
- **Page-specific hiding.** CSS can't see the URL, so while a site is tamed the extension keeps the page's path on `<html data-lock-in-path="…">` and updates it as single-page apps navigate. A selector like `html[data-lock-in-path^="/explore"] main` hides something on some pages only.
- **Prefer stable hooks.** Use test ids, roles, `href`s and custom element names over generated class names (Facebook and Instagram change theirs constantly). Some selectors still rely on English labels (`aria-label="Explore"`), which is a known gap for other languages.
- **Credit sources.** Selectors adapted from open projects (UBlock-Filters-for-Social-Media, Control Panel for Twitter and Declutter Pinterest, all MIT; News Feed Eradicator, AGPL-3.0 like Seiza) are credited in the recipe's `source`. TikTok has no open list to draw on, so its recipe uses TikTok's own `data-e2e` test attributes and links. Bluesky's app is open source (MIT), so its recipe uses the app's own test ids; Threads has no stable hooks, so its recipe sticks to one landmark (the feed column's region label).
- **Tested twice.** Browser tests check each recipe against a stand-in page for the site. Sites change their HTML without notice, so each recipe also needs checking on the real site before a release.

**The editor shows a version of the actual site and animates every toggle:**
- **Preview mock**: each built-in recipe ships with a lightweight, stylized wireframe of the site, built as a Vue component. It is *our own drawing*, not copied site assets. It sits above the site's toggles in Settings.
  - Each region of the mock is tagged with the toggle id it represents (`<MockRegion toggle="hideShorts">`).
  - Flipping a toggle makes that region collapse with a quick, bouncy spring (scale + height + fade), and the rest of the page reflows smoothly. The height animates through `grid-template-rows: 1fr → 0fr`, so the browser does the reflow and no FLIP script is needed.
  - Pointing at (or focusing) a toggle outlines its region. If the region is already hidden, it comes back as a faint dashed ghost, so you can see what the toggle takes away.
  - Sites can have several pages (YouTube: Home and Video); pointing at a toggle that only matters on another page switches to it.
  - Grayscale cross-fades the whole mock.
  - Feedback is instant, with no page loads.
  - Every built-in site has one (Pinterest also has a Pin page). A browser test fails if a recipe gains a toggle that isn't drawn. Custom recipes without a drawing just show their toggles.
- **Live mode** (for custom recipes and sites without a mock): the extension opens the real site with a small floating panel. Toggles apply to the real page instantly with the same animation. An **element picker**, like uBlock's "zapper", lets you click something to hide it, and it keeps the selector on the profile's site rule for you (built: the popup's "Hide parts of this page"; the toggles in the panel are still to do).
- **Animation implementation:** native CSS spring easing (`linear()` easing curves) and CSS transitions, with no animation library, to stay lightweight. Animations are disabled under `prefers-reduced-motion`.

### Session Profiles (fully user-managed)
A profile bundles:
- site levels + recipe toggle overrides
- app rules
- strictness
- timer settings
- the redirect page's settings (wait, if-then plan, pass, links)

You can create, edit, duplicate, delete and reorder profiles, and take them to another computer in the app's backup file (§7). The app ships with **starter profiles that show what's possible**. They're ordinary editable profiles, and "Reset starter profiles" restores them:
- *Writing*: Reddit and YouTube tamed + grayscale (YouTube Shorts redirected); X, Instagram, TikTok and Facebook redirected; Discord closed
- *Deep Work*: all social redirected (five minutes first), chat apps closed
- *Wind Down*: Reddit, YouTube and Instagram tamed (no feeds or recommendations; Shorts and Reels redirected), nothing else

---

## 2. Sessions & timer

### Strictness (chosen per profile, adjustable per session)
- **Gentle**: a pause screen, then you can continue
- **Medium**: to end early, type a random string (Cold Turkey-style)
- **Hard**: can't end before the timer runs out. You get **emergency passes** instead (default 2 per week, configurable).

### The "quiet timer" (Pomodoro that doesn't cut you off)
The timer is a *starting ramp*, not a cage.
- **When it ends, it can stay silent.** The end-of-timer behavior is per profile:
  - *Silent* (default for Writing): **nothing happens at all**. No notification, sound, or menu-bar change.
  - *Gentle*: soft notification
  - *Alert*: sound + notification
- **"Show time since done" setting** (off by default): when on, a finished timer shows "Done · 37 min ago" and keeps counting up, so you can see how much extra you did. When off, it simply shows "Done".
- **Secret overtime bonus:** overtime is always tracked, whichever setting you choose. Minutes after the timer ends count **1.5×**. This is never announced. It shows up only as that session's star twinkling a little brighter in your sky and as extra stardust (§5), a little discovery for later.
- **Anti-gaming:** a session ends when you stop it, or, if you turn it on in Settings, automatically after N minutes of system idle (no keyboard or mouse; off by default, §9). The idle tail isn't counted, so a forgotten timer doesn't farm overtime.

---

## 3. The redirect page (handled by the extension)

When a site is *Redirected*, the extension intercepts the navigation and sends you to **one Seiza page that always looks the same**, so you know what to expect. (It used to pick one of four pages at random: an intention gate, a quote card, a link and an "I'm stuck" page. They overlapped, and three of them were different ways of saying "go back to work" that all led to the gate, so they were folded into this one page in Phase 5.) The page is bundled inside the extension, so it loads instantly and works offline. *Restricted* sites get a plainer page instead: "Reddit is off for now", with no way through until the session ends.

Links to a redirected site can come from anywhere (a search result, a chat), and Chrome only redirects to an extension page if the page the visit came from may load it. So the redirect page is open to every site, with `use_dynamic_url` so websites can't load it directly to detect Seiza.

**What the page shows, top to bottom:**
- The **attempt counter**: "6th try at Reddit today".
- **Five minutes first**: "Focus for 5 minutes. If you still want Reddit after that, go right ahead," with the time left counting down. It leans on two things: the hard part is usually starting, and a pull that comes from a moment of boredom tends to pass once you're back into the work. (Why: in a study of the one sec app, a short wait and an easy way to back out both cut how often people opened an app, while the message alone did nothing. The "five minutes to get started" part is reasoning, not tested; §12.)
- The profile's **if-then plan**, if it has one: "When I want Reddit, I'll write one paragraph first." Without one, a quiet "Add an if-then plan" link sits in the same spot and opens the settings. (Quotes used to fill it, but they were more distracting than helpful there, so the extension shows no quotes now; they live on the app's Sky.)
- **Ways back to work**: a button for each of your own links (your draft, to-do list, Anki or reading list).
- **When you're stuck**: answering "Stuck" to "why?" asks for the **smallest next step**, something you could finish in five minutes, right under the clock, and shows it back as "Next step". It replaces a separate 2-minute "I'm stuck" page (say it out loud, shrink it, stand up, breathe): the five minutes already are the time to get going, and shrinking the task was the part that helps. The step isn't kept.
- **Not now**, which goes back to where you were (or closes the tab). Like simply never going on to the site, it counts as going back to focus, so it can bring a comet (§5).
- When the wait is up, **Go to Reddit**, which opens the site with a time-boxed pass to *that site only*.
- The one-tap **"why?"**: bored / stuck / tired / curious.

**Settings, per profile:**
- **Wait**: *fixed* (5 minutes by default; a few seconds works too, for a short pause) or *longer each try* (starts short and grows with each attempt at that site today, capped). The page words itself to fit: "Focus for 5 minutes" or "Focus for 10 seconds".
- **The wait keeps counting if you leave the page**, since the point is to go and work. Coming back to the site picks the same countdown back up, and once it's done the site stays a click away for as long as the wait was (5 more minutes). After that, a new trip starts a new wait. Going on to the site ends it.
- **If-then plan**: one optional line, with a line explaining what an if-then plan is (decide ahead of time what you'll do when the urge comes; it's one of the best-tested habit techniques, §12) and an example. Set in the extension's settings or the app's Profiles › Plan; the two stay in step.
- **Require a reason**: off by default. When on, "What did you come here for?" has to be answered before Go to Reddit. Typed reasons are listed in the weekly report (§6); the "why?" taps are what get charted.
- **Pass length** (default 5 min), and **when the pass runs out**:
  - *Nothing*: the tab stays as it is.
  - *Pause* (default): the tab is covered by a dim overlay with a "Still need this?" button, which leads back to the redirect page. Reference tabs stay where they are.
  - *Auto-redirect*: the tab is sent back to the redirect page.
- **Your links**, shown as buttons on the page.

Every trip is logged the same way (site, time, attempt #, why-tap, typed reason, whether you went on to the site) for the weekly report and comets.

**Quotes & privacy:** quotes show only in the app, as the Sky's quote for the day. They come only from the bundled pack and your own; nothing is fetched. The bundled pack is public domain only (texts and translations published before 1931), with each quote's source noted in `packages/quotes` so it can be checked. A daily fetch from a public quote API was considered and dropped: it meant a network request, a third party's terms and adding TLS to the app. Your own quotes are added in the app's Settings. The extension has no quotes at all: it used to show them and sync your own with the app, and both were taken out in Phase 5. Redirect events (site, time, attempt #, why-tap, intention text) are sent to the desktop app over Native Messaging for stats and insights.

---

## 4. Satellites (thoughts for later)

**Problem:** mid-focus thoughts like "I should check this real quick" or "don't want to forget this" turn into distractions.

**Satellites** let you launch the thought into orbit, and it comes back when you're done.

To make the name instantly clear:
- It's always shown with its subtitle: **"Satellites · thoughts for later"**.
- The input placeholder reads *"Distracting thought? Launch it — it'll come back after your session."*
- The new-user tour says what satellites are. The launch box keeps a tiny satellite circling a star as its mark, and the Alt+P hotkey beside Launch as a reminder.

**Capture from anywhere:**
- a global hotkey, **Alt+P** (Option+P on a Mac), which opens a tiny spotlight-style box. Return launches the thought and hands focus straight back to the app you were in; Escape or clicking elsewhere closes it. (Option+P normally types π, which Seiza takes over while it's running. The hotkey can be changed or turned off in Settings › Satellites: it needs Option, or two of Control, Shift and Command, so it doesn't take other apps' shortcuts, and a few the system uses are turned away.)
- the menu-bar icon
- the extension popup
- every redirect page

**Return:** nothing pops up when a session ends, so it never pulls you out of the flow (the same idea as the quiet timer). The satellites wait for you: the window shows "3 satellites came back" above the profiles, and the menu bar has a quiet "3 satellites came back…" line. Review them as a checklist:
- **Done:** ticked off.
- **Snooze:** back into orbit; it comes back when the next session ends.
- **Task:** onto the task list.
- **Delete:** gone. Its text is erased; only the fact that one was parked is kept, for insights.

**Between sessions**, a launched thought shows on the dashboard as "in orbit" and comes back when the next session ends. During a session, the window only shows how many are in orbit, not what they say.

**The task list** is a checklist on the dashboard, kept in the app, as long as you like. Ticked-off tasks stay underneath for 24 hours ("Done · last 24 hours"), so one ticked by accident can be unticked; after that they drop off the list (they're still kept for insights). Later: export to Reminders, Todoist and the like.

**Only in the desktop app.** Satellites used to be in the extension too (the popup and redirect pages had the launch box, handed over to the app when linked), but that was taken out as out of scope: the app's Alt+P box works from anywhere, browser included, so there's one place to launch and one list to review. An older extension's handover messages are ignored.

**The look (with the UI redesign in Phase 5):** dreamy and slow. Tiny satellites drift around the UI in lazy orbits among faint, twinkling stars, e.g. around the launch box and the task list, and a launched thought visibly lifts off and joins them. It should feel calm, never busy: slow loops of tens of seconds, soft glows, nothing that flickers or pulls the eye during a session. It's drawn with CSS or a small canvas (no animation library) and stays still with reduced motion.

Satellites also count in insights, e.g. "You parked 12 thoughts this week instead of acting on them."

---

## 5. Rewards: the night sky

One big sky that keeps growing as you focus, a bit like Obsidian's graph view: you never start a new page, your constellations just build outward. Everything in it is earned by focusing, drawn by code and kept on your device.

### Stars
- **Every session adds a star, and every star twinkles.** Longer sessions make bigger, brighter stars, and overtime makes a star twinkle a little brighter still. That's the secret overtime bonus (§2), never announced.
- **Color comes from the profile.** Each profile has a star color, e.g. warm gold for Writing and blue-white for Deep Work, so the sky shows at a glance how you spend your time. The starter profiles (and profiles that already exist when the sky ships) get default colors from a calm palette. Creating a profile includes picking its star color, wherever profiles are created (the extension's settings today), and it can be changed later.
- **Short sessions:** a star's size and brightness follow its minutes, so a 5-minute session is a faint speck next to a 90-minute one. Stardust and the chance of rare events count focused *minutes*, not sessions, so splitting one session into many earns nothing extra.
- **Back-to-back sessions share a star.** A session under 30 minutes that starts within 10 minutes of the last one ending joins that star, which grows to match, as long as both are for the same profile (so a star keeps one color). Sessions of 30 minutes or more always get their own star.
- **The smallest star is 5 minutes.** A lone session shorter than that (often an accidental start) leaves no star, though its minutes still count toward stardust.
- **Missed days cost nothing.** The sky never loses anything, so there's no streak guilt.
- **Streaks count days with a star**, in local time, the same way for moons, personal bests and the weekly report. The report's Focus card shows the streak running now, only while one is (it keeps going through the day after its last day, since that day isn't over), and the longest by the end of the week being shown, from 2 days. A streak is never shown as broken or lost.

### Constellations and the Milky Way
- **One constellation per week.** Each week's stars (weeks start on the computer's first day of the week, as in the weekly report) connect into that week's constellation. Each star sits in its day's slice of the week, left to right, at a height of its own, and the fewest, shortest lines join them, so each constellation's shape is unique to you. A week with gaps is still a constellation: missed days never break anything.
- **Naming:** every constellation starts with a made-up name like "The Quiet Heron" (never the same as another), and you can rename it from the weekly report, under the week's title. A blank name goes back to the made-up one. Only names are saved; the shapes are worked out from the session log.
- **Two layouts, with an animated move between them.** In the Sky view, constellations pack in close, slightly overlapping and zoomed in, so the sky reads as one starry collection: the newest in the middle and older ones spiralling out, fitted above the cards. The Sky shows a month's constellations at a time (a week belongs to the month its middle falls in), the latest unless you step back with the small ‹ › arrows under the sky, which fade from one month to the next; it's back on the latest when you leave the Sky or start a session, and the arrows hide during one. The other months fold away into the Milky Way. Stepped back to a month, the Sky shows that month's planet with the few before it, and that month's comets. In Explore they all spread out into separate constellations that fill the sky, starting on the latest 12 weeks, and you can drag the sky around and zoom out to find older ones further out, as far as the first. In the weekly report they spread out and line up in week order, and the report shows that week's constellation softly behind its header and week title, like a hero image behind a page header.
- **Names on hover, in Explore.** The Sky page shows no names, so the sky stays a sky. In Explore, hovering a constellation makes it shine a little brighter and brings its name up beneath it.
- **The Milky Way** is a soft band of glow, dust lanes and specks across the sky, behind everything, that fills in slowly as your total focus hours grow: quickest at first, so the first hours show, and all there by 1,500 hours (years of focus). Light pollution washes it out first, as it does outside. It's also the zoomed-out view: from far away in Explore, a constellation's stars fold into a soft glow in their colors, and zoomed all the way out your whole history is a spiral of glowing weeks.

### Planets and moons
- **A planet for every month you focus at all.** It's generated from a seed, so no two people's planets look alike. A month without focus has no planet, and no gap where one would be. Like the stars, planets are worked out from the session log, so past months get theirs too.
- **Each month is a different kind of world**, picked by its seed and never the same kind two months running: a **gas giant** (bands with wavy edges and little eddies, later a storm), an **ice giant** (pale, faint bands, branching cracks like Europa's), a **rocky** world (craters, or bumps standing up from it), an **ocean** world (wispy clouds), a **lava** world (dark crust plates with lava glowing in the gaps and a glowing edge) or a **marble** world (dark, with ribbons of light swirling round it). The seed also picks its colors and how it leans. The look is soft and dreamy, never cartoon outlines.
- **Now and then, an unusual shape.** About one month in three, the seed picks a shape the planet grows into: a **star**, a **pumpkin** (ribs and a little stem), a faceted **gem**, a **peanut** (two halves joined, one a little bigger), a **flat disc** seen at an angle with a coin-like edge, a **ring world** (a donut, with moons passing through) or **two linked rings**. It comes through from 20 hours and is complete at 45, so a light month looks like a normal planet and a big one might turn out to be a star: a reason to keep focusing. Shapes and kinds combine (a lava gem, a marble peanut), and ring worlds and linked rings have no rings of their own. Nothing is saved for any of this; it all comes from the seed.
- **It takes shape through the month.** Size follows the month's focus time (full size at 60 hours), and its form changes as the hours add up, each new detail starting faint and filling in over 15 hours:
  - under 3 hours: a **protoplanet**, a small core in a swirling disk of dust
  - 3–10 hours: a **lumpy rock**, smoothing out as it goes
  - 10 hours: **round**, with its kind's surface (more of it every 4 hours)
  - 20 hours: its **first ring**, in a style the seed picks: one clean ring, several thin ones stacked, a wide faint halo, a broken ring of tiny moonlets, or a ring crackling with lightning (which only flickers while exploring); ice worlds may get a frosted ring with icicles hanging from it. Most rings are pale; some are a bright color or the month's top profile's color. An unusual shape starts coming through.
  - 30 hours: an **atmosphere**, with polar caps (ice, rocky and ocean worlds) or a storm (gas giants); some planets get a see-through **glass shell** instead
  - 45 hours: a **second ring crossing the first** at another angle, or more layers on stacked rings; an unusual shape is complete
  - 60 hours: **flattened a little at the poles** from spinning, with an **aurora** shimmering there

  The current month's planet changes as you focus; earlier months stay exactly as they ended.
- **Its glow is its own.** A soft glow in the planet's own color follows its shape (a star glows star-shaped). Some rings take the star color of the profile you focused on most that month, so a Writing-heavy September might wear a gold ring.
- **Where they sit.** Planets are nearer than the stars, around the edge of the constellations, the current month's largest and up to the right, older ones further round, smaller and fainter. The Sky shows the last 6 months' planets, still. Explore shows them all, slowly circling (a lap every 8 minutes), spinning so their surfaces turn, with their moons going round, and hovering one shows its month and focus time beneath it. In the weekly report, the week's month's planet sits faintly beside its constellation's name. Under reduced motion they never move.
- **Moons from streaks.** A streak of days with focus earns a moon on the day it reaches 3, 7 and 14 days in a row, then every week after, around that day's month's planet (at most 3 per planet). Nothing announces a moon; it's just there. A missed day starts a new streak but never takes a moon away. Moons vary in size and color, some icy, some cratered.
- **Kept light:** a planet that isn't moving is drawn once and reused as an image, so the Sky page stays cheap however detailed planets get; only Explore draws them fresh each frame.
- **Changing the looks later** redraws everyone's past planets, since they come from the seed. Once people have planets, new kinds, shapes and styles are added at the end of their lists (or the generator gets a version), so existing planets keep their look.
- **Your best month so far** has a faint pale-gold halo around its planet, once there are two months to compare.
- **Things that happen to a planet.** Now and then a rare event happens to the month's planet instead of the sky (see Rare events), and stays: a **moon crashes into it**, leaving a glowing crater; a **chunk breaks off** and drifts beside it, its broken side glowing; or it **shatters** into a ring of debris circling it, with glowing cracks left on it. At most one a month, and only once the planet is round.

### Comets: resisting a distraction
- Leaving the redirect page with "Not now" (or just never going on to the site) can bring a comet: a 1 in 4 chance.
- **Don't reward the trip, reward the turn-around.** Only the first resist per site per day counts, and the daily chance doesn't grow with more visits, so going to a site just to resist it earns nothing. Comets stay rare enough to be a nice surprise, not a target. (General reasoning, not research: worth revisiting if it seems to pull people toward the sites.)
- A comet glides in the first time the Sky shows it, then stays faintly beside its week's constellation, its tails streaming away. The Sky shows this week's; Explore and the weekly report show each week's (the latest 3 a week).

### Rare events
- Each finished session rolls a very small chance of something special, for its minutes (about 2.5% an hour early in the day, so splitting a session earns nothing), rising to 4 times as likely by 4 hours of focus that day: a **shooting star** (most often), a **meteor shower**, an **aurora** across the top of the sky, or a **supernova** (rarest), which flares and leaves a small nebula in its week's constellation. About 1 in 4 happen to the month's planet instead (see Planets).
- **Personal bests always bring one** of the bigger ones: a new longest session (after the first 10) or the day a streak of days passes your longest yet (from 3 days).
- The roll uses a random secret kept on the device, so events can't be predicted or replayed. Each roll only looks at what came before it, so nothing that happened changes as the log grows.
- **Nothing pops up.** An event plays once, gently, the next time the Sky is shown between sessions (usually right as a session ends), and the weekly report notes it quietly under "In the sky" ("A meteor shower · Tuesday"). Under reduced motion, what stays just appears, and a passing one shows still and fades.
- Later, some events leave a collectable for the observing spot.

### Light pollution
- Outside a session, the sky is washed out by a faint glow and haze. When you lock in, the pollution fades away and the sky is crisp for the whole session, while your new star forms. This is the brand metaphor made playable.
- **Afterglow:** after a session ends, the sky stays clear for a while (about 20% of the session length) before the haze creeps back in over a few minutes. A session ended before its timer was done has no afterglow: the haze starts creeping back straight away.
- **The sky lives behind everything** in the desktop window, and the pages' hairline frames float over it (glass cards until the Phase 5 redesign), so you see the haze between sessions and the clear sky while you're locked in. The cards are slightly 3D: only their glass tilts under the pointer, with a sheen following it, so the text stays sharp.

### The Sky page (home)
The desktop window opens on the Sky. Its pages are **Sky · Your week · Profiles · Settings**, as small mono tabs with a glowing dot that glides to the selected one, and pages that slide in from their tab's side. The star count and stardust total sit in the top corner as a readout. The Sky reads as a landing page, top to bottom: the day, big ("Wednesday, October 28"), with a quote for it from the bundled pack and your own (the same one all day, folded away during a session or while the task list is open) and a readout of the time locked in today (counting the session going on and time past the timer; a session counts on the day it started) and the streak running now, only while one of 2 days or more is (never a broken one); then the sky, its constellations fitted into the gap; then the profiles to start a session; then one frame for thoughts for later (the satellites that came back, the task list when open, and the launch box); then Explore, the month and Tasks along the bottom. The page fits the window, so a long task list scrolls inside its frame.
- **Starting a session:** along the bottom of the sky, under "▶ Start a session", is a row of framed buttons, one per profile, each with its star-color dot, timer length, strictness and "▶ Start". Tap one to lock in. This is the only place sessions start in the window (the menu bar still works).
- **During a session** the timer floats over the clear sky in a frame while your star forms, glowing in the profile's star color: a big clock over a ruler from 0 to the timer's length with a marker at now, then End early with a line on what the profile's strictness will ask (End session once the timer is done or without one). Satellites in orbit, and passes left in a Hard session, are readouts along the bottom. There's no separate session screen.
- **Satellites live here too:** one frame under the profiles holds the satellites that came back after a session with Done, Later, Task and × (passive, never a pop-up), the task list (opened from the Tasks readout at the bottom, closed during sessions) and the one-line launch box. The month stepper and "Explore the sky" share that bottom line. Those in orbit, or back and waiting to be sorted (the latest 20), are drawn as tiny satellites, a body with two solar panels and a light that blinks now and then, slowly circling the constellations on their own tilted orbits, nearest of all. They all look the same. They fade in when launched and out when deleted or ticked off. One that goes onto the task list leaves orbit: it catches the sun and drifts away, once, the next time the Sky shows, since tasks belong to the list (and later to Reminders or Todoist), not the sky. What one says shows beneath it on hover only while exploring between sessions, never during one. They stay still under reduced motion, and stay out of the weekly report.
- **Telescope drift (in Explore):** while exploring, moving the pointer over the sky pans and zooms it gently toward the pointer, like nudging a telescope, with the nearer layers moving a little more for depth. Outside Explore the sky stays put, since moving it behind the buttons was too much. It stays still under `prefers-reduced-motion`.
- **Pan and zoom (in Explore):** drag the sky around (it drifts on a little when let go), and zoom toward the pointer with the wheel or a trackpad pinch, with the square + and − tools at the right, or with the keyboard (+ and −, the arrow keys, and 0 to go back to the start). It zooms in up to 4× and out far enough to see everything, and won't pan off the edge of the sky. Leaving Explore glides back. Under reduced motion it moves straight there.
- **Explore:** an Explore button clears the haze, lets the sky drift with the pointer, and drifts the buttons, frames and header toward you as they fade, as if puffing out of existence, so the sky is on its own to admire and name constellations. They come back the same way. Readouts sit in the four corners: the constellation in the faint crosshair in the middle (its name and dates), the zoom and how many weeks back it is, stars and weeks, and planets and stardust. A column of square tools at the right zooms in and out, goes back to this week, and names the constellation in the crosshair in place of its readout. "Esc · Done exploring" sits at the bottom. The pollution is mood, not a lock. Under `prefers-reduced-motion` it's a plain fade.
- **The stardust total** sits in the top corner. Later it opens the observing spot's drawer for decorating and companions, so rewards don't need a page of their own.

### Profiles page
Only for setting up profiles. Each one is a frame with a port in its star color, its summary and how many stars it has; it opens in place (one at a time) onto Timer, Apps, Sites, Plan and Star panels, with instrument-style controls: steppers for numbers, segmented switches for choices (each option saying what it does, with a line above that changes with the pick, so nothing depends on hovering), small toggles. Timer also has the "Ask for a goal" switch (§7). A timer's length (up to 6 hours) is also a ruler under a fixed marker, dragged or clicked like a tuning dial, always showing two hours' worth. Sites lists each built-in site and any added by address with a segmented switch for its level, and a box to add a site; what Tamed hides and page rules stay in the extension's settings. Plan is the profile's if-then plan (§3), with the same explanation and example as in the extension. Sites and plans sync both ways, newest change winning. Panels unfold rather than pop open. It doesn't start sessions, which keeps its purpose clear.

### Stardust
- Focus earns stardust: 1 per focused minute, with overtime minutes worth 1.5× (unannounced). The idle tail isn't counted (§2).
- It's shown as a running total from Phase 4 on. Nothing spends it yet; it's saved for the observing spot.
- Stardust, like everything in the sky, is worked out from the session log rather than stored on its own, so older sessions count too once it ships. Phase 2's session log must therefore keep everything rewards need: start, end, focused minutes, overtime, idle tail, profile, and gate outcomes.

### Later: the observing spot and companions
- **Your spot** is the foreground at the bottom of the sky: a campsite, rooftop, lighthouse or lake shore that you build up over time (tent → cabin → observatory) and decorate with things bought with stardust or found in rare events.
- **The town on the horizon** is where the light pollution comes from. When you lock in, its windows go dark one by one.
- **Companions:** small, cute characters who keep you company during sessions, e.g. watching through the telescope while you focus and dozing off when you go idle.
- This needs an art style and a set of assets, so it comes after the sky itself. Style still to decide. One option is dark silhouettes against the sky with small glowing details, which matches the icon and keeps each asset cheap to draw.

### What to protect
- **No paywall, ever.** Everything is earned by focusing.
- **Calm, not a slot machine.** Rare things should feel like discoveries.
- **Dreamy.** Motion is soft and floaty (drifting, fading, gentle glow), never snappy or flashy.
- **Drawn by code, not image packs.** It keeps the app small and gives every sky its own planets and shapes.
- **Local only.** Nothing leaves the device unless you share a constellation (below).

### Performance at scale
The sky has to stay smooth after years of use (aim for 20,000+ stars, roughly ten sessions a day for five years):
- One canvas. Still things are drawn once into off-screen images and reused: the Milky Way (drawn again only as it fills in or the window changes size), planets, and constellations too small to see twinkle while exploring (drawn again only when their size changes by more than 15%). While exploring, a planet's image is drawn again only once its surface or moons have moved about half a pixel, or as its lightning jumps or its aurora shimmers (a few to about 20 times a second), while it still moves round the sky every frame.
- Only what's on screen is drawn, checked constellation by constellation (with about 260 weeks in five years, that does the job of a spatial index), so only stars on screen twinkle. Comets and nebulae are skipped with their constellation. Zoomed out, a constellation's stars fold into a single soft glow instead of being drawn one by one.
- The sky is worked out from the log every time, but while a session goes on (once a minute) the weeks that haven't changed are handed back as they were, with their stars as drawn, so only the running week is worked out again. There's no physics simulation running like Obsidian's graph.
- It runs at 60 frames a second while the window is the one you're using, or 30 with **Battery saver** on (Settings → Sky). Twinkling and planets circling keep to that, and the loop sleeps between frames; only gliding, panning, zooming and things playing use every frame the screen shows (120 a second on some screens). While the window isn't focused (e.g. while you work in another app during a session, which is most of the time), the sky pauses: it stays as it was, still drawn again when something changes (like the forming star once a minute), and carries on from where it was when you come back, its clock stopped meanwhile so nothing jumps. Nothing is drawn while it's hidden, the page's once-a-second clock stops then too, and closing the window ends its webview altogether. Twinkling stops under `prefers-reduced-motion`.
- Measured in the browser pane (Chromium, not the app's WebKit) on a 120 Hz screen: the Sky page costs about 9% of one CPU core at 60 frames a second plus about 8 points in the GPU process, against about 5% plus 3 at 20. Explore went from 15–23% (it drew 120 times a second, every planet fresh) to about 16% at 60 and 12% at 20. Drawing itself is now under half a millisecond a frame; what's left is the browser's own cost of each new frame, which didn't change when drawing at half the resolution, so only the frame rate moves it.
- Measured with five years of 11 sessions a day (20,000 stars, 262 weeks, 61 planets, 511 comets) in a 400×600 window at 2× scale: a frame of the Sky went from 23 ms to 2 ms, and of Explore from 38 ms to 8.5 ms at the start, 15 ms at worst while zooming out, and 2 ms zoomed all the way out. Working out the sky again during a session went from about 80 ms to 6 ms.

### Architecture
- A reward engine turns the session log into plain events: minutes, overtime minutes, streak days, distractions resisted, satellites parked, rare events. Like the weekly report, the app's Rust core supplies the log and the engine itself is TypeScript in `packages/core`, unit-tested. The device's secret for rare events is generated once and kept in the app's database (and shared with the other devices in a sync group, §8), with when the Sky last showed new things; the events themselves aren't saved.
- Given the same log and the device's secret, it always produces the same sky, so the sky rebuilds after an import or a sync.
- The sky is one swappable renderer over those events, so the community can build other "worlds" later.

### Community sky (opt-in)
- A **"Share this constellation"** button sends **only the shape**:
  - normalized star positions
  - brightness, rounded into a few coarse levels so session lengths can't be inferred
  - optionally, a name
- **Nothing else is sent:** no stats, dates, timestamps, account, device ID or personal data. The share dialog says this plainly and shows the exact JSON being sent.
- The server doesn't log IP addresses. Names are optional and moderated before they appear.
- On the website, shared constellations are assembled into **community worlds**: a shared sky that anyone can explore.

## 6. Insights (weekly report)
- When you get distracted, broken down by time of day, site, and **why** ("You reach for Reddit when *stuck*, mostly 2–4pm").
- Typed reasons from the redirect page (when a profile requires one), grouped.
- Satellites parked and their outcomes.
- Overtime and focus trends.
- Suggestions, e.g. "Try scheduling Deep Work before 2pm."
- **Cues you can see:** when a pattern is clear ("You reach for Reddit when *stuck*, mostly 2–4pm"), the report offers an if-then plan for it (§3), drafted from the tip ("When I'm stuck and want Reddit, I'll write down the smallest next step."), with the same short explanation of what one is. It goes to the profile whose sessions the site pulled at most, can be changed before saving, and is only offered while that profile has no plan. Only inside the report, never as a pop-up.

Insights are all computed locally.

**In the desktop app:** "Your week" in the header, between sessions (dimmed during one), opens the report for the week in local time, starting on the first day of the week set on the computer (read from macOS's Language & Region settings, or the language's default elsewhere), with arrows back to earlier weeks. Clicking the week's title opens a calendar to jump to any week, with a dot on each day that had a session. The app's Rust core only reads the log for a range of dates; the report itself is worked out in `packages/core/src/insights.ts`, which knows the local time zone and can be shared with the extension later. It reads as one long numbered readout (01 Focus, 02 Profiles, …, counting only the sections shown): focus as a big figure with the change on last week, a thin line for each day of this week (as far as today) against a dashed line for the week before, with the busiest day marked, and a stacked line for each profile's share in its star color; trips to redirected sites by hour, site and "why", how often you went back to focus, and grouped intentions; what became of the satellites launched; and up to three suggestions, each only when there's enough data behind it (a reason a site keeps pulling you in, the time of day focus goes best, a timer that fits how long sessions really run). The extension on its own keeps only a week of gate visits and no session log, so it has no report yet.

## 7. Other features
- **Seiza observes; it doesn't interject.** It works quietly in the background and collects what happens, for the weekly report and the sky. It never prompts you to start a session or nudges you outside what you set up yourself. (Session suggestions from apps, e.g. "Obsidian opened — start *Writing*?", were considered and dropped for this reason.)
- **App rules**: close or hide desktop apps (Discord, Steam, Slack) during a session.
- **Session intention + reflection**: a one-line goal at the start, and "did you do it?" on the page that waits after the session ends (never a pop-up). It's a switch on each profile, off by default, so people who don't want to give a reason never see it (e.g. on for Deep Work, off for Writing). The goal can be skipped even when it's on. Answers feed the weekly report (§6).
  - Built in the app only: the extension's settings stay about what happens on each site. The switch is "Ask for a goal" in Profiles › Timer. Pressing that profile's start button on the Sky turns the row into one line, "What will you get done?", with ‹ Back and a button that reads "Start without a goal" until something's typed. The goal shows under the session's clock. Afterwards, a strip at the top of the Sky (in place of the day's quote) shows it with "Did you do it?" and Yes, I did / Not this time; it waits until it's answered or the next session starts. Sessions started from the menu bar or the extension don't ask.
  - The goal and the answer are kept in the session log (`goal`, `goal_met`), not synced with the extension. Your week's focus section counts them: "3 of 5 goals done · 1 unanswered".
- **Backup (export/import)**, in the app's Settings › Backup. Export saves everything in one JSON file in the Downloads folder ("Seiza backup 2026-10-29.json", only readable by you, never over an earlier one): profiles as they're saved (sites, page rules, redirect settings and plan, timer, app rules, star color, the goal switch), settings, your quotes, the sky's secret, and the whole log (sessions with goals, trips to sites, satellites with what they say, constellation names). Sample data stays out. The file says `"lockIn": "backup"` and a `version`; a newer version than the app knows is turned away.
  - Import adds what's in the file to what's here and never removes anything, so the same file twice changes nothing the second time. A profile in the file replaces the one here with the same id, others are added; its settings replace these except the satellite hotkey and blocking without the extension, which are set up on each computer. A computer with nothing logged takes the backup's sky secret, so the same sky comes back; one with its own sky keeps it. Everything brought in counts as seen, so it doesn't all play at once.
  - Only between sessions, so an import can't loosen a running session (Hard or not) or change its profile under it. The extension takes the imported profiles through the usual sync. The extension itself has no export, since its settings are about each site.
- **Allowlist-only mode**: block everything except the listed sites.
- **Earn your scroll** (opt-in): focus time earns a guilt-free budget of tamed-site time.

---

## 8. Platform, architecture & setup

### Stack
- **Desktop:** **Tauri 2**. A small Rust core with a **Vue 3 + Vite + TypeScript** UI, where most of the code will live.
  - It's chosen for weight and battery: about 10 MB vs Electron's ~150 MB, using the system webview.
  - Native OS access (process events, idle time, global hotkeys, menu bar) runs in Rust.
  - Use plain Vue rather than Nuxt; SSR adds nothing in a desktop app.
- **Extension:** **WXT + Vue + TS**, Manifest V3. One codebase builds both **Chrome-family** (Chrome, Brave, Edge, Arc) and **Firefox-family** (Firefox, Zen). Safari is not planned for now.
- **Shared:** `packages/core` holds the TS types and JSON schemas for Profile, Recipe, Session, RewardEvent and Satellite, plus shared logic. The Rust side mirrors the types; generate them from Rust with `ts-rs`/`specta` so the two never drift.
- **Storage:** local SQLite in the desktop app; `storage.local` in the extension, mirrored from the app.
- **Link between the two:** browser **Native Messaging**. The desktop app is the source of truth for the active session; the extension applies it and reports events back. **The extension is also a first-class standalone product**: it runs sessions, profiles, recipes and redirects entirely on its own, so people can install just the extension. The desktop app adds app blocking, stronger enforcement, Satellites hotkey, and the sky.
- **Blocking without the extension** (macOS first; `blocker.rs`): an opt-in setting in the app (Settings › Blocking without the extension) that keeps a running session's **Restricted** sites from loading in every browser and app, Safari included, for people who just want sites shut. It writes a marked "Seiza" section of `/etc/hosts` (each site, `www.` and `m.`), so it's whole sites only: taming, page rules, Redirected sites and Seiza's own pages still need the extension. It runs alongside a linked extension too. Editing that file needs root, so a tiny helper does it: the app's own executable, copied to `/Library/PrivilegedHelperTools` and run by launchd with `--blocker --uid <installer>`. It's installed only when the setting is first turned on (macOS asks for an administrator's password in its own dialog, via `osascript … with administrator privileges`), and Settings can update or remove it. It only accepts requests over its socket from the person who installed it (checked with `getpeereid`), checks every name itself, writes the file atomically and flushes macOS's DNS cache. The app checks in every minute while blocking; after five quiet minutes (it quit, crashed or was deleted) the helper unblocks everything, and it starts each run unblocked. During a Hard session the setting can't be turned off or the helper removed. Later: once builds are signed, install through `SMAppService` so the system checks the helper's signature (today the copy comes from wherever the app sits); Windows and Linux helpers.

### Performance & battery budget (a hard requirement)
- **Menu-bar resident:** closing the window *destroys* the webview, so only the small Rust core stays running. The UI reopens on demand.
- **Event-driven, not polling:** use OS notifications for app launches (macOS `NSWorkspace` notifications; Windows and Linux equivalents) and system idle checks only during sessions. There are no timers ticking while idle.
- **Extension:**
  - URL matching uses `declarativeNetRequest`, which the browser does natively, with no JS per request.
  - Recipes are injected as static CSS at `document_start`, so there's no flash and no layout-thrashing scripts.
  - MutationObservers are only added where a recipe truly needs them.
  - The MV3 service worker sleeps when idle.
- **Minimal dependencies:** no animation, UI-kit or charting libraries unless they earn their place. The sky is drawn on a single canvas.
- **Measure, don't guess:** track idle CPU, RAM with the window closed and open, and energy impact (Activity Monitor) in CI notes for each release.

### Repo layout (pnpm workspaces)
```
lock-in/
  apps/
    desktop/        # Tauri 2: src-tauri/ (Rust) + src/ (Vue 3 + Vite + TS)
    extension/      # WXT + Vue: background, content scripts, the redirect page, recipe live mode
    website/        # Night-sky themed product site (later; Nuxt static is fine here)
    sync-server/    # Self-hostable Rust sync server (one Docker image; see its README)
  packages/
    core/           # Shared TS types, JSON schemas, session/reward logic
    recipes/        # Built-in site recipes (JSON). Preview mocks live in the extension for now
                    # (apps/extension/components/preview) and move here once the desktop app needs them.
    quotes/         # Bundled quote packs (JSON)
  LICENSE           # AGPL-3.0
  PLAN.md
  pnpm-workspace.yaml   # packages: ["apps/*", "packages/*"]
```

### Suggested scaffolding commands (for your initialization)
- Prerequisites: Node LTS, pnpm, the Rust toolchain via `rustup`, and the Xcode Command Line Tools.
- Root: `pnpm init`, then `pnpm-workspace.yaml` with `apps/*` and `packages/*`.
- Desktop: `pnpm create tauri-app` → name `desktop`, in `apps/`, frontend **Vue + TypeScript**, package manager pnpm.
- Extension: `pnpm dlx wxt@latest init apps/extension` → template **vue**, package manager pnpm.
- Packages: `packages/core` (plain TS package; the others depend on it via `workspace:*`). `recipes` and `quotes` start as JSON folders.

### Updates (next; easy and quiet)
Today the extension updates itself through the stores, but updating the app means downloading the new release by hand. The goal is one click, and knowing a new version is out without being nagged about it.
- **The app checks on its own,** once a day while it's open. A check is one HTTPS request for `latest.json` on GitHub Releases. Nothing about you goes with it beyond what any web request carries (your IP address, to GitHub). Settings › Updates has a switch, **Check for updates automatically**, to turn it off, and a **Check now** button either way.
- **Offered once per version.** When a new version is found, the Sky shows one quiet line, like the tour's offer: the version, **What's new**, **Update and restart** and **Later**. Choosing **Later** (or updating) means that version isn't offered again; the next version is.
- **Then it stays in Settings.** Until you update, the Settings tab keeps a small dot, and Settings › Updates shows the version, what's new and **Update and restart**, so updating right away is always one click away.
- **Between sessions only:** during a session the offer waits, and the button in Settings says it waits until the session ends, since restarting would drop the session's blocking.
- **How it installs:** Tauri's updater plugin (`tauri-plugin-updater`) downloads the new build, checks its signature against the public key in `tauri.conf.json`, replaces the app and restarts it. macOS gets the universal `.app.tar.gz`, Linux the AppImage in place, and a `.deb` installs with `dpkg` after the system asks for a password. Signatures record the version (`requireSignedVersion`), so an old signed build can't be passed off as a new one. Copies not installed from a release (`tauri dev`) link to the download instead.
- **The blocker helper** (§8, Blocking without the extension) is a copy of the app, so after an update it's the old version. The app compares versions and, between sessions, asks to update the helper (macOS asks for the administrator password again). Until then the old helper keeps working.
- **On the release side:**
  - A signing key pair from `tauri signer generate`. The private key and its password become GitHub Actions secrets, which you create and add. Keep a backup: losing the key means existing installs can never update again.
  - `createUpdaterArtifacts: true`, so `release.yml` also uploads the `.app.tar.gz` and AppImage with their `.sig` files.
  - A step that writes `latest.json` (darwin-aarch64 and darwin-x86_64 both pointing at the universal build, plus linux-x86_64). `releases/latest` only follows published releases, so nothing reaches anyone until the draft is published.
- **0.1.0 can't update itself.** People on it download the next version once by hand; the release notes and the website say so.
- **Before shipping:**
  - Test on a real Mac that an app signed ad hoc and replaced in place still opens without a second **Open Anyway**.
  - Update `PRIVACY.md`'s Network section, which today says the app makes no network requests.
  - Check that the plugin's crate and npm package are at least two weeks old.
- **Later, maybe:** our own Homebrew tap (`brew upgrade`), since the main Homebrew list expects notarized apps.
- **To decide:** whether a check also warns when the linked extension is too old for the app.

### Sync (later, optional, privacy-preserving)
- **Local-first always.** Every feature works with no account, and with sync off (the default) the app makes no requests for it.
- **Self-hosted first:** an open-source `sync-server` (Rust + SQLite, one Docker image, which also fits a Proxmox LXC behind a Cloudflare Tunnel). It's written so a hosted tier could run the same image unchanged, so self-hosting is never second-class.
- **End-to-end encrypted:** the server only stores ciphertext.
- **A hosted tier maybe later** (Obsidian Sync-style), decided once self-hosting has proven itself. If it comes, it's the same image and nothing is gated; paying only buys convenience. **AGPL fits this model:** anyone running a modified sync server for others must publish their changes, and as the copyright holder you can still run the hosted tier.
- **Decided in planning (November 2026):**
  - **What syncs first:** the session log (with goals), trips to sites and constellation names, so every device draws the same stars, comets and weekly report: a session finished on the work computer shows up on the personal one, in its star, the streak, today's time and the report. Profiles and live sessions come straight after (below), and satellites after those.
  - **Joining, with no account.** Settings › Sync › **Use your own server** takes the server's address, then **Start a new sync group** or **Join with a pairing code**. On a device already in the group, **Add a device** shows a short code (a number and four words, e.g. `4821-orbit-lantern-quiet-river`), single use and valid for 5 minutes. The number picks the pairing on the server; the words are the PAKE's secret and never leave the two devices. Once it's typed on the new device, the existing one asks "Add *MacBook Air*?" with **Add** and **Don't add**. Settings › Sync lists the devices with when each was last seen and a **Remove** on each, so a lost laptop can be removed from any other device. Each device has its own random token for the server.
  - **How the key reaches a new device.** The group has one key the server never sees. The two devices use the pairing code in a PAKE (SPAKE2, RustCrypto's `spake2` crate) through the server to agree on a one-off encrypted channel, and the key travels inside it, so the server can't learn it even though the code is short. To get right:
    - A maintained, reviewed crate for the PAKE, never our own; the rest is a standard AEAD (XChaCha20-Poly1305, with random nonces so two devices never reuse one) and a KDF.
    - One guess per code: a failed attempt burns it on both sides, and it expires. That leaves anyone, the server's owner included, about a 1 in 10¹³ chance.
    - Key confirmation: both sides prove they agree before anything is sent, or stop.
    - The group's id and a purpose (`seiza pairing v1`) go into the exchange, so it can't be replayed or moved to another group.
    - Tests against the crate's known vectors, and an outside review before any hosted tier.
    - The key is kept in the app's database next to the sky's secret: it protects what's on the server, and the device already holds its own log.
  - **On the wire:** each device uploads its new and changed records as encrypted batches, and the server keeps each group's batches in order and hands a device the ones it hasn't seen. The server can see the group, the device, sizes and times, and nothing else.
  - **Conflicts: newest wins per record,** as on the extension's link (§8): every synced record carries its own `updated_at`. An answered "Did you do it?" beats an unanswered one whatever the time, and a cleared constellation name is kept as cleared so it doesn't come back. Nothing synced is ever deleted, and sample data never syncs.
  - **One sky.** The device that starts the group shares its sky secret, inside the encrypted data. A device that joins with sessions of its own takes it, so its past rare events and comets are drawn again once; stars, constellations and planets don't use the secret, so they stay. Before joining it says so in plain words, with **Join and match the sky** and **Cancel**. When the Sky last showed new things stays per device, so the redrawn events don't all play at once. (Merging two logs can also move personal bests and add moons, whatever the secret.)
  - **When it syncs, within the battery budget (above):** no timers. For stats, a device sends after a session ends, a goal is answered or a constellation is named, and fetches when the window opens and when a session starts or ends. Live sessions add one quiet connection (below), which stats then use too.
  - **Live sessions, straight after stats:** start Deep Work on one computer and it starts on all of them.
    - **Profiles sync just before:** edit a profile on one computer and it changes on all of them (newest wins per profile, a deleted one kept as deleted), so every computer has every profile and every star's color. Settings stay per computer. A running session carries a copy of its rules, so editing its profile elsewhere mid-session, or loosening a Hard one, can't change what's running.
    - **Arriving in seconds:** while sync is on, the app's background core keeps one quiet connection (a WebSocket) to the server, which tells a group's devices when something new arrives, encrypted like the rest. Nothing polls, and it reconnects after sleep.
    - **One session:** one id, one log record and one star, written by the computer that ends it. **End early** on any computer ends it everywhere, after asking whatever its strictness asks, and emergency passes are shared, since used passes are in the synced log. A computer that wakes mid-session joins it if it's still running.
    - **Idle:** ending a session for being idle becomes off by default (§9). With it on, a session on several computers only ends once all of them have been idle that long (a sleeping or offline one counts as idle): the computer about to end it asks the others first, so nothing ticks in the background.
    - **Following is a switch:** Settings › Sync on each computer has **Start sessions from my other computers here**, on when it joins. Off, that computer only syncs stats (e.g. a shared family computer). Nothing pops up when a session arrives; the menu bar and the Sky just show it, as if it had started here.
    - Each computer still watches only its own browsers.
  - **The desktop app only, at first.** A linked extension's trips already reach the app; an extension used on its own doesn't sync.
  - **Who can start a group on a server:** a setting in the server's `.env`. `first group only`, the default: the first device starts its owner's group and then the server closes to new ones, so a personal server needs no invites. `invites`: an invite code per new group, for sharing a server with friends, or a hosted tier's signup. `open`: anyone. All three with per-group size caps and rate limits (and Cloudflare's rate-limit rules in front). A plain, non-secret `Seiza-Protocol: 1` header lets the server drop scanners before doing any work. A key built into the app wouldn't stop anyone, since the app is open source; the pairing code is what protects a group.
  - **Counting users, with no telemetry.** The server's `stats` command counts groups (about one per person), devices, and groups active in the last 7 and 30 days, from what it keeps anyway. A hosted tier would add a paying flag per group, to compare paying with active. The app sends nothing extra, people who stay offline aren't counted, and a self-hosted server only tells its owner. `PRIVACY.md` will say the server keeps when each device was last seen.
  - **Backups:** every device has the whole log, so a lost server loses nothing; re-pair and upload again. The docs show backing up the server's SQLite file anyway, and the app's Backup export (§7) stays the offline copy.
  - **Leaving and moving.** **Leave sync** stops this device syncing and keeps everything on it; the server deletes the group once its last device leaves. **Move to another server** takes a new address, one device uploads the whole log, and the others re-pair. If a server goes away, the app keeps working offline, with "Last synced 3 days ago" in Settings › Sync and nothing more. A hosted tier shutting down would then need only notice (e.g. 90 days).
  - **First run stays as it is for now** (offline, nothing asked). The first-run choice of offline, hosted or your own server comes with a hosted tier.
- **Build order,** one PR each:
  1. ✅ **Sync-ready records in the app** (`records.rs`): an `updated_at` on sessions, trips and constellation names, a cleared name kept with no name, and the merge rules, with tests, before any network code. Each change made on a device also takes a number from a local counter, so sync asks for what changed since the last number it sent, whatever the clocks say; records from other devices are saved with their own `updated_at` and aren't sent back. A change made here is always newer than the copy it replaces, even one from a clock that's ahead. Records travel as JSON with a `kind` (`session`, `trip`, `constellationName`).
  2. ✅ **The server** in `apps/sync-server` (`seiza-sync`, axum + SQLite): groups, devices and their tokens (kept only as hashes), each group's batches in order, the new-groups setting, size caps, rate limits per address (by Cloudflare's `CF-Connecting-IP` when set), the protocol header, and the pairing relay for step 3 (opaque messages between the two devices, single use, 5 minutes, one new device per pairing, kept in memory). Commands: `stats`, `invite`, `backup` and `health`. A Docker image checked in CI, and its README covers running it (a Proxmox LXC behind a Cloudflare Tunnel as the example), backups and the API. The app doesn't use it yet.
  3. **Encryption and pairing** in the app: the group key, encrypted batches, and the SPAKE2 pairing with the checks above, over the server's pairing relay.
  4. **Settings › Sync:** Use your own server, Start a new sync group, Join with a pairing code, Add a device, the device list with Remove, Last synced, Leave sync and Move to another server, and joining with one sky.
  5. **Sending and fetching** at the moments above, with the Sky, streaks, today's time and the weekly report counting every device.
  6. **Before release:** `PRIVACY.md`'s Network section, the website's sync note (Phase 6, §9), and every new dependency at least two weeks old.
  7. ✅ **Profiles** (`records.rs`, a fourth kind of record, `profile`): an `updated_at` on each profile, deleted ones kept as deleted (with their last copy, so their stars keep their color and a session running on one keeps its name), settings left per computer. The ordered list the app and the extension use stays as it was; sync keeps its own copy of each profile beside it, notes every change made here (in the app, by the extension, or from a backup), and folds in other devices' changes: edits in place, deletions out, new ones on the end, newer than the extension's list so it takes them on too.
  8. **Live sessions:** the server's quiet connection, one session across computers (ending, shared passes, idle, joining on wake), the copy of the rules a session carries, and the follow switch.
  9. **Later:** satellites; a hosted tier maybe, after the outside review.

### Support the developer
- A small, quiet "Support Seiza" link appears only on the website and at the very bottom of Settings. It goes to [Ko-fi](https://ko-fi.com/sortelone).
- No nags, popups or feature gates.

---

## 9. Roadmap

**Phase 0: Setup** (you scaffold)
- ✅ pnpm monorepo, AGPL-3.0 license, shared types in `packages/core`, unit tests, in-repo browser tests, CI on every push.
- ✅ Lint (ESLint with typescript-eslint and eslint-plugin-vue) and format (Prettier), checked in CI.
- JSON schemas for recipes and profiles (still to do, for the recipe editor). Backups don't need one: the app's Rust types define the file and check it on import (§7).

**Phase 1: Extension MVP** *(replaces your manual SocialFocus toggling first)*
- ✅ Standalone: sessions run from the extension popup, with no desktop app needed (the app plugs in later via Native Messaging).
- ✅ Recipes for Reddit, YouTube, X, Instagram, TikTok, Facebook, LinkedIn and Pinterest: tame, grayscale, redirect. (Still to do: check each on the real site.)
- ✅ Page rules (§1), including redirecting Shorts and Reels players inside single-page apps.
- ✅ Profiles: CRUD + starter profiles, manual start/stop, four site levels including Restricted, settings page with a Manage mode for deleting several profiles at once.
- ✅ Redirect destinations: intention gate with countdown (fixed or escalating), quote card (with your own quotes), productive URL, stuck page. Passes, and what happens when they run out (nothing, pause or redirect).
- ✅ Attempt counter + why-tap.
- ✅ Recipe editor with animated preview mocks for all eight sites. (Editing a recipe's selectors, and live mode, come in Phase 5.)

**Phase 2: Desktop app (Tauri)** *(macOS and Linux; Windows support comes later: Native Messaging registry entries, idle time, and finding the browser behind a bridge)*
- ✅ Menu-bar app, session engine, quiet timer + idle detection.
- ✅ Strictness levels + emergency passes.
- ✅ Native Messaging link + extension heartbeat: if the extension goes quiet during a session, warn → close that browser, depending on strictness. The app and extension each work alone, so it only watches sessions whose profile tames sites, and closing is opt-in in Settings. Hard sessions used to count as broken too; not any more (§9).
- ✅ App rules, SQLite. The session log keeps everything the rewards will need later (§5), so stardust and the sky can be worked out from past sessions once they ship. (On macOS, app launches come from `NSWorkspace` notifications; Linux has no unprivileged equivalent, so it checks every 10 seconds during sessions, about 9 ms each.)

**Phase 3: Satellites & insights**
- ✅ Satellites (§4) in the desktop app: the Alt+P box, menu-bar capture, the post-session return, and the task list.
- ~~Satellites in the extension~~: built, then taken out as out of scope (§4); satellites live in the app.
- ✅ Weekly report (§6) in the desktop app.

**Phase 4: The night sky**
Built in slices, one PR each:
1. ✅ **Reward engine + first sky:** stars from the session log (sizes, shared and skipped stars), the stardust total, profile star colors (defaults, plus a choice when creating a profile), and one canvas behind the window with twinkling stars, light pollution between sessions, a clear sky during them with an afterglow, glass cards that tilt under the pointer, sliding pages, and sample sessions for development builds.
2. ✅ **The Sky as home:** the Sky page with its profile buttons, the session over the sky, satellites and tasks, the telescope drift and Explore; the Profiles page for editing only; and animations for the weekly report's calendar (switching weeks and months, opening and closing it).
3. ✅ **Weekly constellations:** naming, names shown beneath them on hover in Explore, the close-together layout on the Sky page and the spread-out week-order layout in the weekly report, and the week's constellation behind the report's header and week title.
4. ✅ **Planets and moons:** a seeded planet for every month with focus, one of six kinds of world, forming from dust to a lumpy rock to a round world and gaining its surface, rare unusual shapes (star, pumpkin, gem, peanut, flat, ring world, linked rings), rings in six styles (then a crossing ring), glass shells, an atmosphere and an aurora as the month's focus grows, tinted by the month's top profile and spinning in Explore; moons from streaks; planets framing the constellations on the Sky, circling in Explore with hover labels, and beside the week's constellation in the report.
5. ✅ **Comets and rare events:** comets for going back to focus, staying beside their week's constellation; shooting stars, meteor showers, auroras and supernovae from a chance roll with the device's secret, and for personal bests; things that happen to the month's planet (a moon crash, a chunk breaking off, shattering); a halo for your best month's planet; each playing once on the Sky and noted in the weekly report.
6. ✅ **Streak stats and satellites in the sky:** the current streak (while one is running) and the longest in the weekly report, never a broken streak; satellites in orbit and back circling the constellations, their text on hover in Explore between sessions only, and one that goes onto the task list drifting away.
7. ✅ **Performance at scale:** the Sky a month at a time with stepping back, pan and zoom in Explore, drawing only what's on screen at 60 frames a second, cached still layers (the Milky Way, small constellations and planets), the Milky Way filling in with focus hours, older constellations folding into it on the Sky and into a soft glow far away, and reusing past weeks while a session goes on.

**Phase 5: Polish & release**
- ✅ **Milestone color themes** (idea from Nike Run Club's milestone levels): huge lifetime milestones of focus unlock new glow colors for the frames' corner ticks and the tabs' glowing dot: Starlight from the start, then Aurora at 100 hours, Nebula at 500, Dawn at 1,000 and Sunlight at 5,000 (overtime counts, broken sessions don't). Once unlocked, you pick one in Settings › Glow color, and can always go back. Earned only by focusing, never bought, and unlocked quietly: Settings shows only the colors unlocked so far and when the next one comes; nothing pops up. Desktop only for now; the extension's pages keep Starlight.
- UI redesign, from the reference images in `apps/desktop/references` (some of it is already in: glass cards with thin light borders and a sheen, the glowing tab pill, the floating dock of profiles, and dot charts with a "Peak" pill in the weekly report):
  - **Dark glass with colored glow** (`glow-cool-and-background-with-glassy-ui-elements-nice`): color bleeding softly from behind a card, e.g. the running session glowing in its profile's star color; a small floating toolbar of icon buttons for Explore (zoom in and out, recenter, name a constellation), which fits with pan and zoom (Phase 4); tiny stats in a corner while exploring; small colored status dots.
  - **Charts** (`ui-charts-dots-chart-cool-other-charts-cool-too-i-like-ui`): a big number with a small "vs last week" beside it on every report card; hatched bars for comparisons (this week against last), with the one you're looking at solid and a pill showing the change; thin segmented bars for each profile's share of focus; a "compared to" week next to the week picker.
  - **Floating boxes** (`ui-floating-boxes`): on wider windows, a bento grid for the weekly report and Profiles, with cards of different sizes side by side and one accent card in a star color (e.g. the week's top suggestion); larger corner radii and soft, wide shadows; a date block ("Friday · Oct 30") style for headers. Tags on tasks only if the user sets them (§7: the app observes, it doesn't label things for you).
  - **Chosen direction: C, Observatory HUD.** Three HTML prototypes live in `prototypes/ui-redesign/` (open `index.html` to compare them). C is the one to build; A (Nebula glass) and B (Bento drift) stay there for reference, to borrow parts from. C's look: the sky stays the star, with hairline frames with bright corner ticks instead of filled glass cards, small monospace readouts (stars and stardust in the top corner, tasks and Explore along the bottom), mono tabs with a glowing dot for the current page, and colored status dots.
    - ✅ **Sky** (with the frame: the header readout, the tabs and the frames everywhere): satellites that came back in a frame with Done, Later, Task and ×; a "Start a session" row of profile buttons, each with its color dot, length, strictness and an explicit "▶ Start"; a one-line satellite box; tasks in a drawer from the bottom bar, closed during sessions.
    - ✅ **Session:** a big clock over a ruler from 0 to the timer's length with a marker at now, "End early" with a line on what the profile's strictness will ask, and in-orbit and passes-left readouts.
    - ✅ **Explore:** readouts in all four corners, a faint crosshair, a column of square tool buttons (zoom, back to this week, name), and "Esc · Done exploring".
    - ✅ **Your week:** one long numbered readout (01 Focus, 02 Profiles, …) with a thin line for this week against a dashed line for the week before and the peak marked, a stacked line for each profile's share, and hour-of-day bars for trips.
    - ✅ **Profiles and Settings:** each profile opens in place onto Timer, Apps, Sites and Star panels with instrument-style controls (steppers, segmented switches, small toggles); Settings is numbered sections like the report.
    - ✅ **A landing page** (after the redesign): the Sky re-ordered top to bottom as the day (big date, a quote for the day, time locked in today and the streak), the sky, starting a session, then thoughts for later in one frame.
    - ✅ **The satellite box** (Alt+P): a frame like the app's, in a see-through window, that drifts up into place, lifts off with the thought when launched and fades when closed (only fades with reduced motion). It takes the glow color too, and the thoughts in orbit circle it as tiny satellites on tilted orbits, passing behind it and in front, with a launched one joining them. See-through windows need Tauri's `macOSPrivateApi`, which only rules out the Mac App Store (which the app's process watching rules out anyway).
    - ✅ **Lists that drift:** satellites that came back, those in orbit and tasks drift in and out instead of snapping. One that leaves fades and blurs its own way (a ticked-off one glints, Later drifts up into orbit, Task drifts down, × fades aside) and the gap closes; whole sections fold away. **The timer's frame is glassier** during a session: the usual dark tint (a lighter one made the clock harder to read), with a sheen, lit edges and the session's color pooling at the bottom, drawn with gradients rather than a backdrop blur.
    - No lines drawn between the UI and the sky: they were tried and dropped as unclear.
    - ✅ **The extension too:** the popup, settings page, redirect page and the cover over open tabs share the app's look: hairline frames with corner ticks, mono readouts and worded keys, segmented level switches, the profiles' star-color dots and "▶ Start", over a still night sky drawn with gradients. The cover tints the page instead of blurring it, so a busy page underneath costs nothing.
- ✅ The satellites' dreamy orbiting look (§4): the thoughts in orbit circle the thoughts frame on the Sky as tiny satellites, as they do the Alt+P box, and a launched thought lifts out of the launch box and joins them with a glint. The launch box is marked by a tiny satellite circling a star, with the hotkey beside Launch; the tour explains satellites (it replaced a first-use line). Along with it, clearer help text for app rules (what Close and Hide do, and that rules apply when a session starts and again whenever the app is opened during it).
- Richer report charts if the plain lines and bars stop being enough, e.g. focus across many weeks or a day-by-hour heatmap. A chart library such as [vue-data-ui](https://vue-data-ui.graphieros.com/docs) could do these; weigh its size against the performance budget (§8) first.
- ✅ A short tour for new users, in the app and the extension. The app's Sky offers it once in a one-line strip (Take the tour, No thanks), and the extension's popup does the same. It walks the real pages in short steps, with the part a step is about framed and the page switching as it goes: the app's covers the sky, starting a session, profiles, satellites (and Alt+P) and the browser; the extension's covers profiles, site levels, the redirect page and linking the app. Every step has Back, Next and Skip tour (and Esc ends it). It can be taken again from the app's Settings › Tour and from the top of the extension's settings page.
- ✅ Sites editable from either side: the app's Profiles › Sites sets each site's level and adds or removes sites by address, and the extension's settings and popup can add any site too (the popup offers "Keep it away from now on" during a session).
- ✅ From the research (§12): **one predictable redirect page** with Five minutes first, replacing the four random pages (§3); and **if-then plans** on profiles, set in the extension's settings or the app's Profiles › Plan, shown on the redirect page, and offered by the weekly report when a pattern is clear (§3, §6).
- ✅ From the research (§12): **session intention + reflection** as a switch on each profile, in the app (§7).
- ✅ **Element picker:** "Hide parts of this page" in the popup, on any tamed site (built-in or added by address), opens a small panel on the real page. Each click hides that part straight away (the page's own links and buttons wait), so you see what the site will look like; parts picked this time are listed with Bigger (the part around it) and Undo, and ones hidden before with Show again. Nothing is saved until Done, and Cancel puts everything back. Parts belong to the site, not a profile: they're hidden whenever it's tamed, in any profile's session or all the time, so they're picked once. During a Hard session parts can be hidden but not shown again. Sites added by address can now be tamed this way.
- ✅ **Tamed all the time:** the popup's "On this site" has two tabs, **No session** and **In a session**, opening on the one that applies now. No session holds "Tame Reddit all the time", which belongs to no profile and has toggles of its own; In a session holds each profile's level and toggles, so Reddit can be tamed every day and still restricted in a session. The extension's settings have a matching No session section, which also lists each site's hidden parts. These site settings sync with the app and go in its backups (`siteSettings`).
- ✅ **Recipe live mode:** the site's own toggles in the same panel, for whatever tames the page right now (the session's, or the site's all-the-time ones, named in the panel). Flipping one changes the page straight away; like picks, nothing is saved until Done and Cancel puts it back.
- ✅ **Twitch, Threads and Bluesky** recipes, each with a preview drawing and a test page. Checked against the live sites logged out only (Bluesky's test ids also against its open-source app); logged-in parts like Twitch's followed channels follow community filter lists. Hiding a playing video or stream (Twitch's front page plays one) also pauses it, since hiding alone leaves its sound on.
- More built-in recipes as people ask for them.
- ✅ Your own quotes in the app too: Settings › Your quotes adds and removes them, the Sky's quote for the day picks from them (more often than the bundled ones), and (until the extension dropped quotes) they synced with the extension's. The bundled pack grew from 22 to 181 quotes, all public domain with their sources noted (two modern Marcus Aurelius translations and a 1932 Seneca were replaced); the daily quote fetch was dropped (§3).
- ✅ The support link (§8): "Support Seiza on Ko-fi" at the very bottom of the app's Settings and of the extension's settings page, and a Sponsor button on the repository (`.github/FUNDING.yml`). The website gets it in Phase 6.
- ✅ Export/import: Settings › Backup in the app, everything in one file, imported by adding to what's here, between sessions only (§7).
- ✅ Blocking without the extension, macOS first (§8). Windows and Linux later.
- **First release:** the desktop app on GitHub Releases and the extension on the Chrome Web Store and Firefox Add-ons.
  - ✅ Release prep: pushing a version tag builds the macOS app (one universal build, signed ad hoc only) and the Linux `.deb` and AppImage onto a draft release with install steps (`.github/workflows/release.yml`); `pnpm zip` builds the Chrome store zip without the manifest key; the Firefox zip declares no data collection and comes with a sources zip (`SOURCES.md`); the privacy policy (`PRIVACY.md`) and store listing text (`apps/extension/STORE.md`).
  - ✅ The Chrome Web Store's id for the extension is in `hosts.rs`, next to the unpacked build's.
  - Left: checking each recipe on the real sites while logged in; screenshots and the promo tile; submitting to both stores.
  - Not paying for Apple or Windows code signing for now. Unsigned builds work after **Open Anyway** in macOS's Privacy & Security settings, which the release notes explain.

**Two smaller session changes** (decided in the sync planning, November 2026)
- ✅ Ending a session for being idle is off by default for new installs, and stays in Settings (a switch, then the minutes) for anyone who forgets to end sessions. Stepping away for a drink shouldn't end one. Installs from before keep the 15 minutes they had, unless they'd changed it.
- ✅ No more broken sessions: the extension heartbeat still warns and, if turned on, closes the browser (in Medium and Hard alike), but the session no longer counts as broken. Sessions already logged as broken stay as they are, and still earn nothing.

**Phase 6: Website, community sky & sync**
- Marketing website (§10) + community-worlds gallery, with the Why it works page (§12).
  - ✅ The website in `apps/website`, built from mock-up D (`prototypes/website`): the framed home page with the Lock in switch, the four levels, the redirect page, the sky, satellites, privacy and install; Why it works (§12) and Privacy pages; the quiet Ko-fi link in the footer; and a note that sync is planned, free to self-host or hosted for a small fee (§8). Static, served by a Cloudflare Worker deployed from GitHub. Still to do: the domain, store links once the extension is listed, and the community-worlds gallery.
  - ✅ Copy brought up to date after the sync and habit goals planning (§8, §12): the sync note (`SyncNote.vue`) says sync is being built and self-hosted, with no hosted tier promised; Why it works (`why-it-works.vue`) lists habit goals as out of scope rather than parked.
- Constellation sharing endpoint.
- Sync between devices (§8, Sync): self-hosted first, starting with the session log, trips and constellation names, in the build order there. Profiles and live sessions straight after, then satellites; a hosted tier maybe later.

**Next: easy app updates** (§8, Updates): a daily update check you can turn off, each new version offered once on the Sky and then kept in Settings with a dot, and one-click Update and restart between sessions; signed update builds and `latest.json` from `release.yml`.

**Later: the observing spot** (once the art style is chosen)
- Your spot, the town on the horizon, decorations and companions, bought with stardust or found in rare events (§5).

---

## 10. Marketing website (Phase 6, to discuss in detail later)
- **Hero:** a light-polluted night sky. Around the edges, distractions crowd in: fake reels, tweet cards, notification badges, autoplaying thumbnails, all jittering and glowing. They're original mock UI, not real brand assets.
- **The toggle:** a single big **Lock in** toggle sits at the center. Flipping it pushes every distraction out past the edges with springy, staggered motion. The orange glow drains away, and a deep, clear sky full of stars fades in, ideally with a constellation drawing itself.
- **Design language everywhere: distracted vs. focused.**
  - *Distracted* states are noisy, saturated, cluttered and jittery.
  - *Focused* states are calm, dark, spacious and slow.
  - Animations, sections and even button hovers play on this contrast.
  - It respects `prefers-reduced-motion`.
- Also on the site: download links, the community sky (§5), how it works, privacy promises, and the quiet support link.
- **Why it works:** a page that walks through the research behind the features (§12) in plain words, each with its source, and is honest about what's strong, what's modest and what's only reasoning.

---

## 11. Verification (Phase 1–2)
- **Extension, automated:** `pnpm test` (core logic) and `pnpm test:e2e` (the real extension in Chrome, against stand-in pages for each site) run in CI on every push. They cover sessions, popup toggles, every recipe, page rules (including single-page navigation), redirects and the gate, every destination, pass expiry, restricted sites, settings and the profile editor.
- **Extension, by hand:** run `wxt dev` for Chrome and Firefox. In a *Writing* session:
  - each recipe hides what it says on the **real** site (stand-in pages can't catch a site changing its HTML), while threads, search and messages still work
  - grayscale is applied
  - x.com goes to the redirect page, whose Go to button counts down and grants a 5-minute pass that later expires
  - clicking a Short from a YouTube video goes to the redirect page, and Not now returns to the video
  - the attempt counter goes up
- End the session: everything returns to normal.
- **Recipe editor:** each toggle animates the preview mock instantly. Exported JSON matches the schema in `packages/core`.
- **Desktop:**
  - Starting a profile in the app updates the extension within ~1s.
  - Disabling the extension mid-session triggers the strictness response.
  - In *Silent* mode the timer ends with no visible change at all. With "Show time since done" on, it shows "Done · N min ago".
  - Overtime minutes are recorded either way.
  - With ending idle sessions turned on, idle auto-ends the session without counting the idle tail.
  - Hard mode can't be ended without a pass.
- **Performance:** with the window closed, check idle CPU (~0%) and memory in Activity Monitor. Energy impact should be "Low".

---

## 12. The science behind it
Seiza should help because of research that has held up, not because it sounds like it should. Most of what it does already rests on that research. This section keeps track of which findings back which features, so new features can be checked against it and the website's Why it works page (§10) can explain it. Each finding was checked against its paper's abstract (October 2026); the links are under Sources. "Strong" means a large meta-analysis, "modest" means real but small or a single study, and "reasoning" means not tested.

### What Seiza already does, and why it works
| Finding | Strength | In Seiza |
|---|---|---|
| **A pause and an easy way out cut how often you give in.** In a 6-week field study of the one sec app (Grüning et al., 2023), openings of the chosen apps fell 57%. The option to back out mattered most and the wait helped; the message alone did nothing. | Modest (one study) | The redirect page's wait and its Not now (§3) |
| **Limits you set yourself work.** Letting people cap their own future screen time cut social media use a lot, and short-term help to cut back kept working after it ended (Allcott, Gentzkow & Song, 2022). | Strong (large field experiment) | Profiles, Hard mode and its passes, Restricted sites |
| **Habits form from repeating something in the same context, and then that context sets them off** (Wood & Rünger, 2016). | Strong (review) | Taming feeds and hiding recommendations, removing the cue instead of fighting it |
| **Writing down a plan for an unfinished task stops it from pulling at your attention** (Masicampo & Baumeister, 2011). | Modest (a set of lab studies) | Satellites (§4) |
| **Tracking your progress helps you reach goals, more so when it's written down** (Harkin et al., 2016, 138 studies). | Strong (meta-analysis) | The weekly report (§6), the sky |
| **Habits take a while, and one missed day doesn't undo one.** In the best-known study, the median was about 66 days, from 18 to 254 (Lally et al., 2010). | Modest (small study, not yet replicated) | Never showing a broken streak (§5); no "21 days" claims anywhere |
| **Being kind to yourself after a slip goes with less procrastination next time** (Wohl, Pychyl & Bennett, 2010, students across two exams). | Modest (correlational) | A session that ends early just ends: no scolding, no lost progress beyond that session |
| **Starting small makes starting easier** (the two-minute rule in *Atomic Habits*). | Reasoning | The quiet timer (§2): a short timer you're free to run past |
| **People keep at things they chose themselves; pressure from outside wears that down** (self-determination theory, Ryan & Deci, 2000). | Strong (a large body of work) | Seiza observes and never interjects (§7); every default is editable |

### New, from the research (decided October 2026)
- **Five minutes first** (§3), at the heart of one redirect page that always looks the same.
- **If-then plans** (§3, §6). Deciding ahead of time "when X happens, I'll do Y" had a medium-to-large effect on reaching goals across 94 tests (Gollwitzer & Sheeran, 2006), one of the strongest results in the field. Since few people know the term, the app explains it where you write one.
- **Session intention + reflection** (§7), a switch on each profile. A specific goal beats a vague "do your best" (Locke & Latham, 2002), and the "did you do it?" afterwards is progress tracking.

### Considered and left out
- **Planning the week at a fresh start** (a Monday, a new month): Seiza isn't a planning app.
- **Break timers between sessions:** short breaks help with tiredness more than with output; not worth the extra settings.
- **A separate rule for forgiving slips:** Seiza already doesn't punish them (above).
- **Habit goals** ("earn your sites": a profile's sites staying Restricted each day until a habit like reading 20 pages is logged). Dropped in November 2026 as out of scope: Seiza helps you focus and keeps distractions away; building habits is another app's job. The idea behind it, temptation bundling, also had a smaller effect in a larger follow-up than in the first study (Milkman, Minson & Volpp, 2014).

### What hasn't held up (don't design on it)
- **Willpower as a fuel tank that runs out** ("ego depletion"): a 23-lab replication found an effect of about zero (Hagger et al., 2016).
- **"21 days to form a habit":** not from research (above).
- **Self-imposed deadlines beating procrastination** (Ariely & Wertenbroch, 2002, often cited for commitment devices): a replication didn't find it, and the authors have asked for the paper to be retracted. Cite Allcott et al. for limits you set yourself instead.

### What to keep out
- **Points, badges and leaderboards.** Expected, tangible rewards can wear down people's own motivation (Deci, Koestner & Ryan, 1999; still debated). The sky reflects what you did; it isn't a currency to chase.
- **Streaks you're afraid to lose.** Show the current and longest streak, never a broken one (§5).
- **Nudges and nagging.** Seiza only acts on what you set up (§7).
- **Variable rewards** (the slot-machine pull) only for focusing, never for visiting a site, and never something you can chase (§5, Comets and Rare events).

---

## Sources
- Freedom: [free vs premium](https://support.freedom.to/en/articles/13764747-what-s-included-in-free-and-premium-plans), [features](https://api.freedom.to/features)
- Focus Friend: [App Store](https://apps.apple.com/US/app/id6742278016), [review](https://makeheadway.com/blog/focus-friend-app-review/)
- one sec (attempt counter, pause friction): [review](https://cellphoneplans.androidauthority.com/CellPhones/Apps/one-sec-app-review)
- Opal (focus score, mindful block screens, intentions): [the new Opal](https://opal.so/blog/introducing-the-new-opal), [review](https://makeheadway.com/blog/opal-app-review/)
- Cold Turkey (random-text unlock, Frozen Turkey): [TechRadar](https://www.techradar.com/reviews/cold-turkey)
- SocialFocus: [socialfocus.app](https://socialfocus.app/)
- Open-source prior art: [Focuser (Rust/Tauri)](https://github.com/aadeshrao123/Focuser/)
- iOS limits (deferred): [Screen Time API in 2026](https://dev.to/twillywill/the-ios-screen-time-api-in-2026-what-apples-docs-dont-tell-you-151e), [Personal Team can't use Family Controls](https://hsb.horse/en/blog/personal-team-family-controls-limitation/)
- Tauri 2: [roadmap / plugins](https://v2.tauri.app/blog/roadmap-to-tauri-2-0/)
- Research (§12):
  - Allcott, Gentzkow & Song (2022), [Digital Addiction](https://www.nber.org/papers/w28936), *American Economic Review*
  - Deci, Koestner & Ryan (1999), [extrinsic rewards and intrinsic motivation](https://doi.org/10.1037/0033-2909.125.6.627), *Psychological Bulletin*
  - Gollwitzer & Sheeran (2006), [implementation intentions and goal achievement](https://doi.org/10.1016/S0065-2601(06)38002-1), *Advances in Experimental Social Psychology*
  - Grüning, Riedel & Lorenz-Spreen (2023), [directing smartphone use through the self-nudge app one sec](https://doi.org/10.1073/pnas.2213114120), *PNAS*
  - Hagger et al. (2016), [a multilab preregistered replication of the ego-depletion effect](https://doi.org/10.1177/1745691616652873), *Perspectives on Psychological Science*
  - Harkin et al. (2016), [does monitoring goal progress promote goal attainment?](https://doi.org/10.1037/bul0000025), *Psychological Bulletin*
  - Lally et al. (2010), [how are habits formed](https://doi.org/10.1002/ejsp.674), *European Journal of Social Psychology*
  - Locke & Latham (2002), [a practically useful theory of goal setting](https://doi.org/10.1037/0003-066X.57.9.705), *American Psychologist*
  - Masicampo & Baumeister (2011), [plan making can eliminate the cognitive effects of unfulfilled goals](https://doi.org/10.1037/a0024192), *Journal of Personality and Social Psychology*
  - Milkman, Minson & Volpp (2014), [temptation bundling at the gym](https://repository.upenn.edu/oid_papers/150), *Management Science*; the larger follow-up: [Wharton working paper](https://faculty.wharton.upenn.edu/wp-content/uploads/2017/07/TemptationBundling.pdf)
  - Ryan & Deci (2000), [self-determination theory](https://doi.org/10.1037/0003-066X.55.1.68), *American Psychologist*
  - Wohl, Pychyl & Bennett (2010), [self-forgiveness for procrastinating](https://doi.org/10.1016/j.paid.2010.01.029), *Personality and Individual Differences*
  - Wood & Rünger (2016), [psychology of habit](https://dornsife.usc.edu/wendy-wood/wp-content/uploads/sites/183/2023/10/wood.runger.2016.pdf), *Annual Review of Psychology*
  - On Ariely & Wertenbroch (2002): [Data Colada on the failed replication](https://datacolada.org/138)
