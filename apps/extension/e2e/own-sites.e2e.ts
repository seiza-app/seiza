import { starterProfiles, type Profile } from '@lock-in/core';
import type { Page } from 'puppeteer';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import {
  controlPage,
  launch,
  openPopup,
  redirectPage,
  setStored,
  startSession,
  stored,
  storedWhen,
  type TestBrowser,
} from './harness';

// Sites people add themselves, by address. The browser would ask to let Seiza reach each one;
// here the test build is already allowed to reach other.test and elsewhere.test, and not
// unreached.test, which is what a site added in the desktop app looks like before it's allowed.

const other = 'http://other.test/page';
const elsewhere = 'http://www.elsewhere.test/';

describe('sites people add', () => {
  let t: TestBrowser;
  let control: Page;

  const siteOf = async (profileId: string, recipeId: string) =>
    (await stored(control)).profiles?.find((p) => p.id === profileId)?.sites.find((s) => s.recipeId === recipeId);

  beforeAll(async () => {
    t = await launch({ reach: ['*://*.other.test/*', '*://*.elsewhere.test/*'] });
    control = await controlPage(t);
  });
  afterAll(() => t?.close());

  it('adds one by its address in settings, redirected', async () => {
    const options = await t.browser.newPage();
    await options.goto(`chrome-extension://${t.extensionId}/options.html`);
    await options.waitForSelector('form.add input[aria-label="Add a site"]');

    // The test build's Reddit is reddit.test.
    await options.type('form.add input[aria-label="Add a site"]', 'old.reddit.test');
    expect(await options.$eval('form.add .error', (e) => e.textContent)).toContain('Reddit is built in');

    await options.$eval('form.add input[aria-label="Add a site"]', (input) => ((input as HTMLInputElement).value = ''));
    await options.type('form.add input[aria-label="Add a site"]', 'https://www.Other.test/some/page');
    await options.keyboard.press('Enter');
    await storedWhen(control, (s) => !!s.profiles?.[0]?.sites.some((site) => site.recipeId === 'site:other.test'));
    expect(await siteOf(starterProfiles[0]!.id, 'site:other.test')).toEqual({
      recipeId: 'site:other.test',
      level: 'redirected',
    });
    await options.waitForFunction(() => document.body.innerText.includes('other.test'));
    await options.close();
  });

  it('covers a tab already open when the session starts, and redirects new visits', async () => {
    const tab = await t.browser.newPage();
    await tab.goto(other);
    await startSession(control, starterProfiles[0]!.id);

    // The content script runs on added sites once Seiza may reach them.
    const overlay = () => tab.evaluate(() => document.querySelector('lock-in-pause')?.shadowRoot?.textContent ?? '');
    await expect.poll(overlay, { timeout: 10_000 }).toContain('other.test');

    await tab.goto(other);
    await tab.waitForSelector('.badge');
    expect(tab.url()).toBe(redirectPage(t, other));
    expect(await tab.$eval('main', (m) => (m as HTMLElement).innerText)).toContain('First try at other.test today');
    await setStored(control, { activeSession: null });
    await tab.close();
  });

  it('keeps a site away from the popup during a session', async () => {
    const profileId = starterProfiles[0]!.id;
    const tab = await t.browser.newPage();
    await tab.goto(elsewhere);
    await startSession(control, profileId);

    const popup = await openPopup(t, tab);
    await popup.waitForSelector('main > .site .keep-away');
    expect(await popup.$eval('main > .site', (e) => (e as HTMLElement).innerText)).toContain('elsewhere.test');
    const [, restrict] = await popup.$$('.keep-away button');
    await restrict!.click();

    await storedWhen(control, (s) =>
      s.profiles!.some((p) => p.sites.some((site) => site.recipeId === 'site:elsewhere.test')),
    );
    expect(await siteOf(profileId, 'site:elsewhere.test')).toMatchObject({ level: 'restricted' });
    // The tab reloads, straight onto the page with no way through.
    await expect.poll(() => tab.url(), { timeout: 10_000 }).toBe(redirectPage(t, elsewhere));
    await setStored(control, { activeSession: null });
    await tab.close();
  });

  it("says when a site added elsewhere can't be reached yet, and can be taken off", async () => {
    const list: Profile[] = (await stored(control)).profiles!;
    const profileId = list[0]!.id;
    await setStored(control, {
      profiles: list.map((p) =>
        p.id === profileId
          ? { ...p, sites: [...p.sites, { recipeId: 'site:unreached.test', level: 'restricted' }] }
          : p,
      ),
    });

    const options = await t.browser.newPage();
    await options.goto(`chrome-extension://${t.extensionId}/options.html`);
    await options.waitForFunction(() => document.body.innerText.includes("Seiza can't reach unreached.test yet"));
    expect(await options.evaluate(() => document.body.innerText)).not.toContain("can't reach other.test");

    await options.click('button[aria-label="Remove unreached.test"]');
    await storedWhen(
      control,
      (s) => !s.profiles!.find((p) => p.id === profileId)!.sites.some((x) => x.recipeId === 'site:unreached.test'),
    );
    await options.close();
  });

  it("won't loosen sites during a Hard session", async () => {
    const profileId = starterProfiles[0]!.id;
    await setStored(control, {
      activeSession: {
        id: 'hard',
        profileId,
        startedAt: Date.now(),
        timer: { minutes: 30, onEnd: 'silent', showSinceDone: false, strictness: 'hard' },
      },
    });
    const options = await t.browser.newPage();
    await options.goto(`chrome-extension://${t.extensionId}/options.html`);
    await options.waitForFunction(() => document.body.innerText.includes('sites can only get stricter'));
    await options.click('button[aria-label="Remove other.test"]');
    // Making one stricter still works.
    await options.click('[role="radiogroup"][aria-label="X level"] button.restricted');
    await storedWhen(control, (s) => s.profiles![0]!.sites.find((x) => x.recipeId === 'x')?.level === 'restricted');
    expect(await siteOf(profileId, 'site:other.test')).toMatchObject({ level: 'redirected' });
    await setStored(control, { activeSession: null });
    await options.close();
  });

  it('lets a site inside a redirected one load, and keeps the wider one away', async () => {
    const list: Profile[] = (await stored(control)).profiles!;
    const profileId = list[0]!.id;
    await setStored(control, {
      profiles: list.map((p) =>
        p.id === profileId
          ? {
              ...p,
              sites: [
                ...p.sites.filter((x) => x.recipeId !== 'site:other.test'),
                { recipeId: 'site:other.test', level: 'redirected' },
                { recipeId: 'site:mail.other.test', level: 'tamed' },
              ],
            }
          : p,
      ),
    });
    await startSession(control, profileId);

    const tab = await t.browser.newPage();
    await tab.goto('http://mail.other.test/inbox');
    expect(tab.url()).toBe('http://mail.other.test/inbox');
    await tab.goto(other);
    expect(tab.url()).toBe(redirectPage(t, other));
    await setStored(control, { activeSession: null });
    await tab.close();
  });
});
