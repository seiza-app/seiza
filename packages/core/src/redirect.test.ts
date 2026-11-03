import { describe, expect, it } from 'vitest';
import {
  attemptsToday,
  checkOpenPage,
  hasPass,
  nextPassExpiry,
  redirectRules,
  siteAccess,
  withGateVisit,
  withGateVisitContinued,
  withGateVisitWhy,
  withPass,
} from './redirect';
import type { RedirectRule } from './redirect';
import type { GateVisit, Profile, Session, SiteRecipe } from './types';

const site = (id: string): SiteRecipe => ({ id, name: id, matches: [`*://*.${id}.com/*`], toggles: [] });
const recipes = [site('reddit'), site('x'), site('docs')];

const profiles: Profile[] = [
  {
    id: 'deep',
    name: 'Deep',
    sites: [
      { recipeId: 'reddit', level: 'redirected' },
      { recipeId: 'x', level: 'redirected' },
      { recipeId: 'docs', level: 'allowed' },
    ],
  },
];

const now = 1_000_000;
const minute = 60_000;
const session: Session = { id: 's1', profileId: 'deep', startedAt: 0 };

describe('redirectRules', () => {
  const blocks = (rules: RedirectRule[], url: string) => {
    const matching = rules.filter((rule) => new RegExp(rule.regexFilter, 'i').test(url));
    const top = matching.sort((a, b) => b.priority - a.priority)[0];
    return top?.action === 'redirect';
  };

  it("redirects the profile's redirected sites", () => {
    const rules = redirectRules({ session, profiles, recipes, now });
    expect(blocks(rules, 'https://www.reddit.com/r/all')).toBe(true);
    expect(blocks(rules, 'https://x.com/home')).toBe(true);
    expect(blocks(rules, 'https://docs.com/')).toBe(false);
  });

  it("follows the session's own copy of its profile, whatever the profile says now", () => {
    const loosened: Profile[] = [{ ...profiles[0]!, sites: [] }];
    const running = { ...session, rules: profiles[0] };
    expect(blocks(redirectRules({ session: running, profiles: loosened, recipes, now }), 'https://reddit.com/')).toBe(
      true,
    );
    expect(redirectRules({ session, profiles: loosened, recipes, now })).toEqual([]);
  });

  it('redirects nothing without a session or profile', () => {
    expect(redirectRules({ session: null, profiles, recipes, now })).toEqual([]);
    expect(redirectRules({ session, profiles: [], recipes, now })).toEqual([]);
  });

  it('leaves out sites with a running pass', () => {
    const passed = { ...session, passes: { reddit: now + minute } };
    expect(blocks(redirectRules({ session: passed, profiles, recipes, now }), 'https://reddit.com/')).toBe(false);
    expect(
      blocks(redirectRules({ session: passed, profiles, recipes, now: now + minute }), 'https://reddit.com/'),
    ).toBe(true);
  });

  describe('with page rules', () => {
    const paged: Profile[] = [
      {
        id: 'deep',
        name: 'Deep',
        sites: [
          {
            recipeId: 'reddit',
            level: 'redirected',
            paths: [
              { path: '/r/writing', level: 'allowed' },
              { path: '/r/writing/comments', level: 'restricted' },
            ],
          },
          {
            recipeId: 'x',
            level: 'tamed',
            paths: [
              { path: '/', level: 'redirected' },
              { path: '/explore', level: 'redirected' },
            ],
          },
        ],
      },
    ];
    const rules = (s: Session = session) => redirectRules({ session: s, profiles: paged, recipes, now });

    it('lets the most specific rule win', () => {
      expect(blocks(rules(), 'https://reddit.com/r/all')).toBe(true);
      expect(blocks(rules(), 'https://reddit.com/r/writing')).toBe(false);
      expect(blocks(rules(), 'https://reddit.com/r/Writing/?sort=new')).toBe(false);
      expect(blocks(rules(), 'https://reddit.com/r/writing/comments/abc')).toBe(true);
      expect(blocks(rules(), 'https://reddit.com/r/writingprompts')).toBe(true);
    });

    it('redirects only the listed pages of an open site', () => {
      expect(blocks(rules(), 'https://x.com/')).toBe(true);
      expect(blocks(rules(), 'https://x.com/?s=1')).toBe(true);
      expect(blocks(rules(), 'https://x.com/explore/tabs/news')).toBe(true);
      expect(blocks(rules(), 'https://x.com/home')).toBe(false);
      expect(blocks(rules(), 'https://x.com/someone/status/1')).toBe(false);
    });

    it('opens redirected pages with a pass, but not restricted ones', () => {
      const passed = { ...session, passes: { reddit: now + minute } };
      expect(blocks(rules(passed), 'https://reddit.com/r/all')).toBe(false);
      expect(blocks(rules(passed), 'https://reddit.com/r/writing/comments/abc')).toBe(true);
    });
  });
});

describe('sites inside other sites', () => {
  const own = (domain: string): SiteRecipe => ({
    id: `site:${domain}`,
    name: domain,
    matches: [`*://*.${domain}/*`],
    toggles: [],
  });
  // Sites someone added come longest address first, as `siteRecipes` lists them.
  const known = [...recipes, own('mail.example.com'), own('example.com')];
  const nested = (sites: Profile['sites'], otherSites?: Profile['otherSites']): Profile[] => [
    { id: 'deep', name: 'Deep', otherSites, sites },
  ];
  const rules = (list: Profile[]) => redirectRules({ session, profiles: list, recipes: known, now });
  // At the same priority, an allow rule wins over a redirect, as in the browser.
  const blocks = (list: RedirectRule[], url: string) => {
    const matching = list.filter((rule) => new RegExp(rule.regexFilter, 'i').test(url));
    const top = matching.sort((a, b) => b.priority - a.priority || (a.action === 'allow' ? -1 : 1))[0];
    return top?.action === 'redirect';
  };

  it('lets an open site inside a redirected one load', () => {
    const list = rules(
      nested([
        { recipeId: 'site:example.com', level: 'redirected' },
        { recipeId: 'site:mail.example.com', level: 'tamed' },
      ]),
    );
    expect(blocks(list, 'https://example.com/')).toBe(true);
    expect(blocks(list, 'https://www.example.com/a')).toBe(true);
    expect(blocks(list, 'https://mail.example.com/inbox')).toBe(false);
  });

  it('treats a site inside another, listed only on some other profile, as one of every other site', () => {
    const list = (otherSites?: Profile['otherSites']) =>
      rules(nested([{ recipeId: 'site:example.com', level: 'redirected' }], otherSites));
    expect(blocks(list(), 'https://mail.example.com/inbox')).toBe(false);
    expect(blocks(list('redirected'), 'https://mail.example.com/inbox')).toBe(true);
    expect(
      blocks(
        rules(nested([{ recipeId: 'site:example.com', level: 'allowed' }], 'restricted')),
        'https://mail.example.com/',
      ),
    ).toBe(true);
  });

  it('redirects a closed site inside an open one, even with every other site closed', () => {
    const list = rules(
      nested(
        [
          { recipeId: 'site:example.com', level: 'allowed' },
          { recipeId: 'site:mail.example.com', level: 'redirected' },
        ],
        'restricted',
      ),
    );
    expect(blocks(list, 'https://example.com/')).toBe(false);
    expect(blocks(list, 'https://mail.example.com/inbox')).toBe(true);
    expect(blocks(list, 'https://elsewhere.org/')).toBe(true);
  });

  it("keeps the wider site's page rules off the site inside it", () => {
    const list = rules(
      nested([
        { recipeId: 'site:example.com', level: 'allowed', paths: [{ path: '/', level: 'redirected' }] },
        { recipeId: 'site:mail.example.com', level: 'allowed', paths: [{ path: '/news', level: 'restricted' }] },
      ]),
    );
    expect(blocks(list, 'https://example.com/')).toBe(true);
    expect(blocks(list, 'https://mail.example.com/')).toBe(false);
    expect(blocks(list, 'https://mail.example.com/news')).toBe(true);
  });

  it('catches an address with a trailing dot', () => {
    const list = rules(nested([{ recipeId: 'site:example.com', level: 'redirected' }]));
    expect(blocks(list, 'https://example.com./')).toBe(true);
    expect(blocks(redirectRules({ session, profiles, recipes, now }), 'https://www.reddit.com./r/all')).toBe(true);
  });
});

describe('passes', () => {
  it('lasts the given number of minutes', () => {
    const passed = withPass(session, 'reddit', now, 5);
    expect(passed.passes).toEqual({ reddit: now + 5 * minute });
    expect(hasPass(passed, 'reddit', now + 5 * minute - 1)).toBe(true);
    expect(hasPass(passed, 'reddit', now + 5 * minute)).toBe(false);
    expect(hasPass(passed, 'x', now)).toBe(false);
    expect(hasPass(null, 'reddit', now)).toBe(false);
  });

  it('drops expired passes when adding one', () => {
    const old = { ...session, passes: { x: now - 1, reddit: now + minute } };
    expect(withPass(old, 'docs', now, 5).passes).toEqual({ reddit: now + minute, docs: now + 5 * minute });
  });

  it('reports the next expiry', () => {
    const passed = { ...session, passes: { x: now - 1, reddit: now + 2 * minute, docs: now + minute } };
    expect(nextPassExpiry(passed, now)).toBe(now + minute);
    expect(nextPassExpiry(session, now)).toBeUndefined();
    expect(nextPassExpiry(null, now)).toBeUndefined();
  });
});

describe('gate visits', () => {
  const today = new Date(2026, 9, 26, 15).getTime();
  const visit = (id: string, recipeId: string, at: number): GateVisit => ({ id, recipeId, at });

  it('counts attempts at a site since midnight', () => {
    const visits = [
      visit('a', 'reddit', new Date(2026, 9, 25, 23).getTime()),
      visit('b', 'reddit', new Date(2026, 9, 26, 0, 5).getTime()),
      visit('c', 'x', new Date(2026, 9, 26, 9).getTime()),
      visit('d', 'reddit', new Date(2026, 9, 26, 14).getTime()),
    ];
    expect(attemptsToday(visits, 'reddit', today)).toBe(2);
    expect(attemptsToday(visits, 'docs', today)).toBe(0);
  });

  it('forgets visits after 90 days', () => {
    const old = visit('old', 'reddit', today - 91 * 24 * 60 * minute);
    const recent = visit('recent', 'reddit', today - 89 * 24 * 60 * minute);
    expect(withGateVisit([old, recent], visit('new', 'x', today)).map((v) => v.id)).toEqual(['recent', 'new']);
  });

  it('records the intention when continuing', () => {
    const visits = [visit('a', 'reddit', today), visit('b', 'x', today)];
    expect(withGateVisitContinued(visits, 'a', '  find the thread about tents  ')).toEqual([
      { ...visits[0], continued: true, intention: 'find the thread about tents' },
      visits[1],
    ]);
  });

  it('leaves out empty intentions and trims long ones', () => {
    expect(withGateVisitContinued([visit('a', 'reddit', today)], 'a', '   ')[0]).not.toHaveProperty('intention');
    expect(withGateVisitContinued([visit('a', 'reddit', today)], 'a', 'x'.repeat(500))[0]!.intention).toHaveLength(200);
    const emoji = withGateVisitContinued([visit('a', 'reddit', today)], 'a', 'x'.repeat(199) + '🌙🌙');
    expect(emoji[0]!.intention).toBe('x'.repeat(199) + '🌙');
  });
});

describe('withGateVisitWhy', () => {
  const visits: GateVisit[] = [{ id: 'a', recipeId: 'reddit', at: now }];

  it('records, changes and clears the answer', () => {
    const bored = withGateVisitWhy(visits, 'a', 'bored');
    expect(bored[0]!.why).toBe('bored');
    expect(withGateVisitWhy(bored, 'a', 'stuck')[0]!.why).toBe('stuck');
    expect(withGateVisitWhy(bored, 'a', undefined)[0]).toEqual(visits[0]);
  });
});

describe('checkOpenPage', () => {
  const url = 'https://www.reddit.com/r/all';
  const withExpiry = (onPassExpiry: 'nothing' | 'pause' | 'redirect'): Profile[] => [
    { ...profiles[0]!, redirect: { gate: { onPassExpiry } } },
  ];

  it('pauses pages that were open before the session by default', () => {
    expect(checkOpenPage({ url, session, profiles, recipes, now })).toMatchObject({
      action: 'pause',
      recipe: { id: 'reddit' },
      passExpired: false,
    });
  });

  it('notes when a pass ran out', () => {
    const expired = { ...session, passes: { reddit: now - 1 } };
    expect(checkOpenPage({ url, session: expired, profiles, recipes, now })).toMatchObject({ passExpired: true });
  });

  it('follows the profile setting', () => {
    expect(checkOpenPage({ url, session, profiles: withExpiry('redirect'), recipes, now }).action).toBe('redirect');
    expect(checkOpenPage({ url, session, profiles: withExpiry('nothing'), recipes, now })).toEqual({ action: 'none' });
  });

  it('waits until a running pass ends', () => {
    const passed = { ...session, passes: { reddit: now + minute } };
    expect(checkOpenPage({ url, session: passed, profiles, recipes, now })).toEqual({
      action: 'none',
      recheckAt: now + minute,
    });
  });

  it('leaves other pages alone', () => {
    expect(checkOpenPage({ url, session: null, profiles, recipes, now })).toEqual({ action: 'none' });
    expect(checkOpenPage({ url: 'https://docs.com/', session, profiles, recipes, now })).toEqual({ action: 'none' });
    expect(checkOpenPage({ url: 'https://example.org/', session, profiles, recipes, now })).toEqual({ action: 'none' });
  });
});

describe('restricted sites', () => {
  const restricted: Profile[] = [{ ...profiles[0]!, sites: [{ recipeId: 'reddit', level: 'restricted' }] }];
  const passed = { ...session, passes: { reddit: now + minute } };

  it('stay shut even with a pass', () => {
    expect(siteAccess({ session: passed, profiles: restricted, recipeId: 'reddit', now })).toBe('restricted');
    expect(redirectRules({ session: passed, profiles: restricted, recipes, now })).toMatchObject([
      { action: 'redirect' },
    ]);
  });

  it('open up when the session ends', () => {
    expect(siteAccess({ session: null, profiles: restricted, recipeId: 'reddit', now })).toBe('open');
  });

  it('cover tabs that were already open, whatever the pass setting', () => {
    const nothing: Profile[] = [{ ...restricted[0]!, redirect: { gate: { onPassExpiry: 'nothing' } } }];
    expect(
      checkOpenPage({ url: 'https://reddit.com/', session: passed, profiles: nothing, recipes, now }),
    ).toMatchObject({
      action: 'block',
      recipe: { id: 'reddit' },
    });
  });
});

describe('siteAccess', () => {
  it('reports redirected sites until they have a pass', () => {
    expect(siteAccess({ session, profiles, recipeId: 'x', now })).toBe('redirected');
    expect(siteAccess({ session: { ...session, passes: { x: now + 1 } }, profiles, recipeId: 'x', now })).toBe('open');
    expect(siteAccess({ session, profiles, recipeId: 'docs', now })).toBe('open');
  });
});

describe('every other site', () => {
  const allowlist = (otherSites: Profile['otherSites']): Profile[] => [
    {
      id: 'deep',
      name: 'Deep',
      otherSites,
      sites: [
        { recipeId: 'docs', level: 'allowed', paths: [{ path: '/drafts', level: 'restricted' }] },
        { recipeId: 'x', level: 'redirected' },
        { recipeId: 'site:wiki.org', level: 'tamed' },
      ],
    },
  ];
  const known = [...recipes, { id: 'site:wiki.org', name: 'wiki.org', matches: ['*://*.wiki.org/*'], toggles: [] }];
  const rules = (otherSites: Profile['otherSites'], s: Session = session) =>
    redirectRules({ session: s, profiles: allowlist(otherSites), recipes: known, now });
  const blocks = (list: RedirectRule[], url: string) => {
    const matching = list.filter((rule) => new RegExp(rule.regexFilter, 'i').test(url));
    // At the same priority, an allow rule wins over a redirect, as in the browser.
    const top = matching.sort((a, b) => b.priority - a.priority || (a.action === 'allow' ? -1 : 1))[0];
    return top?.action === 'redirect';
  };

  it('keeps every unlisted site away, built-in ones too, and lets the listed ones through', () => {
    const list = rules('restricted');
    expect(blocks(list, 'https://example.com/')).toBe(true);
    expect(blocks(list, 'https://www.reddit.com/r/all')).toBe(true);
    expect(blocks(list, 'https://docs.com/doc/1')).toBe(false);
    expect(blocks(list, 'https://docs.com/drafts/2')).toBe(true);
    expect(blocks(list, 'https://en.wiki.org/a')).toBe(false);
    expect(blocks(list, 'https://x.com/home')).toBe(true);
    expect(siteAccess({ session, profiles: allowlist('restricted'), recipeId: 'reddit', now })).toBe('restricted');
    expect(siteAccess({ session, profiles: allowlist('redirected'), recipeId: 'site:a.com', now })).toBe('redirected');
  });

  it('never covers this computer or the local network', () => {
    const list = rules('restricted');
    for (const url of [
      'http://localhost:3000/',
      'http://app.localhost/',
      'http://127.0.0.1:8080/x',
      'http://192.168.1.10:8006/',
      'http://10.0.0.2/',
      'http://172.20.1.1/',
      'http://nas.local/',
    ]) {
      expect(blocks(list, url), url).toBe(false);
    }
    expect(blocks(list, 'http://172.32.1.1/')).toBe(true);
    expect(blocks(list, 'https://localhost.example.com/')).toBe(true);
  });

  it('lets an unlisted site through while it has a pass', () => {
    const passed = { ...session, passes: { 'site:example.com': now + minute, reddit: now + minute } };
    const list = rules('redirected', passed);
    expect(blocks(list, 'https://example.com/a')).toBe(false);
    expect(blocks(list, 'https://reddit.com/')).toBe(false);
    expect(blocks(list, 'https://example.org/')).toBe(true);
    // Restricted stays shut, pass or not.
    expect(blocks(rules('restricted', passed), 'https://example.com/a')).toBe(true);
  });

  it('adds nothing while every other site is allowed', () => {
    expect(rules('allowed')).toEqual(rules(undefined));
    expect(blocks(rules(undefined), 'https://example.com/')).toBe(false);
  });

  it('covers tabs open on an unlisted site', () => {
    const unlisted = [{ id: 'site:example.com', name: 'example.com', matches: ['*://*.example.com/*'], toggles: [] }];
    const check = (otherSites: Profile['otherSites']) =>
      checkOpenPage({ url: 'https://example.com/', session, profiles: allowlist(otherSites), recipes: unlisted, now });
    expect(check('restricted')).toMatchObject({ action: 'block' });
    expect(check('redirected')).toMatchObject({ action: 'pause', passExpired: false });
    expect(check(undefined)).toEqual({ action: 'none' });
  });
});
