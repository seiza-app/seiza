import { describe, expect, it } from 'vitest';
import { findRecipe } from './recipe';
import { redirectRules } from './redirect';
import {
  hasRoomForOwnSite,
  maxOwnSites,
  ownSiteDomain,
  ownSiteId,
  siteDomain,
  siteFor,
  siteName,
  siteRecipes,
  sitesLooser,
  withSiteRemoved,
} from './sites';
import type { Profile, SiteRecipe, SiteRule } from './types';

const reddit: SiteRecipe = { id: 'reddit', name: 'Reddit', matches: ['*://*.reddit.com/*'], toggles: [] };

const profile = (sites: SiteRule[]): Profile => ({ id: 'focus', name: 'Focus', sites });

describe('siteDomain', () => {
  it('cuts addresses and pasted links down to the site', () => {
    expect(siteDomain('example.com')).toBe('example.com');
    expect(siteDomain('  https://www.Example.com/page?q=1#top ')).toBe('example.com');
    expect(siteDomain('http://news.ycombinator.com/item')).toBe('news.ycombinator.com');
    expect(siteDomain('example.com:8080/a')).toBe('example.com');
    expect(siteDomain('bbc.co.uk.')).toBe('bbc.co.uk');
  });

  it('turns away anything that is not a site', () => {
    for (const raw of ['', 'hello', 'localhost:3000', '192.168.1.1', 'mailto:me@example.com', 'exa mple.com', 'a..b']) {
      expect(siteDomain(raw), raw).toBeUndefined();
    }
    expect(siteDomain('file:///etc/passwd')).toBeUndefined();
    expect(siteDomain('chrome://extensions')).toBeUndefined();
  });
});

describe('own sites', () => {
  it('keeps the address in the id', () => {
    expect(ownSiteId('example.com')).toBe('site:example.com');
    expect(ownSiteDomain('site:example.com')).toBe('example.com');
    expect(ownSiteDomain('reddit')).toBeUndefined();
  });

  it('uses the built-in recipe for an address that has one', () => {
    expect(siteFor([reddit], 'https://old.reddit.com/r/all')?.id).toBe('reddit');
    expect(siteFor([reddit], 'example.com')).toEqual({
      id: 'site:example.com',
      name: 'example.com',
      matches: ['*://*.example.com/*'],
      toggles: [],
    });
    expect(siteFor([reddit], 'nonsense')).toBeUndefined();
  });

  it('lists each added site once, after the built-in ones, most specific first', () => {
    const profiles = [
      profile([{ recipeId: 'site:example.com', level: 'restricted' }]),
      { ...profile([{ recipeId: 'site:news.example.com', level: 'allowed' }]), id: 'other' },
      { ...profile([{ recipeId: 'site:example.com', level: 'redirected' }]), id: 'third' },
      { ...profile([{ recipeId: 'site:not a site', level: 'restricted' }]), id: 'bad' },
    ];
    const recipes = siteRecipes([reddit], profiles);
    expect(recipes.map((r) => r.id)).toEqual(['reddit', 'site:news.example.com', 'site:example.com']);
    expect(findRecipe(recipes, 'https://news.example.com/')?.id).toBe('site:news.example.com');
    expect(findRecipe(recipes, 'https://www.example.com/a')?.id).toBe('site:example.com');
    // A site tamed all the time is known even if no profile has it.
    const always = siteRecipes([reddit], [], [{ recipeId: 'site:calm.org', allTheTime: true }]);
    expect(always.map((r) => r.id)).toEqual(['reddit', 'site:calm.org']);
  });

  it('redirects an added site like any other', () => {
    const profiles = [profile([{ recipeId: 'site:example.com', level: 'redirected' }])];
    const rules = redirectRules({
      session: { id: 's', profileId: 'focus', startedAt: 0 },
      profiles,
      recipes: siteRecipes([reddit], profiles),
      now: 1,
    });
    expect(rules).toHaveLength(1);
    expect(new RegExp(rules[0]!.regexFilter).test('https://example.com/')).toBe(true);
    expect(new RegExp(rules[0]!.regexFilter).test('https://notexample.com/')).toBe(false);
  });

  it('names sites by their recipe or address', () => {
    expect(siteName([reddit], 'reddit')).toBe('Reddit');
    expect(siteName([reddit], 'site:example.com')).toBe('example.com');
  });

  it('takes a site off one profile', () => {
    const profiles = [profile([{ recipeId: 'site:example.com', level: 'restricted' }]), { ...profile([]), id: 'b' }];
    expect(withSiteRemoved(profiles, 'focus', 'site:example.com')[0]!.sites).toEqual([]);
  });

  it('caps how many sites a profile can add', () => {
    const many = Array.from({ length: maxOwnSites }, (_, i) => ({
      recipeId: `site:s${i}.com`,
      level: 'restricted' as const,
    }));
    expect(hasRoomForOwnSite(profile(many.slice(1)))).toBe(true);
    expect(hasRoomForOwnSite(profile(many))).toBe(false);
  });
});

describe('sitesLooser', () => {
  const looser = (a: SiteRule[], b: SiteRule[]) => sitesLooser({ sites: a }, { sites: b });
  const before: SiteRule[] = [
    { recipeId: 'youtube', level: 'tamed', paths: [{ path: '/shorts', level: 'restricted' }] },
    { recipeId: 'site:example.com', level: 'redirected' },
  ];

  it('allows anything stricter, and new sites', () => {
    expect(looser(before, before)).toBe(false);
    expect(
      looser(before, [
        { recipeId: 'youtube', level: 'redirected', paths: [{ path: '/shorts', level: 'restricted' }] },
        { recipeId: 'site:example.com', level: 'restricted' },
        { recipeId: 'site:new.com', level: 'redirected' },
      ]),
    ).toBe(false);
  });

  it('catches a site loosened or taken off', () => {
    expect(looser(before, [before[0]!, { recipeId: 'site:example.com', level: 'allowed' }])).toBe(true);
    expect(looser(before, [before[0]!])).toBe(true);
  });

  it('catches a page loosened, taken off, or opened inside a stricter site', () => {
    const youtube = (paths: SiteRule['paths']): SiteRule[] => [
      { recipeId: 'youtube', level: 'tamed', paths },
      before[1]!,
    ];
    expect(looser(before, youtube([{ path: '/shorts', level: 'redirected' }]))).toBe(true);
    expect(looser(before, youtube([]))).toBe(true);
    expect(looser(before, youtube([{ path: '/SHORTS', level: 'restricted' }]))).toBe(false);
    expect(
      looser(
        before,
        youtube([
          { path: '/shorts', level: 'restricted' },
          { path: '/shorts/abc', level: 'tamed' },
        ]),
      ),
    ).toBe(true);
  });

  describe('with every other site kept away', () => {
    const allowlist = (sites: SiteRule[], otherSites: Profile['otherSites'] = 'restricted') => ({ sites, otherSites });
    const docs: SiteRule = { recipeId: 'site:docs.com', level: 'allowed' };

    it('catches every other site loosened', () => {
      expect(sitesLooser(allowlist([docs]), allowlist([docs], 'redirected'))).toBe(true);
      expect(sitesLooser(allowlist([docs]), { sites: [docs] })).toBe(true);
      expect(sitesLooser(allowlist([docs], 'redirected'), allowlist([docs]))).toBe(false);
      expect(sitesLooser({ sites: [docs] }, allowlist([docs]))).toBe(false);
    });

    it('catches a site added at a looser level than every other site', () => {
      expect(sitesLooser(allowlist([docs]), allowlist([docs, { recipeId: 'reddit', level: 'tamed' }]))).toBe(true);
      expect(sitesLooser(allowlist([docs]), allowlist([docs, { recipeId: 'reddit', level: 'restricted' }]))).toBe(
        false,
      );
      expect(
        sitesLooser(
          allowlist([docs]),
          allowlist([docs, { recipeId: 'reddit', level: 'restricted', paths: [{ path: '/r/a', level: 'allowed' }] }]),
        ),
      ).toBe(true);
    });

    it('lets a site be taken off, since it then joins every other site', () => {
      expect(sitesLooser(allowlist([docs]), allowlist([]))).toBe(false);
      expect(sitesLooser(allowlist([docs], 'redirected'), allowlist([]))).toBe(false);
      const redirected: SiteRule = { recipeId: 'x', level: 'redirected' };
      expect(sitesLooser(allowlist([redirected]), allowlist([]))).toBe(false);
      expect(sitesLooser(allowlist([redirected], 'redirected'), allowlist([], 'redirected'))).toBe(false);
      const restricted: SiteRule = { recipeId: 'x', level: 'restricted' };
      expect(sitesLooser(allowlist([restricted], 'redirected'), allowlist([], 'redirected'))).toBe(true);
    });
  });
});
