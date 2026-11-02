import { otherSitesLevel } from './profiles';
import type { PathRule, Profile, SiteLevel } from './types';

/** Longest page path we keep. */
export const maxPathLength = 200;

/** Most page rules one site can have. */
export const maxPathRules = 50;

/**
 * Tidies a page path the user typed: pasted URLs are cut down to their path, a leading
 * slash is added and a trailing one dropped. Returns undefined for anything that isn't a
 * plain path (queries, fragments, spaces, or characters that don't belong in a URL path).
 */
export function normalizePath(raw: string): string | undefined {
  let path = raw.trim();
  if (/^(https?:\/\/)?[^/\s]+\.[a-z]{2,}(\/|$)/i.test(path)) {
    path = path.replace(/^(https?:\/\/)?[^/]+/i, '');
  }
  if (!path.startsWith('/')) path = `/${path}`;
  path = path.replace(/\/{2,}/g, '/');
  if (path.length > 1) path = path.replace(/\/$/, '');

  if (path.length > maxPathLength) return undefined;
  if (!/^[A-Za-z0-9\-._~%!$&'()*+,;=:@/]+$/.test(path)) return undefined;
  return path;
}

/**
 * The regex source a page rule's path matches, without anchors. "/" is the home page
 * only; any other path also covers everything under it, so "/r/writing" includes its
 * posts. `*` matches within one part of the path, e.g. "/*\/reels" for every profile's
 * Reels tab. Only uses syntax RE2 supports, so it works in declarativeNetRequest too.
 */
export function pathRegexSource(path: string): string {
  if (path === '/') return '/?';
  const body = path
    .split('*')
    .map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    .join('[^/?#]*');
  return `${body}(/[^?#]*)?`;
}

export function pathMatches(path: string, pathname: string): boolean {
  return new RegExp(`^${pathRegexSource(path)}$`, 'i').test(pathname);
}

/**
 * The page rule that applies to a path, if any. When several match, the longest (most
 * specific) wins, so "/r/writing" beats "/r" and anything beats "/".
 */
export function findPathRule(rules: readonly PathRule[] | undefined, pathname: string): PathRule | undefined {
  return (rules ?? [])
    .filter((rule) => pathMatches(rule.path, pathname))
    .reduce<PathRule | undefined>(
      (best, rule) => (!best || rule.path.length > best.path.length ? rule : best),
      undefined,
    );
}

/**
 * How a profile treats one page of a site: the page rule that covers it, else the site's level,
 * else what the profile does with every other site.
 */
export function levelFor(profile: Profile | undefined, recipeId: string, pathname?: string): SiteLevel {
  const rule = profile?.sites.find((site) => site.recipeId === recipeId);
  if (!rule) return otherSitesLevel(profile);
  if (pathname === undefined) return rule.level;
  return findPathRule(rule.paths, pathname)?.level ?? rule.level;
}
