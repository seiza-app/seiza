import {
  checkOpenPage,
  findRecipe,
  localAddresses,
  nextPassExpiry,
  otherSitesLevel,
  redirectRules,
  sessionProfile,
  siteFor,
  siteRecipes,
  type Profile,
  type Session,
  type SiteRecipe,
} from '@lock-in/core';
import { recipes } from '@lock-in/recipes';

// Redirects are declarativeNetRequest rules: the browser matches them itself, so no
// extension code runs on each request. The background keeps the rules in step with the
// session, and anything that needs them current right away asks it to sync.

const syncMessage = 'lock-in:sync-redirects';
const closeTabMessage = 'lock-in:close-tab';
export const passAlarm = 'lock-in:pass-expiry';

/**
 * The page a redirected visit lands on. The original URL goes after the # so the page can
 * send the user on. `expired` marks a tab sent back because its pass ran out.
 */
export function redirectPageUrl(target: string, options: { expired?: boolean } = {}): string {
  return `${browser.runtime.getURL('/redirect.html')}${options.expired ? '?expired' : ''}#${target}`;
}

/**
 * Rebuilds the redirect rules for the current session and schedules the next pass expiry, and
 * covers tabs already open on sites the profile keeps away without listing them.
 */
export async function syncRedirects(): Promise<void> {
  const [session, profileList, sites] = await Promise.all([
    activeSession.getValue(),
    profiles.getValue(),
    siteSettings.getValue(),
  ]);
  const now = Date.now();

  const addRules = redirectRules({
    session,
    profiles: profileList,
    recipes: siteRecipes(recipes, profileList),
    now,
  }).map((rule, index) => ({
    id: index + 1,
    priority: rule.priority,
    action:
      rule.action === 'redirect'
        ? {
            type: 'redirect' as Browser.declarativeNetRequest.RuleActionType,
            // \0 is the full original URL.
            redirect: { regexSubstitution: redirectPageUrl('\\0') },
          }
        : { type: 'allow' as Browser.declarativeNetRequest.RuleActionType },
    condition: {
      regexFilter: rule.regexFilter,
      // Page rules ignore case, like the rest of Seiza's path matching.
      isUrlFilterCaseSensitive: false,
      resourceTypes: ['main_frame' as Browser.declarativeNetRequest.ResourceType],
    },
  }));

  const existing = await browser.declarativeNetRequest.getDynamicRules();
  await browser.declarativeNetRequest.updateDynamicRules({
    removeRuleIds: existing.map((rule) => rule.id),
    addRules,
  });

  // Wake up when the next pass ends so its site gets redirected again.
  const expiry = nextPassExpiry(session, now);
  if (expiry) await browser.alarms.create(passAlarm, { when: expiry });
  else await browser.alarms.clear(passAlarm);

  await coverOtherSiteTabs(session, profileList, siteRecipes(recipes, profileList, sites), now);
}

/**
 * Tabs open on a site the profile doesn't list, from before the session started or a pass that
 * ran out, have no content script to cover them, so they're sent to the Seiza page from here.
 * Sites with a recipe, built in or added, have the content script (`known`). While every other
 * site is redirected, this follows the profile's "when a pass runs out" setting, where a pause
 * becomes the page too.
 */
async function coverOtherSiteTabs(
  session: Session | null,
  profileList: Profile[],
  known: readonly SiteRecipe[],
  now: number,
): Promise<void> {
  if (!session || otherSitesLevel(sessionProfile(profileList, session)) === 'allowed') return;
  const local = new RegExp(localAddresses, 'i');
  const tabs = await browser.tabs.query({ url: ['http://*/*', 'https://*/*'] });
  for (const tab of tabs) {
    if (tab.id === undefined || !tab.url || local.test(tab.url) || findRecipe(known, tab.url)) continue;
    const site = siteFor(recipes, tab.url);
    if (!site) continue;
    const check = checkOpenPage({ url: tab.url, session, profiles: profileList, recipes: [site], now });
    if (check.action !== 'none') {
      await browser.tabs.update(tab.id, { url: redirectPageUrl(tab.url, { expired: check.passExpired }) });
    }
  }
}

/** Asks the background to sync and waits until it has. */
export async function syncRedirectsNow(): Promise<void> {
  await browser.runtime.sendMessage({ type: syncMessage });
}

export function isSyncRequest(message: unknown): boolean {
  return (message as { type?: unknown } | null)?.type === syncMessage;
}

/** Asks the background to close the tab this is called from (content scripts can't). */
export async function closeThisTab(): Promise<void> {
  await browser.runtime.sendMessage({ type: closeTabMessage });
}

export function isCloseTabRequest(message: unknown): boolean {
  return (message as { type?: unknown } | null)?.type === closeTabMessage;
}
