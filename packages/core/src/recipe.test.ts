import { describe, expect, it } from 'vitest';
import { buildRecipeCss, cssForPage, isSafeSelector, resolveToggles } from './recipe';
import type { Profile, Session, SiteRecipe } from './types';

const recipe: SiteRecipe = {
  id: 'example',
  name: 'Example',
  matches: ['*://*.example.com/*'],
  toggles: [
    { id: 'hideFeed', label: 'Hide feed', selectors: ['#feed', '.trending'], default: true },
    { id: 'hideAds', label: 'Hide ads', selectors: ['ad-post'], default: false },
    { id: 'grayscale', label: 'Grayscale', effect: 'grayscale', default: false },
  ],
};

const session: Session = { id: 's1', profileId: 'focus', startedAt: 0 };

function profileWith(level: 'allowed' | 'tamed' | 'redirected', toggles?: Record<string, boolean>): Profile {
  return { id: 'focus', name: 'Focus', sites: [{ recipeId: 'example', level, toggles }] };
}

describe('isSafeSelector', () => {
  it('accepts ordinary selectors', () => {
    expect(isSafeSelector('shreddit-app[pagetype="home"] .main-container')).toBe(true);
    expect(isSafeSelector('#a > div:has(svg[icon-name="trend"]) ~ *')).toBe(true);
  });

  it('rejects anything that could break out of the rule', () => {
    expect(isSafeSelector('a } body { display: none')).toBe(false);
    expect(isSafeSelector('a; @import url(x)')).toBe(false);
    expect(isSafeSelector('a /* comment */')).toBe(false);
    expect(isSafeSelector('\\7b')).toBe(false);
    expect(isSafeSelector('   ')).toBe(false);
  });
});

describe('resolveToggles', () => {
  it('uses defaults unless the profile overrides them', () => {
    expect(resolveToggles(recipe, { recipeId: 'example', level: 'tamed', toggles: { grayscale: true } })).toEqual({
      hideFeed: true,
      hideAds: false,
      grayscale: true,
    });
  });
});

describe('resolveToggles with session overrides', () => {
  it('lets session changes win over the profile and defaults', () => {
    const rule = { recipeId: 'example', level: 'tamed' as const, toggles: { grayscale: true } };
    expect(resolveToggles(recipe, rule, { grayscale: false, hideAds: true })).toEqual({
      hideFeed: true,
      hideAds: true,
      grayscale: false,
    });
  });

  it('falls back to recipe defaults without a rule', () => {
    expect(resolveToggles(recipe, undefined)).toEqual({ hideFeed: true, hideAds: false, grayscale: false });
  });
});

describe('buildRecipeCss', () => {
  it('hides selectors from enabled toggles only', () => {
    const css = buildRecipeCss(recipe, { hideFeed: true, hideAds: false });
    expect(css).toContain('#feed');
    expect(css).toContain('.trending');
    expect(css).not.toContain('ad-post');
    expect(css).not.toContain('grayscale');
  });

  it('gives each selector a rule of its own, so one the browser rejects drops only itself', () => {
    const css = buildRecipeCss(recipe, { hideFeed: true }, [{ selector: 'div:has(> .clip)', label: 'Clip' }]);
    expect(css.split('\n\n')).toEqual([
      '#feed {\n  display: none !important;\n}',
      '.trending {\n  display: none !important;\n}',
      'div:has(> .clip) {\n  display: none !important;\n}',
    ]);
  });

  it('adds grayscale when its toggle is on', () => {
    expect(buildRecipeCss(recipe, { grayscale: true })).toContain('filter: grayscale(1)');
  });

  it('drops unsafe selectors', () => {
    const unsafe: SiteRecipe = {
      ...recipe,
      toggles: [{ id: 't', label: 'T', selectors: ['.ok', 'a } body { color: red'], default: true }],
    };
    const css = buildRecipeCss(unsafe, { t: true });
    expect(css).toContain('.ok');
    expect(css).not.toContain('color: red');
  });

  it('returns an empty string when nothing is enabled', () => {
    expect(buildRecipeCss(recipe, {})).toBe('');
  });

  it('hides picked parts too, dropping unsafe ones', () => {
    const css = buildRecipeCss(recipe, {}, [
      { selector: 'aside.trending', label: 'Trending' },
      { selector: '} body {', label: 'Bad' },
    ]);
    expect(css).toContain('aside.trending');
    expect(css).not.toContain('body');
  });
});

describe('cssForPage', () => {
  const page = 'https://www.example.com/';

  it('returns nothing without an active session', () => {
    expect(cssForPage({ url: page, session: null, profiles: [profileWith('tamed')], recipes: [recipe] })).toBe('');
  });

  it('tames a site all the time with no session, with its own toggles', () => {
    const always = (allTheTime: boolean) => [{ recipeId: 'example', allTheTime, toggles: { grayscale: true } }];
    const outside = cssForPage({
      url: page,
      session: null,
      profiles: [],
      recipes: [recipe],
      siteSettings: always(true),
    });
    expect(outside).toContain('#feed');
    expect(outside).toContain('grayscale(1)');
    expect(cssForPage({ url: page, session: null, profiles: [], recipes: [recipe], siteSettings: always(false) })).toBe(
      '',
    );
    // During a session, the session's profile decides.
    expect(
      cssForPage({
        url: page,
        session,
        profiles: [profileWith('allowed')],
        recipes: [recipe],
        siteSettings: always(true),
      }),
    ).toBe('');
  });

  it('tames a matching site in a tamed profile', () => {
    const css = cssForPage({
      url: page,
      session,
      profiles: [profileWith('tamed', { grayscale: true })],
      recipes: [recipe],
    });
    expect(css).toContain('#feed');
    expect(css).toContain('grayscale(1)');
  });

  it("hides the site's picked parts in a session and all the time", () => {
    const hidden = [{ selector: 'aside.trending', label: 'Trending' }];
    const options = { url: page, recipes: [recipe] };
    expect(
      cssForPage({
        ...options,
        session,
        profiles: [profileWith('tamed')],
        siteSettings: [{ recipeId: 'example', hidden }],
      }),
    ).toContain('aside.trending');
    expect(
      cssForPage({
        ...options,
        session: null,
        profiles: [],
        siteSettings: [{ recipeId: 'example', hidden, allTheTime: true }],
      }),
    ).toContain('aside.trending');
    // Not while the site isn't tamed.
    expect(
      cssForPage({
        ...options,
        session,
        profiles: [profileWith('allowed')],
        siteSettings: [{ recipeId: 'example', hidden }],
      }),
    ).toBe('');
  });

  it('applies session overrides on top of the profile', () => {
    const changed: Session = { ...session, overrides: { example: { hideFeed: false, grayscale: true } } };
    const css = cssForPage({ url: page, session: changed, profiles: [profileWith('tamed')], recipes: [recipe] });
    expect(css).not.toContain('#feed');
    expect(css).toContain('grayscale(1)');
  });

  it('leaves allowed sites and unmatched pages alone', () => {
    expect(cssForPage({ url: page, session, profiles: [profileWith('allowed')], recipes: [recipe] })).toBe('');
    expect(
      cssForPage({ url: 'https://other.org/', session, profiles: [profileWith('tamed')], recipes: [recipe] }),
    ).toBe('');
  });

  it('returns nothing when the session profile no longer exists', () => {
    expect(cssForPage({ url: page, session, profiles: [], recipes: [recipe] })).toBe('');
  });

  it('follows page rules', () => {
    const paged: Profile = {
      id: 'focus',
      name: 'Focus',
      sites: [{ recipeId: 'example', level: 'allowed', paths: [{ path: '/feed', level: 'tamed' }] }],
    };
    const css = (url: string) => cssForPage({ url, session, profiles: [paged], recipes: [recipe] });
    expect(css('https://www.example.com/feed/latest')).toContain('#feed');
    expect(css('https://www.example.com/settings')).toBe('');
  });
});
