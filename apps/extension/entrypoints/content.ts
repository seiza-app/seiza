import {
  allTheTimeToggles,
  buildRecipeCss,
  checkOpenPage,
  cssForPage,
  endLock,
  findRecipe,
  findRule,
  levelFor,
  maxHiddenParts,
  pagePathAttribute,
  resolveToggles,
  sessionProfile,
  siteAccess,
  siteRecipes,
  siteSettingsFor,
  type Profile,
  type Session,
  type SiteRecipe,
  type SiteSettings,
} from '@lock-in/core';
import { recipes } from '@lock-in/recipes';
import type { Picker } from '@/utils/picker';

// Listed here for the built-in sites. The background adds sites people add themselves, once
// they've let Seiza reach them (ownSites.ts).
export default defineContentScript({
  matches: recipes.flatMap((recipe) => recipe.matches),
  runAt: 'document_start',
  main(ctx) {
    // One <style> element holds this page's recipe CSS. It's filled in, emptied, or
    // updated as sessions start and stop, so tabs that are already open react immediately.
    const style = document.createElement('style');
    style.dataset.lockIn = '';
    document.documentElement.append(style);

    // The session, profiles and site settings are kept here and updated by storage events, so
    // moving between pages doesn't read storage.
    let session: Session | null = null;
    let profileList: Profile[] = [];
    let sites: SiteSettings[] = [];
    let known = recipes;
    let url = new URL(location.href);

    // While the element picker is open (from the popup), it decides what's hidden of the parts
    // picked on this site, in a session or not, and which of its toggles are flipped, so you see
    // what Done would save before saving it.
    let picking: { recipeId: string; toggles: Record<string, boolean> } | undefined;
    let picker: Picker | undefined;
    const pickStyle = document.createElement('style');
    pickStyle.dataset.lockInPicker = '';

    let removeOverlay: (() => void) | undefined;
    let overlayKey = '';
    let recheck: number | undefined;

    const savedWhilePicking = () => (picking && siteSettingsFor(sites, picking.recipeId)?.hidden) ?? [];
    // The site settings and session as the page's own stylesheet sees them: while picking,
    // without the parts the picker is showing or hiding itself, and with its toggles flipped.
    // Flips go to whichever of the two tames the page (pickerToggles), the other ignores them.
    const tamingSites = () =>
      picking
        ? sites.map((site) =>
            site.recipeId === picking!.recipeId
              ? { ...site, hidden: undefined, toggles: { ...site.toggles, ...picking!.toggles } }
              : site,
          )
        : sites;
    const tamingSession = (): Session | null => {
      if (!picking || !session || !Object.keys(picking.toggles).length) return session;
      const id = picking.recipeId;
      return {
        ...session,
        overrides: { ...session.overrides, [id]: { ...session.overrides?.[id], ...picking.toggles } },
      };
    };

    /** The toggles taming this page right now, for the picker: the session's, or the site's own. */
    const pickerToggles = (recipe: SiteRecipe) => {
      const list = (values: Record<string, boolean>) =>
        recipe.toggles.map((toggle) => ({ id: toggle.id, label: toggle.label, on: values[toggle.id] ?? false }));
      if (session) {
        const profile = sessionProfile(profileList, session);
        if (levelFor(profile, recipe.id, url.pathname) !== 'tamed') return undefined;
        const values = resolveToggles(recipe, findRule(profile, recipe.id), session.overrides?.[recipe.id]);
        return { when: 'this session', list: list(values) };
      }
      const settings = siteSettingsFor(sites, recipe.id);
      return settings?.allTheTime
        ? { when: 'all the time', list: list(allTheTimeToggles(recipe, settings)) }
        : undefined;
    };

    // Hiding a playing video doesn't stop its sound (Twitch's front page plays a stream), so
    // anything hidden here is paused too: when it's hidden, and whenever it starts playing.
    const hiddenSelector = () =>
      [style, pickStyle]
        .flatMap((element) => [...(element.sheet?.cssRules ?? [])])
        .flatMap((rule) =>
          'selectorText' in rule && (rule as CSSStyleRule).style.display === 'none'
            ? [(rule as CSSStyleRule).selectorText]
            : [],
        )
        .join(', ');
    const quiet = (media: HTMLMediaElement, selector = hiddenSelector()) => {
      if (selector && !media.paused && media.closest(selector)) media.pause();
    };
    const quietHidden = () => {
      const selector = hiddenSelector();
      if (!selector) return;
      for (const media of document.querySelectorAll<HTMLMediaElement>('video, audio')) quiet(media, selector);
    };
    // Media events don't bubble, so this listens on the way down. While the pause cover is up,
    // nothing plays behind it, like the next video starting by itself.
    ctx.addEventListener(
      document,
      'play',
      (event) => {
        if (!(event.target instanceof HTMLMediaElement)) return;
        if (removeOverlay) event.target.pause();
        else quiet(event.target);
      },
      { capture: true },
    );

    const restyle = () => {
      style.textContent = cssForPage({
        url,
        session: tamingSession(),
        profiles: profileList,
        recipes: known,
        siteSettings: tamingSites(),
      });
      quietHidden();

      // Recipes can hide things on some pages only by matching the path kept on <html>.
      // It's only there while the page is tamed.
      if (style.textContent) document.documentElement.setAttribute(pagePathAttribute, url.pathname);
      else document.documentElement.removeAttribute(pagePathAttribute);
    };

    const apply = (navigated = false) => {
      picker?.refresh();
      restyle();

      // Single-page apps change pages without loading one, so the browser's redirect rules
      // never see it. Moving onto a redirected or restricted page is caught here instead.
      if (navigated) {
        const recipe = findRecipe(known, url);
        const access = recipe
          ? siteAccess({ session, profiles: profileList, recipeId: recipe.id, path: url.pathname, now: Date.now() })
          : 'open';
        if (access !== 'open') {
          // Let the site finish its own navigation first, so Back returns to the page before.
          const target = url.href;
          ctx.setTimeout(() => location.replace(redirectPageUrl(target)), 0);
          return;
        }
      }

      // Redirected and restricted pages only get this far if the tab was already open.
      const check = checkOpenPage({ url, session, profiles: profileList, recipes: known, now: Date.now() });
      clearTimeout(recheck);

      if (check.action === 'none') {
        removeOverlay?.();
        removeOverlay = undefined;
        overlayKey = '';
        if (check.recheckAt) recheck = ctx.setTimeout(() => apply(), check.recheckAt - Date.now() + 250);
      } else if (check.action === 'redirect') {
        location.replace(redirectPageUrl(url.href, { expired: check.passExpired }));
      } else {
        const restricted = check.action === 'block';
        const reason = restricted ? 'restricted' : check.passExpired ? 'expired' : 'open-before';
        // Only redraw the cover when what it says would change.
        const key = `${reason}:${check.profile.name}`;
        if (key === overlayKey) return;

        removeOverlay?.();
        overlayKey = key;
        removeOverlay = showPauseOverlay({
          siteName: check.recipe.name,
          profileName: check.profile.name,
          reason,
          onStillNeed: restricted ? undefined : () => location.assign(redirectPageUrl(url.href)),
          onClose: () => void closeThisTab(),
        });
      }
    };

    function startPicking(recipeId: string) {
      const recipe = known.find((r) => r.id === recipeId);
      if (!recipe) return false;
      picker?.close();
      document.documentElement.append(pickStyle);
      // The page's own stylesheet leaves the saved parts to the picker's preview from here on.
      picking = { recipeId, toggles: {} };
      picker = showPicker({
        siteName: recipe.name,
        saved: savedWhilePicking,
        locked: () => !!session && endLock(session, Date.now()) === 'hard',
        maxParts: maxHiddenParts,
        partFor: hiddenPartFor,
        pickable,
        toggles: () => pickerToggles(recipe),
        preview: (parts, toggles) => {
          pickStyle.textContent = buildRecipeCss({ ...recipe, toggles: [] }, {}, parts);
          if (picking && JSON.stringify(toggles) !== JSON.stringify(picking.toggles)) {
            picking.toggles = toggles;
            restyle();
          } else {
            quietHidden();
          }
        },
        onDone: async (change) => {
          await changeHiddenParts(recipeId, change);
          // Flips are saved where they were made: to the session, or to the site tamed all the time.
          for (const [toggleId, value] of Object.entries(change.toggles)) {
            if (session) await setSessionToggle(recipe, toggleId, value);
            else await setAllTheTimeToggle(recipeId, toggleId, value);
          }
          stopPicking();
        },
        onCancel: stopPicking,
      });
      apply();
      return true;
    }
    function stopPicking() {
      picker?.close();
      picker = undefined;
      picking = undefined;
      pickStyle.remove();
      apply();
    }
    // The popup's "Hide parts of this page" asks for the picker on this tab.
    const onMessage = (message: unknown, _sender: unknown, sendResponse: (started: boolean) => void) => {
      const ask = message as { type?: string; recipeId?: string } | undefined;
      if (ask?.type !== 'lock-in:pick' || !ask.recipeId) return;
      sendResponse(startPicking(ask.recipeId));
    };
    browser.runtime.onMessage.addListener(onMessage);

    void Promise.all([activeSession.getValue(), profiles.getValue(), siteSettings.getValue()]).then(
      ([s, p, settings]) => {
        session = s;
        profileList = p;
        sites = settings;
        known = siteRecipes(recipes, p, settings);
        apply();
      },
    );
    const unwatchSession = activeSession.watch((value) => {
      session = value;
      apply();
    });
    const unwatchProfiles = profiles.watch((value) => {
      profileList = value;
      known = siteRecipes(recipes, value, sites);
      apply();
    });
    const unwatchSites = siteSettings.watch((value) => {
      sites = value;
      known = siteRecipes(recipes, profileList, value);
      apply();
    });
    // Fired as single-page apps navigate, before the new page shows.
    ctx.addEventListener(window, 'wxt:locationchange', ({ newUrl }) => {
      const pageChanged = newUrl.pathname !== url.pathname;
      url = newUrl;
      if (pageChanged) apply(true);
    });
    // Timers in background tabs can run late, so check again whenever the tab comes back.
    ctx.addEventListener(document, 'visibilitychange', () => {
      if (document.visibilityState === 'visible') apply();
    });
    ctx.onInvalidated(() => {
      browser.runtime.onMessage.removeListener(onMessage);
      stopPicking();
      unwatchSession();
      unwatchProfiles();
      unwatchSites();
      style.remove();
      document.documentElement.removeAttribute(pagePathAttribute);
      removeOverlay?.();
    });
  },
});
