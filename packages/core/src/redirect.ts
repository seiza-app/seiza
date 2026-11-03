import { findRule } from './edits';
import { matchPatternToRegex, parseHttpUrl } from './match';
import { levelFor, pathRegexSource } from './paths';
import { otherSitesLevel, sessionProfile } from './profiles';
import { findRecipe } from './recipe';
import { redirectSettingsFor } from './settings';
import { ownSiteDomain, ownSiteRecipe } from './sites';
import { cutChars } from './text';
import type { GateVisit, PassExpiry, Profile, Session, SiteLevel, SiteRecipe, WhyReason } from './types';

/** Longest intention we keep. */
export const maxIntentionLength = 200;

/** How long gate visits are kept. */
const visitRetentionDays = 90;
const day = 24 * 60 * 60 * 1000;

export function hasPass(session: Session | null, recipeId: string, now: number): boolean {
  return (session?.passes?.[recipeId] ?? 0) > now;
}

type Access = 'open' | 'redirected' | 'restricted';

/**
 * Whether a site, or one page of it, is kept from loading right now. Redirected pages
 * open up while the site has a pass; restricted ones stay shut until the session ends.
 */
export function siteAccess(options: {
  session: Session | null;
  profiles: readonly Profile[];
  recipeId: string;
  /** The page's path, so page rules apply. Leave it out for the site as a whole. */
  path?: string;
  now: number;
}): Access {
  const { session, profiles, recipeId, path, now } = options;
  if (!session) return 'open';
  return accessFor(levelFor(sessionProfile(profiles, session), recipeId, path), session, recipeId, now);
}

function accessFor(level: SiteLevel, session: Session, recipeId: string, now: number): Access {
  if (level === 'restricted') return 'restricted';
  if (level === 'redirected' && !hasPass(session, recipeId, now)) return 'redirected';
  return 'open';
}

/** A declarativeNetRequest rule, before the extension fills in where redirects go. */
export interface RedirectRule {
  regexFilter: string;
  /** Send the visit to the Seiza page, or let it load even though a broader rule redirects it. */
  action: 'redirect' | 'allow';
  /**
   * Higher wins. Every other site is 1. Sites rank above it, a site added by its address above any
   * shorter one it sits inside, and each site's page rules rank just above the site, longer paths
   * first.
   */
  priority: number;
}

/**
 * Every web address, for a profile that keeps every other site away. Addresses on this computer
 * or the local network (`localAddresses`) are let through.
 */
export const everySite = '^https?://.*$';

/**
 * This computer and the local network: localhost, private IP addresses and `.local` names. No
 * profile can list them (they aren't sites with an address), so they're never one of "every
 * other site".
 */
export const localAddresses =
  '^https?://(localhost|[^/:]+\\.(localhost|local)|127\\.[0-9.]+|10\\.[0-9.]+|192\\.168\\.[0-9.]+|172\\.(1[6-9]|2[0-9]|3[01])\\.[0-9.]+|\\[::1\\])(:[0-9]+)?([/?#].*)?$';

/**
 * The rules that keep sites and pages from loading during the active session: redirected
 * ones without a pass, and restricted ones. Page rules that open a page inside a closed
 * site become allow rules, so the browser applies the same "most specific wins" order. When
 * the profile keeps every other site away too, one rule covers them, and the sites it lets
 * through (listed ones that are open, and unlisted ones with a pass) get allow rules above it.
 */
export function redirectRules(options: {
  session: Session | null;
  profiles: readonly Profile[];
  recipes: readonly SiteRecipe[];
  now: number;
}): RedirectRule[] {
  const { session, profiles, recipes, now } = options;
  const profile = sessionProfile(profiles, session);
  if (!session || !profile) return [];

  const others = otherSitesLevel(profile);
  const othersClosed = accessFor(others, session, '', now) !== 'open';
  // Sites nobody listed that got a pass from the redirect page, so they can be let through.
  const passed = othersClosed
    ? Object.keys(session.passes ?? {}).flatMap((id) => {
        const domain = ownSiteDomain(id);
        return domain && !recipes.some((recipe) => recipe.id === id) ? [ownSiteRecipe(domain)] : [];
      })
    : [];

  const known = [...recipes, ...passed];
  const domains = known.flatMap((recipe) => ownSiteDomain(recipe.id) ?? []);
  // A site inside another ("mail.example.com" in "example.com") always gets a rule of its own, so
  // the wider site's rules never reach it.
  const inside = (recipe: SiteRecipe) => {
    const domain = ownSiteDomain(recipe.id);
    return !!domain && domains.some((other) => domain.endsWith(`.${other}`));
  };

  const sites = known.flatMap((recipe) => {
    const rule = findRule(profile, recipe.id);
    const nested = inside(recipe);
    // An unlisted site that's closed is already covered by the rule for every other site.
    if (!rule && !nested && (!othersClosed || accessFor(others, session, recipe.id, now) !== 'open')) return [];
    const closed = (level: SiteLevel) => accessFor(level, session, recipe.id, now) !== 'open';
    const pages = (rule?.paths ?? []).map((page) => ({ path: page.path, closed: closed(page.level) }));
    const siteClosed = closed(rule?.level ?? others);
    if (!siteClosed && !othersClosed && !nested && !pages.some((page) => page.closed)) return [];

    const regexes = (pathSource?: string) =>
      recipe.matches.flatMap((pattern) => matchPatternToRegex(pattern, pathSource) ?? []);
    const priority = sitePriority(recipe);
    // Otherwise an open site only needs a rule of its own when every other site is closed.
    const site: RedirectRule[] =
      siteClosed || othersClosed || nested
        ? regexes().map((regexFilter) => ({ regexFilter, action: siteClosed ? 'redirect' : 'allow', priority }))
        : [];
    return [
      ...site,
      ...pages.flatMap((page) =>
        regexes(`${pathRegexSource(page.path)}([?#].*)?`).map((regexFilter): RedirectRule => ({
          regexFilter,
          action: page.closed ? 'redirect' : 'allow',
          priority: priority + 1 + page.path.length,
        })),
      ),
    ];
  });
  if (!othersClosed) return sites;
  return [
    { regexFilter: everySite, action: 'redirect', priority: 1 },
    // At the same priority an allow rule beats a redirect, and a listed site still beats both.
    { regexFilter: localAddresses, action: 'allow', priority: 1 },
    ...sites,
  ];
}

/**
 * Where a site's rules rank. Sites someone added are found longest address first (`siteRecipes`),
 * so "mail.example.com" is a site of its own inside "example.com", and its rules beat the wider
 * site's, page rules included. Built-in sites never overlap each other or an added one: adding an
 * address on a built-in site gives the built-in site. Page paths are at most 200 characters, so a
 * site's page rules stay below the next longer address.
 */
function sitePriority(recipe: SiteRecipe): number {
  return 2 + (ownSiteDomain(recipe.id)?.length ?? 0) * 1000;
}

/** Gives the session a pass to one site, dropping any passes that have already run out. */
export function withPass(session: Session, recipeId: string, now: number, minutes: number): Session {
  const live = Object.entries(session.passes ?? {}).filter(([, until]) => until > now);
  return { ...session, passes: { ...Object.fromEntries(live), [recipeId]: now + minutes * 60_000 } };
}

/** When the next pass runs out, if any are still running. */
export function nextPassExpiry(session: Session | null, now: number): number | undefined {
  const upcoming = Object.values(session?.passes ?? {}).filter((until) => until > now);
  return upcoming.length > 0 ? Math.min(...upcoming) : undefined;
}

/** How many times the gate has been hit for a site since local midnight. */
export function attemptsToday(visits: readonly GateVisit[], recipeId: string, now: number): number {
  const midnight = new Date(now).setHours(0, 0, 0, 0);
  return visits.filter((visit) => visit.recipeId === recipeId && visit.at >= midnight).length;
}

/** Adds a visit and forgets ones older than the retention window. */
export function withGateVisit(visits: readonly GateVisit[], visit: GateVisit): GateVisit[] {
  const cutoff = visit.at - visitRetentionDays * day;
  return [...visits.filter((v) => v.at >= cutoff), visit];
}

/** Marks a visit as continued, keeping what the user said they came for. */
export function withGateVisitContinued(visits: readonly GateVisit[], visitId: string, intention: string): GateVisit[] {
  const trimmed = cutChars(intention.trim(), maxIntentionLength);
  return visits.map((visit) =>
    visit.id === visitId ? { ...visit, continued: true, ...(trimmed ? { intention: trimmed } : {}) } : visit,
  );
}

/** Records the one-tap answer to "why are you here?", or clears it. */
export function withGateVisitWhy(
  visits: readonly GateVisit[],
  visitId: string,
  why: WhyReason | undefined,
): GateVisit[] {
  return visits.map((visit) => {
    if (visit.id !== visitId) return visit;
    const { why: _previous, ...rest } = visit;
    return why ? { ...rest, why } : rest;
  });
}

/** What an already-open page on a redirected or restricted site should do right now. */
export type OpenPageCheck =
  | {
      action: 'none';
      /** When to check again, because a pass runs out then. */
      recheckAt?: number;
    }
  | {
      action: Exclude<PassExpiry, 'nothing'> | 'block';
      recipe: SiteRecipe;
      profile: Profile;
      /** True when the page had a pass that ran out, false when it was open before the session. */
      passExpired: boolean;
    };

/**
 * New visits to redirected and restricted sites never load: the browser redirects them.
 * Pages that were already open when the session started, or when a pass ran out, are
 * handled here. Restricted sites are always covered; redirected ones follow the profile's
 * "when a pass runs out" setting.
 */
export function checkOpenPage(options: {
  url: string | URL;
  session: Session | null;
  profiles: readonly Profile[];
  recipes: readonly SiteRecipe[];
  now: number;
}): OpenPageCheck {
  const { url, session, profiles, recipes, now } = options;
  const profile = sessionProfile(profiles, session);
  const recipe = findRecipe(recipes, url);
  const level = recipe ? levelFor(profile, recipe.id, parseHttpUrl(String(url))?.pathname) : 'allowed';
  if (!session || !profile || !recipe || (level !== 'redirected' && level !== 'restricted')) return { action: 'none' };

  if (level === 'restricted') return { action: 'block', recipe, profile, passExpired: false };
  if (hasPass(session, recipe.id, now)) return { action: 'none', recheckAt: session.passes![recipe.id] };

  const { onPassExpiry } = redirectSettingsFor(profile).gate;
  if (onPassExpiry === 'nothing') return { action: 'none' };
  return { action: onPassExpiry, recipe, profile, passExpired: session.passes?.[recipe.id] !== undefined };
}
