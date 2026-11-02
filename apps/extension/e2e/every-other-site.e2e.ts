import { starterProfiles, type OtherSitesLevel, type Profile } from '@lock-in/core';
import type { Page } from 'puppeteer';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import {
  controlPage,
  launch,
  redirectPage,
  setStored,
  startSession,
  stored,
  storedWhen,
  text,
  wait,
  type TestBrowser,
} from './harness';

// A profile's "Every other site": what happens to every site it doesn't list. Keeping them away
// needs the browser's OK to reach every site, which the test build already has.

const listed = 'http://other.test/page';
const unlisted = 'http://elsewhere.test/';
// Built in, and not in Writing's list.
const linkedin = 'http://www.linkedin.test/feed';

describe('every other site', () => {
  let t: TestBrowser;
  let control: Page;
  const profileId = starterProfiles[0]!.id;

  /** Lets other.test through and sets every other site, for Writing. */
  async function allowlist(otherSites: OtherSitesLevel) {
    // Nothing's saved until something changes, so it starts from the starter profiles.
    const list: Profile[] = (await stored(control)).profiles ?? starterProfiles;
    await setStored(control, {
      profiles: list.map((p) =>
        p.id === profileId
          ? {
              ...p,
              otherSites,
              sites: [
                ...p.sites.filter((s) => s.recipeId !== 'site:other.test'),
                { recipeId: 'site:other.test', level: 'allowed' },
              ],
            }
          : p,
      ),
    });
  }

  beforeAll(async () => {
    t = await launch({ reach: ['*://*/*'] });
    control = await controlPage(t);
  });
  afterAll(() => t?.close());

  it('keeps every unlisted site away while restricted, and lets the listed ones through', async () => {
    await allowlist('restricted');
    const before = await t.browser.newPage();
    await before.goto(unlisted);
    await startSession(control, profileId);

    // A tab open from before the session has no content script, so the background covers it.
    await expect.poll(() => before.url(), { timeout: 10_000 }).toBe(redirectPage(t, unlisted));
    await before.waitForSelector('h1');
    expect(await text(before)).toContain('elsewhere.test is off for now');

    const tab = await t.browser.newPage();
    await tab.goto(listed);
    expect(tab.url()).toBe(listed);
    await tab.goto(linkedin);
    expect(tab.url()).toBe(redirectPage(t, linkedin));
    await tab.goto('http://news.unknown.test/today');
    expect(tab.url()).toBe(redirectPage(t, 'http://news.unknown.test/today'));

    await setStored(control, { activeSession: null });
    await tab.goto(unlisted);
    expect(tab.url()).toBe(unlisted);
    await Promise.all([before.close(), tab.close()]);
  });

  it('gives an unlisted site the wait, and lets it through with a pass, while redirected', async () => {
    await allowlist('redirected');
    await startSession(control, profileId);
    const tab = await t.browser.newPage();
    await tab.goto(unlisted);
    expect(tab.url()).toBe(redirectPage(t, unlisted));
    await tab.waitForSelector('.badge');
    expect(await text(tab)).toContain('try at elsewhere.test today');

    const session = (await stored(control)).activeSession!;
    await setStored(control, { activeSession: { ...session, passes: { 'site:elsewhere.test': Date.now() + 60_000 } } });
    await tab.goto(unlisted);
    expect(tab.url()).toBe(unlisted);
    await tab.goto('http://news.unknown.test/');
    expect(tab.url()).toBe(redirectPage(t, 'http://news.unknown.test/'));

    await setStored(control, { activeSession: null });
    await tab.close();
  });

  it('is set in settings, and only gets stricter during a Hard session', async () => {
    await allowlist('redirected');
    const options = await t.browser.newPage();
    await options.goto(`chrome-extension://${t.extensionId}/options.html`);
    const others = '[role="radiogroup"][aria-label="Every other site level"]';
    await options.waitForSelector(others);
    // A built-in site with no level set follows every other site, and shows it.
    expect(
      await options.$eval('[role="radiogroup"][aria-label="LinkedIn level"] [aria-checked="true"]', (b) => b.className),
    ).toContain('redirected');

    await options.click(`${others} button.restricted`);
    await storedWhen(control, (s) => s.profiles!.find((p) => p.id === profileId)!.otherSites === 'restricted');

    await setStored(control, {
      activeSession: {
        id: 'hard',
        profileId,
        startedAt: Date.now(),
        timer: { minutes: 30, onEnd: 'silent', showSinceDone: false, strictness: 'hard' },
      },
    });
    await options.waitForFunction(() => document.body.innerText.includes('sites can only get stricter'));
    await options.click(`${others} button.allowed`);
    // Adding a site looser than every other site counts as loosening too.
    await options.type('form.add input[aria-label="Add a site"]', 'calm.test');
    await options.keyboard.press('Enter');
    await wait(500);
    const profile = (await stored(control)).profiles!.find((p) => p.id === profileId)!;
    expect(profile.otherSites).toBe('restricted');
    expect(profile.sites.some((s) => s.recipeId === 'site:calm.test')).toBe(false);

    await setStored(control, { activeSession: null });
    await options.close();
  });
});
