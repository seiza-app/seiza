import { describe, expect, it } from 'vitest';
import {
  withNewProfile,
  withOtherSites,
  withPathRuleRemoved,
  withPathRules,
  withProfileDuplicated,
  withProfileMoved,
  withProfileRemoved,
  withProfilesRemoved,
  withProfileRenamed,
  withProfileToggle,
  withRedirectSettings,
  withRulesEdited,
  withSessionOverridesSaved,
  withSessionToggle,
  withSiteLevel,
  withStarterProfilesReset,
} from './edits';
import { sessionProfile } from './profiles';
import { defaultRedirectSettings } from './settings';
import type { Profile, Session, SiteRecipe } from './types';

const recipe: SiteRecipe = {
  id: 'example',
  name: 'Example',
  matches: ['*://*.example.com/*'],
  toggles: [
    { id: 'hideFeed', label: 'Hide feed', selectors: ['#feed'], default: true },
    { id: 'grayscale', label: 'Grayscale', effect: 'grayscale', default: false },
  ],
};

const profiles: Profile[] = [
  { id: 'focus', name: 'Focus', sites: [{ recipeId: 'example', level: 'tamed', toggles: { grayscale: true } }] },
  { id: 'other', name: 'Other', sites: [{ recipeId: 'example', level: 'tamed' }] },
];

const rule = profiles[0]!.sites[0]!;
const session: Session = { id: 's1', profileId: 'focus', startedAt: 0 };

describe('withSessionToggle', () => {
  it('records a change for this session only', () => {
    const next = withSessionToggle(session, recipe, rule, 'hideFeed', false);
    expect(next.overrides).toEqual({ example: { hideFeed: false } });
    expect(session.overrides).toBeUndefined();
  });

  it('drops the override when set back to the profile value', () => {
    const changed = withSessionToggle(session, recipe, rule, 'grayscale', false);
    expect(changed.overrides).toEqual({ example: { grayscale: false } });
    expect(withSessionToggle(changed, recipe, rule, 'grayscale', true)).toEqual(session);
  });

  it('keeps overrides for other toggles and sites', () => {
    const start: Session = { ...session, overrides: { other: { x: true }, example: { grayscale: false } } };
    expect(withSessionToggle(start, recipe, rule, 'hideFeed', false).overrides).toEqual({
      other: { x: true },
      example: { grayscale: false, hideFeed: false },
    });
  });

  it('uses the recipe default when the profile has no rule', () => {
    expect(withSessionToggle(session, recipe, undefined, 'hideFeed', true)).toEqual(session);
  });

  it('ignores unknown toggles', () => {
    expect(withSessionToggle(session, recipe, rule, 'nope', true)).toBe(session);
  });
});

describe('withProfileToggle', () => {
  it('changes only the matching profile and site', () => {
    const next = withProfileToggle(profiles, 'other', 'example', 'grayscale', true);
    expect(next[1]!.sites[0]!.toggles).toEqual({ grayscale: true });
    expect(next[0]).toBe(profiles[0]);
    expect(profiles[1]!.sites[0]!.toggles).toBeUndefined();
  });

  it('leaves profiles without a rule for the site unchanged', () => {
    expect(withProfileToggle(profiles, 'focus', 'missing', 'hideFeed', false)).toEqual(profiles);
  });
});

describe('withSessionOverridesSaved', () => {
  it('moves session overrides into the profile', () => {
    const changed: Session = { ...session, overrides: { example: { hideFeed: false }, other: { x: true } } };
    const result = withSessionOverridesSaved(profiles, changed, 'example');
    expect(result.profiles[0]!.sites[0]!.toggles).toEqual({ grayscale: true, hideFeed: false });
    expect(result.session.overrides).toEqual({ other: { x: true } });
  });

  it('removes the overrides field once nothing is left', () => {
    const changed: Session = { ...session, overrides: { example: { hideFeed: false } } };
    expect(withSessionOverridesSaved(profiles, changed, 'example').session).toEqual(session);
  });

  it("saves them into the session's own copy of its profile too", () => {
    const changed: Session = { ...session, rules: profiles[0], overrides: { example: { hideFeed: false } } };
    const result = withSessionOverridesSaved(profiles, changed, 'example');
    expect(result.session.rules).toEqual(result.profiles[0]);
  });

  it('does nothing when the profile has no rule for the site', () => {
    const changed: Session = { ...session, overrides: { missing: { x: true } } };
    const result = withSessionOverridesSaved(profiles, changed, 'missing');
    expect(result.profiles).toEqual(profiles);
    expect(result.session).toBe(changed);
  });
});

describe('withRedirectSettings', () => {
  it('replaces the settings of one profile only', () => {
    const settings = { ...defaultRedirectSettings, plan: 'Write one line first.' };
    const next = withRedirectSettings(profiles, 'other', settings);
    expect(next[1]!.redirect).toEqual(settings);
    expect(next[0]).toBe(profiles[0]);
  });
});

describe('managing profiles', () => {
  const ids = (list: Profile[]) => list.map((p) => p.id);

  it('adds a new, empty profile at the end', () => {
    const next = withNewProfile(profiles, 'new', '  Reading   time ');
    expect(next.at(-1)).toEqual({ id: 'new', name: 'Reading time', starColor: '#f5c86b', sites: [] });
    expect(withNewProfile(profiles, 'new', '  ').at(-1)!.name).toBe('New profile');
  });

  it('duplicates a profile right after the original, without sharing data', () => {
    const next = withProfileDuplicated(profiles, 'focus', 'copy');
    expect(ids(next)).toEqual(['focus', 'copy', 'other']);
    expect(next[1]).toMatchObject({ name: 'Focus copy', sites: profiles[0]!.sites });
    expect(next[1]!.sites).not.toBe(profiles[0]!.sites);
  });

  it('removes a profile', () => {
    expect(ids(withProfileRemoved(profiles, 'focus'))).toEqual(['other']);
  });

  it('removes several profiles, but never the last one', () => {
    const three = [...profiles, { id: 'mine', name: 'Mine', sites: [] }];
    expect(ids(withProfilesRemoved(three, ['focus', 'mine']))).toEqual(['other']);
    expect(ids(withProfilesRemoved(three, ['focus', 'other', 'mine']))).toEqual(['focus', 'other', 'mine']);
    expect(ids(withProfilesRemoved(three, []))).toEqual(['focus', 'other', 'mine']);
  });

  it('renames a profile, ignoring blank names and trimming long ones', () => {
    expect(withProfileRenamed(profiles, 'focus', 'Deep focus')[0]!.name).toBe('Deep focus');
    expect(withProfileRenamed(profiles, 'focus', '   ')[0]!.name).toBe('Focus');
    expect(withProfileRenamed(profiles, 'focus', 'x'.repeat(100))[0]!.name).toHaveLength(40);
    // Cut by characters, so a long name never ends in half an emoji.
    expect(withProfileRenamed(profiles, 'focus', 'x'.repeat(39) + '🌙🌙')[0]!.name).toBe('x'.repeat(39) + '🌙');
  });

  it('moves a profile, stopping at either end', () => {
    expect(ids(withProfileMoved(profiles, 'other', -1))).toEqual(['other', 'focus']);
    expect(ids(withProfileMoved(profiles, 'other', 1))).toEqual(['focus', 'other']);
    expect(ids(withProfileMoved(profiles, 'focus', -5))).toEqual(['focus', 'other']);
  });

  it('sets a site level, keeping toggles', () => {
    const redirected = withSiteLevel(profiles, 'focus', 'example', 'redirected');
    expect(redirected[0]!.sites[0]).toEqual({ recipeId: 'example', level: 'redirected', toggles: { grayscale: true } });
    expect(withSiteLevel(redirected, 'focus', 'example', 'tamed')[0]!.sites[0]!.toggles).toEqual({ grayscale: true });
  });

  it('adds a rule for a site the profile did not mention', () => {
    expect(withSiteLevel(profiles, 'focus', 'x', 'restricted')[0]!.sites.at(-1)).toEqual({
      recipeId: 'x',
      level: 'restricted',
    });
  });

  it('sets what happens to every other site, keeping nothing for Allowed', () => {
    const restricted = withOtherSites(profiles, 'focus', 'restricted');
    expect(restricted[0]!.otherSites).toBe('restricted');
    expect(restricted[1]).toBe(profiles[1]);
    expect(withOtherSites(restricted, 'focus', 'allowed')[0]).toEqual(profiles[0]);
  });

  it("resets starter profiles and keeps the user's own", () => {
    const starters: Profile[] = [
      { id: 'focus', name: 'Focus', sites: [] },
      { id: 'fresh', name: 'Fresh', sites: [] },
    ];
    const edited = withProfileRenamed([...profiles, { id: 'mine', name: 'Mine', sites: [] }], 'focus', 'Renamed');
    const reset = withStarterProfilesReset(edited, starters);
    expect(ids(reset)).toEqual(['focus', 'other', 'mine', 'fresh']);
    expect(reset[0]).toEqual(starters[0]);
    expect(reset[0]).not.toBe(starters[0]);
  });
});

describe('page rules', () => {
  const paths = (list: Profile[], profileId = 'focus', recipeId = 'example') =>
    list.find((p) => p.id === profileId)!.sites.find((s) => s.recipeId === recipeId)?.paths;

  it('adds tidied paths, skipping invalid ones and duplicates', () => {
    const edited = withPathRules(profiles, 'focus', 'example', ['shorts/', '/Shorts', '/a?b', '/'], 'redirected');
    expect(paths(edited)).toEqual([
      { path: '/shorts', level: 'redirected' },
      { path: '/', level: 'redirected' },
    ]);
    expect(paths(edited, 'other')).toBeUndefined();
  });

  it('changes the level of a path it already has', () => {
    const added = withPathRules(profiles, 'focus', 'example', ['/shorts'], 'redirected');
    expect(paths(withPathRules(added, 'focus', 'example', ['/SHORTS'], 'restricted'))).toEqual([
      { path: '/shorts', level: 'restricted' },
    ]);
  });

  it('adds an Allowed rule for a site the profile did not mention', () => {
    const edited = withPathRules(profiles, 'focus', 'x', ['/explore'], 'redirected');
    expect(edited[0]!.sites.at(-1)).toEqual({
      recipeId: 'x',
      level: 'allowed',
      paths: [{ path: '/explore', level: 'redirected' }],
    });
  });

  it('adds the rule at the level every other site has, so only the pages change', () => {
    const allowlist = withOtherSites(profiles, 'focus', 'restricted');
    const edited = withPathRules(allowlist, 'focus', 'x', ['/explore'], 'allowed');
    expect(edited[0]!.sites.at(-1)).toEqual({
      recipeId: 'x',
      level: 'restricted',
      paths: [{ path: '/explore', level: 'allowed' }],
    });
  });

  it('removes a path, dropping the list once it is empty', () => {
    const added = withPathRules(profiles, 'focus', 'example', ['/shorts', '/explore'], 'redirected');
    const one = withPathRuleRemoved(added, 'focus', 'example', '/shorts');
    expect(paths(one)).toEqual([{ path: '/explore', level: 'redirected' }]);
    expect(withPathRuleRemoved(one, 'focus', 'example', '/explore')[0]!.sites[0]).toEqual(rule);
  });

  it('keeps page rules when the site level changes', () => {
    const added = withPathRules(profiles, 'focus', 'example', ['/shorts'], 'redirected');
    expect(paths(withSiteLevel(added, 'focus', 'example', 'allowed'))).toEqual([
      { path: '/shorts', level: 'redirected' },
    ]);
  });
});

describe("the running session's rules", () => {
  const running: Session = { ...session, rules: profiles[0] };

  it("come from the session's copy, or its profile when it has none", () => {
    const renamed = withProfileRenamed(profiles, 'focus', 'Renamed elsewhere');
    expect(sessionProfile(renamed, running)?.name).toBe('Focus');
    expect(sessionProfile(renamed, session)?.name).toBe('Renamed elsewhere');
    expect(sessionProfile(profiles, null)).toBeUndefined();
  });

  it('take edits to their profile, and only to it', () => {
    expect(withRulesEdited(running, (list) => withProfileRenamed(list, 'focus', 'Essays'), 0).rules?.name).toBe(
      'Essays',
    );
    expect(withRulesEdited(running, (list) => withProfileRenamed(list, 'other', 'Essays'), 0)).toBe(running);
    expect(withRulesEdited(session, (list) => withProfileRenamed(list, 'focus', 'Essays'), 0)).toBe(session);
  });

  it("don't loosen while the session is Hard", () => {
    const hard: Session = { ...running, timer: { minutes: 50, strictness: 'hard' } as Session['timer'] };
    const allow = (list: Profile[]) => withSiteLevel(list, 'focus', 'example', 'allowed');
    expect(withRulesEdited(hard, allow, 60_000)).toBe(hard);
    expect(withRulesEdited(running, allow, 60_000).rules).toEqual(allow([profiles[0]!])[0]);

    const allowlist: Session = { ...hard, rules: { ...profiles[0]!, otherSites: 'restricted' } };
    const loosen = (list: Profile[]) => withOtherSites(list, 'focus', 'redirected');
    const tighten = (list: Profile[]) => withOtherSites(list, 'focus', 'restricted');
    const addAllowed = (list: Profile[]) => withSiteLevel(list, 'focus', 'site:docs.com', 'allowed');
    expect(withRulesEdited(allowlist, loosen, 60_000)).toBe(allowlist);
    expect(withRulesEdited(allowlist, addAllowed, 60_000)).toBe(allowlist);
    expect(withRulesEdited(hard, tighten, 60_000).rules?.otherSites).toBe('restricted');
  });
});
