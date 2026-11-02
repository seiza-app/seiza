import { ownSiteDomain, ownSitePattern, type Profile, type SiteSettings } from '@lock-in/core';
import { onUnmounted, ref, watchEffect, type Ref } from 'vue';

// Sites people add themselves (packages/core/src/sites.ts) need their OK before Seiza can
// redirect them or cover their tabs, one site at a time, so it never asks to read every site.
// The built-in sites are allowed when Seiza is installed. The browser only asks straight from a
// click, so `askToReach` has to come before anything in the click that waits.

const scriptId = 'lock-in-own-sites';

/** The addresses of every site the profiles have added, or that has settings of its own, once each. */
export function ownDomains(list: readonly Profile[], sites: readonly SiteSettings[] = []): string[] {
  const ids = [
    ...list.flatMap((profile) => profile.sites.map((site) => site.recipeId)),
    ...sites.map((s) => s.recipeId),
  ];
  return [...new Set(ids.flatMap((id) => ownSiteDomain(id) ?? []))];
}

/** Asks the browser to let Seiza reach these sites. Resolves to whether it may. */
export function askToReach(domains: string[]): Promise<boolean> {
  return browser.permissions.request({ origins: domains.map(ownSitePattern) });
}

/** Which of these sites Seiza may reach. */
export async function reachable(domains: readonly string[]): Promise<Set<string>> {
  const allowed = await Promise.all(
    domains.map((domain) => browser.permissions.contains({ origins: [ownSitePattern(domain)] })),
  );
  return new Set(domains.filter((_, i) => allowed[i]));
}

/** The sites Seiza may reach, kept current as the browser's permissions change. */
export function useReachable(domains: () => string[]): Ref<Set<string> | undefined> {
  const result = ref<Set<string>>();
  let checked = 0;
  const check = () => {
    const turn = ++checked;
    void reachable(domains()).then((set) => {
      if (turn === checked) result.value = set;
    });
  };
  watchEffect(check);
  browser.permissions.onAdded.addListener(check);
  browser.permissions.onRemoved.addListener(check);
  onUnmounted(() => {
    browser.permissions.onAdded.removeListener(check);
    browser.permissions.onRemoved.removeListener(check);
  });
  return result;
}

/**
 * Every site, which Seiza needs to reach to keep away the sites a profile doesn't list (its
 * "Every other site"). It's only asked for when that's set to anything but Allowed.
 */
const everySite = '*://*/*';

/** Asks the browser to let Seiza reach every site. Resolves to whether it may. */
export function askToReachEverySite(): Promise<boolean> {
  return browser.permissions.request({ origins: [everySite] });
}

/** Whether Seiza may reach every site, kept current as the browser's permissions change. */
export function useReachesEverySite(): Ref<boolean | undefined> {
  const result = ref<boolean>();
  const check = () => void browser.permissions.contains({ origins: [everySite] }).then((yes) => (result.value = yes));
  check();
  browser.permissions.onAdded.addListener(check);
  browser.permissions.onRemoved.addListener(check);
  onUnmounted(() => {
    browser.permissions.onAdded.removeListener(check);
    browser.permissions.onRemoved.removeListener(check);
  });
  return result;
}

/**
 * Runs the content script on the added sites Seiza may reach, as the manifest does on the
 * built-in ones, so their tabs are covered like any other. Called whenever the profiles, the site
 * settings or the permissions change, and when the browser starts.
 */
export async function syncOwnSiteScripts(): Promise<void> {
  const allowed = await reachable(ownDomains(await profiles.getValue(), await siteSettings.getValue()));
  const matches = [...allowed].map(ownSitePattern);

  const registered = await browser.scripting.getRegisteredContentScripts({ ids: [scriptId] });
  if (registered.length) await browser.scripting.unregisterContentScripts({ ids: [scriptId] });
  if (matches.length === 0) return;
  await browser.scripting.registerContentScripts([
    { id: scriptId, matches, js: ['content-scripts/content.js'], runAt: 'document_start' },
  ]);
}
