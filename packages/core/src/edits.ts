import { maxPathRules, normalizePath } from './paths';
import { sessionProfile } from './profiles';
import { sitesLooser } from './sites';
import { isStarColor, newStarColor } from './sky';
import { endLock } from './timer';
import type { Profile, RedirectSettings, Session, SiteLevel, SiteRecipe, SiteRule } from './types';

// Pure, immutable edits to profiles and sessions. The extension (and later the desktop
// app) persists whatever these return.

export function findRule(profile: Profile | undefined, recipeId: string): SiteRule | undefined {
  return profile?.sites.find((site) => site.recipeId === recipeId);
}

/**
 * Sets a toggle for the current session only. Setting it back to the profile's value
 * removes the override, so the session only records real differences.
 */
export function withSessionToggle(
  session: Session,
  recipe: SiteRecipe,
  rule: SiteRule | undefined,
  toggleId: string,
  value: boolean,
): Session {
  const toggle = recipe.toggles.find((t) => t.id === toggleId);
  if (!toggle) return session;

  const baseline = rule?.toggles?.[toggleId] ?? toggle.default;
  const { [toggleId]: _previous, ...others } = session.overrides?.[recipe.id] ?? {};
  return withRecipeOverrides(session, recipe.id, value === baseline ? others : { ...others, [toggleId]: value });
}

/** Sets a toggle on a profile's rule for a site. Profiles without a rule for that site are left alone. */
export function withProfileToggle(
  profiles: readonly Profile[],
  profileId: string,
  recipeId: string,
  toggleId: string,
  value: boolean,
): Profile[] {
  return profiles.map((profile) =>
    profile.id !== profileId
      ? profile
      : {
          ...profile,
          sites: profile.sites.map((site) =>
            site.recipeId === recipeId ? { ...site, toggles: { ...site.toggles, [toggleId]: value } } : site,
          ),
        },
  );
}

/** Replaces how a profile handles redirected sites. */
export function withRedirectSettings(
  profiles: readonly Profile[],
  profileId: string,
  settings: RedirectSettings,
): Profile[] {
  return profiles.map((profile) => (profile.id === profileId ? { ...profile, redirect: settings } : profile));
}

/** Copies a session's overrides for one site into its profile, then clears them from the session. */
export function withSessionOverridesSaved(
  profiles: readonly Profile[],
  session: Session,
  recipeId: string,
): { profiles: Profile[]; session: Session } {
  const rule = findRule(sessionProfile(profiles, session), recipeId);
  const overrides = session.overrides?.[recipeId];
  if (!rule || !overrides) return { profiles: [...profiles], session };

  const save = (list: Profile[]) =>
    Object.entries(overrides).reduce(
      (saved, [toggleId, value]) => withProfileToggle(saved, session.profileId, recipeId, toggleId, value),
      list,
    );
  const withRules = session.rules ? { ...session, rules: save([session.rules])[0] } : session;
  return { profiles: save([...profiles]), session: withRecipeOverrides(withRules, recipeId, {}) };
}

/**
 * Makes an edit to the list of profiles in the running session's copy of its profile too, so a
 * change made here applies straight away. While the session is Hard, one that loosens its
 * sites is left out of the copy, as the desktop app does.
 */
export function withRulesEdited(session: Session, edit: (list: Profile[]) => Profile[], now: number): Session {
  const rules = session.rules && edit([session.rules]).find((p) => p.id === session.profileId);
  if (!session.rules || !rules || rules === session.rules) return session;
  if (endLock(session, now) === 'hard' && sitesLooser(session.rules.sites, rules.sites)) return session;
  return { ...session, rules };
}

/** Replaces one site's session overrides, dropping empty maps so the session stays tidy. */
function withRecipeOverrides(session: Session, recipeId: string, recipeOverrides: Record<string, boolean>): Session {
  const { [recipeId]: _old, ...otherRecipes } = session.overrides ?? {};
  const overrides =
    Object.keys(recipeOverrides).length > 0 ? { ...otherRecipes, [recipeId]: recipeOverrides } : otherRecipes;

  const { overrides: _previous, ...rest } = session;
  return Object.keys(overrides).length > 0 ? { ...rest, overrides } : rest;
}

/** Longest profile name we keep. */
export const maxProfileNameLength = 40;

/** Adds an empty profile at the end, with a star color no other profile has yet. Every site starts out allowed. */
export function withNewProfile(profiles: readonly Profile[], id: string, name: string): Profile[] {
  return [...profiles, { id, name: cleanName(name) || 'New profile', starColor: newStarColor(profiles), sites: [] }];
}

/** Copies a profile, placing the copy right after it. */
export function withProfileDuplicated(profiles: readonly Profile[], profileId: string, newId: string): Profile[] {
  const index = profiles.findIndex((p) => p.id === profileId);
  const original = profiles[index];
  if (!original) return [...profiles];

  const copy: Profile = structuredClone({ ...original, id: newId, name: cleanName(`${original.name} copy`) });
  return [...profiles.slice(0, index + 1), copy, ...profiles.slice(index + 1)];
}

export function withProfileRemoved(profiles: readonly Profile[], profileId: string): Profile[] {
  return withProfilesRemoved(profiles, [profileId]);
}

/** Removes several profiles at once. There's always at least one left: removing every profile does nothing. */
export function withProfilesRemoved(profiles: readonly Profile[], profileIds: readonly string[]): Profile[] {
  const kept = profiles.filter((p) => !profileIds.includes(p.id));
  return kept.length > 0 ? kept : [...profiles];
}

/** Renames a profile. Blank names are ignored. */
export function withProfileRenamed(profiles: readonly Profile[], profileId: string, name: string): Profile[] {
  const cleaned = cleanName(name);
  if (!cleaned) return [...profiles];
  return profiles.map((p) => (p.id === profileId ? { ...p, name: cleaned } : p));
}

/** Sets a profile's star color. Anything but a hex color like "#f5c86b" is ignored. */
export function withStarColor(profiles: readonly Profile[], profileId: string, color: string): Profile[] {
  if (!isStarColor(color)) return [...profiles];
  return profiles.map((p) => (p.id === profileId ? { ...p, starColor: color.toLowerCase() } : p));
}

/** Moves a profile earlier (negative offset) or later in the list, stopping at either end. */
export function withProfileMoved(profiles: readonly Profile[], profileId: string, offset: number): Profile[] {
  const from = profiles.findIndex((p) => p.id === profileId);
  if (from === -1) return [...profiles];

  const to = Math.min(profiles.length - 1, Math.max(0, from + offset));
  const list = [...profiles];
  const [moved] = list.splice(from, 1);
  list.splice(to, 0, moved!);
  return list;
}

/** Sets how a profile treats a site. Toggles are kept, so switching back to tamed restores them. */
export function withSiteLevel(
  profiles: readonly Profile[],
  profileId: string,
  recipeId: string,
  level: SiteLevel,
): Profile[] {
  return profiles.map((profile) => {
    if (profile.id !== profileId) return profile;
    const exists = profile.sites.some((site) => site.recipeId === recipeId);
    return {
      ...profile,
      sites: exists
        ? profile.sites.map((site) => (site.recipeId === recipeId ? { ...site, level } : site))
        : [...profile.sites, { recipeId, level }],
    };
  });
}

/**
 * Adds page rules to a site in a profile, or changes the level of ones it already has.
 * Paths are tidied first and invalid ones skipped. A profile with no rule for the site
 * gets one, left at Allowed, so only the pages change.
 */
export function withPathRules(
  profiles: readonly Profile[],
  profileId: string,
  recipeId: string,
  paths: readonly string[],
  level: SiteLevel,
): Profile[] {
  const cleaned = paths
    .flatMap((path) => normalizePath(path) ?? [])
    .filter((path, index, list) => list.findIndex((other) => samePath(other, path)) === index);
  if (cleaned.length === 0) return [...profiles];

  return withSite(profiles, profileId, recipeId, (site) => {
    const updated = (site.paths ?? []).map((rule) =>
      cleaned.some((path) => samePath(path, rule.path)) ? { ...rule, level } : rule,
    );
    const added = cleaned
      .filter((path) => !updated.some((rule) => samePath(rule.path, path)))
      .map((path) => ({ path, level }));
    return { ...site, paths: [...updated, ...added].slice(0, maxPathRules) };
  });
}

/** Removes a page rule, so the page follows the site's level again. */
export function withPathRuleRemoved(
  profiles: readonly Profile[],
  profileId: string,
  recipeId: string,
  path: string,
): Profile[] {
  return withSite(profiles, profileId, recipeId, (site) => {
    const { paths: _previous, ...rest } = site;
    const paths = (site.paths ?? []).filter((rule) => !samePath(rule.path, path));
    return paths.length > 0 ? { ...rest, paths } : rest;
  });
}

/** Paths match without regard to case, so "/Shorts" and "/shorts" are the same rule. */
function samePath(a: string, b: string): boolean {
  return a.toLowerCase() === b.toLowerCase();
}

/** Applies a change to one site's rule in one profile, adding an Allowed rule first if there isn't one. */
function withSite(
  profiles: readonly Profile[],
  profileId: string,
  recipeId: string,
  change: (site: SiteRule) => SiteRule,
): Profile[] {
  return profiles.map((profile) => {
    if (profile.id !== profileId) return profile;
    const exists = profile.sites.some((site) => site.recipeId === recipeId);
    const sites = exists ? profile.sites : [...profile.sites, { recipeId, level: 'allowed' as const }];
    return { ...profile, sites: sites.map((site) => (site.recipeId === recipeId ? change(site) : site)) };
  });
}

/** Puts the starter profiles back the way they shipped, keeping the user's own profiles. */
export function withStarterProfilesReset(profiles: readonly Profile[], starters: readonly Profile[]): Profile[] {
  const restored = profiles.map((profile) => starters.find((s) => s.id === profile.id) ?? profile);
  const missing = starters.filter((starter) => !profiles.some((p) => p.id === starter.id));
  return structuredClone([...restored, ...missing]);
}

function cleanName(name: string): string {
  return name.trim().replace(/\s+/g, ' ').slice(0, maxProfileNameLength);
}
