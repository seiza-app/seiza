import { describe, expect, it } from 'vitest';
import { findPathRule, levelFor, normalizePath, pathMatches } from './paths';
import type { Profile } from './types';

describe('normalizePath', () => {
  it('tidies what people type', () => {
    expect(normalizePath(' shorts ')).toBe('/shorts');
    expect(normalizePath('/r/writing/')).toBe('/r/writing');
    expect(normalizePath('//r//writing')).toBe('/r/writing');
    expect(normalizePath('/')).toBe('/');
    expect(normalizePath('/*/reels')).toBe('/*/reels');
  });

  it('cuts pasted URLs down to their path', () => {
    expect(normalizePath('https://www.youtube.com/shorts/')).toBe('/shorts');
    expect(normalizePath('youtube.com/shorts')).toBe('/shorts');
    expect(normalizePath('https://www.tiktok.com')).toBe('/');
  });

  it('rejects anything that is not a plain path', () => {
    expect(normalizePath('/search?q=cats')).toBeUndefined();
    expect(normalizePath('/a#top')).toBeUndefined();
    expect(normalizePath('/two words')).toBeUndefined();
    expect(normalizePath('/<script>')).toBeUndefined();
    expect(normalizePath(`/${'a'.repeat(300)}`)).toBeUndefined();
  });
});

describe('pathMatches', () => {
  it('treats "/" as the home page only', () => {
    expect(pathMatches('/', '/')).toBe(true);
    expect(pathMatches('/', '/home')).toBe(false);
  });

  it('covers a path and everything under it, ignoring case', () => {
    expect(pathMatches('/shorts', '/shorts')).toBe(true);
    expect(pathMatches('/shorts', '/shorts/abc123')).toBe(true);
    expect(pathMatches('/shorts', '/Shorts/abc123')).toBe(true);
    expect(pathMatches('/shorts', '/shortsville')).toBe(false);
    expect(pathMatches('/shorts', '/watch')).toBe(false);
  });

  it('matches * within one part of the path', () => {
    expect(pathMatches('/*/reels', '/someone/reels')).toBe(true);
    expect(pathMatches('/*/reels', '/someone/reels/1')).toBe(true);
    expect(pathMatches('/*/reels', '/a/b/reels')).toBe(false);
  });

  it("doesn't treat regex characters specially", () => {
    expect(pathMatches('/a.b', '/a.b')).toBe(true);
    expect(pathMatches('/a.b', '/axb')).toBe(false);
  });
});

describe('levels with page rules', () => {
  const profile: Profile = {
    id: 'p',
    name: 'P',
    sites: [
      {
        recipeId: 'reddit',
        level: 'tamed',
        paths: [
          { path: '/r', level: 'redirected' },
          { path: '/r/writing', level: 'allowed' },
        ],
      },
    ],
  };

  it('picks the most specific rule', () => {
    expect(findPathRule(profile.sites[0]!.paths, '/r/writing/comments/1')?.path).toBe('/r/writing');
    expect(findPathRule(profile.sites[0]!.paths, '/user/me')).toBeUndefined();
  });

  it("falls back to the site's level", () => {
    expect(levelFor(profile, 'reddit', '/r/all')).toBe('redirected');
    expect(levelFor(profile, 'reddit', '/r/writing')).toBe('allowed');
    expect(levelFor(profile, 'reddit', '/settings')).toBe('tamed');
    expect(levelFor(profile, 'reddit')).toBe('tamed');
    expect(levelFor(profile, 'x', '/home')).toBe('allowed');
    expect(levelFor(undefined, 'reddit', '/')).toBe('allowed');
  });

  it('treats a site the profile does not list like every other site', () => {
    expect(levelFor({ ...profile, otherSites: 'restricted' }, 'x', '/home')).toBe('restricted');
    expect(levelFor({ ...profile, otherSites: 'redirected' }, 'site:a.com')).toBe('redirected');
    expect(levelFor({ ...profile, otherSites: 'restricted' }, 'reddit', '/settings')).toBe('tamed');
    // Tamed needs a recipe, so it can't be every other site's level.
    expect(levelFor({ ...profile, otherSites: 'tamed' as never }, 'x')).toBe('allowed');
  });
});
