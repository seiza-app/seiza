import { isSafeSelector } from './recipe';
import { cutChars } from './text';
import type { HiddenPart, SiteSettings } from './types';

// What's set for a site outside any profile (Rust: SiteSettings in sites.rs), kept by the
// extension and synced with the app:
// - the parts of its page picked to hide with the element picker, hidden whenever the site is
//   tamed, in any profile's session or all the time, so they're picked once for every case;
// - whether it's tamed all the time, even with no session running, with toggles of its own.

/** Most parts of a page one site can have hidden, and the longest selector and name for one. */
export const maxHiddenParts = 50;
export const maxSelectorLength = 500;
export const maxHiddenLabelLength = 80;

export function siteSettingsFor(list: readonly SiteSettings[], recipeId: string): SiteSettings | undefined {
  return list.find((site) => site.recipeId === recipeId);
}

/**
 * Hides a part of a site's page, picked with the element picker. One already hidden, a selector
 * that isn't safe or too long, or one past the limit is left out.
 */
export function withHiddenPart(list: readonly SiteSettings[], recipeId: string, part: HiddenPart): SiteSettings[] {
  const selector = part.selector.trim();
  if (!isSafeSelector(selector) || selector.length > maxSelectorLength) return [...list];
  const label = cutChars(part.label.trim(), maxHiddenLabelLength);
  return withSite(list, recipeId, (site) => {
    const hidden = site.hidden ?? [];
    if (hidden.length >= maxHiddenParts || hidden.some((p) => p.selector === selector)) return site;
    return { ...site, hidden: [...hidden, { selector, label }] };
  });
}

/** Shows a hidden part of a site's page again. */
export function withHiddenPartRemoved(
  list: readonly SiteSettings[],
  recipeId: string,
  selector: string,
): SiteSettings[] {
  return withSite(list, recipeId, ({ hidden, ...rest }) => {
    const kept = (hidden ?? []).filter((part) => part.selector !== selector);
    return kept.length > 0 ? { ...rest, hidden: kept } : rest;
  });
}

/** Tames a site all the time, even with no session running, or stops. Its toggles are kept either way. */
export function withAllTheTime(list: readonly SiteSettings[], recipeId: string, on: boolean): SiteSettings[] {
  return withSite(list, recipeId, ({ allTheTime: _previous, ...rest }) => (on ? { ...rest, allTheTime: true } : rest));
}

/** Sets a toggle for a site tamed all the time. */
export function withAllTheTimeToggle(
  list: readonly SiteSettings[],
  recipeId: string,
  toggleId: string,
  value: boolean,
): SiteSettings[] {
  return withSite(list, recipeId, (site) => ({ ...site, toggles: { ...site.toggles, [toggleId]: value } }));
}

/**
 * Applies a change to one site's settings, adding them if there aren't any. A site left with
 * nothing set is dropped, so the list only holds sites that have something.
 */
function withSite(
  list: readonly SiteSettings[],
  recipeId: string,
  change: (site: SiteSettings) => SiteSettings,
): SiteSettings[] {
  const exists = list.some((site) => site.recipeId === recipeId);
  const all = exists ? list : [...list, { recipeId }];
  return all
    .map((site) => (site.recipeId === recipeId ? change(site) : site))
    .filter((site) => site.hidden?.length || site.allTheTime || Object.keys(site.toggles ?? {}).length);
}
