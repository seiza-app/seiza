import type { Page } from 'puppeteer';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import {
  controlPage,
  launch,
  openPopup,
  redirectPage,
  setProfileRedirect,
  setStored,
  startSession,
  stored,
  storedWhen,
  text,
  urls,
  visibility,
  wait,
  type TestBrowser,
} from './harness';

// One browser for the whole file; each test picks up where the last one left off.
describe('redirects and the redirect page', () => {
  let t: TestBrowser;
  let tab: Page;
  const gate = (target: string) => redirectPage(t, target);

  const gateText = () => tab.$eval('main', (main) => (main as HTMLElement).innerText);
  const continueButton = () =>
    tab.$eval('button.continue', (b) => ({ disabled: b.disabled, label: b.textContent!.trim() }));

  beforeAll(async () => {
    t = await launch();
    // A short fixed wait, and a reason to type, so the test knows what to expect.
    const control = await controlPage(t);
    await setProfileRedirect(control, 'deep-work', {
      gate: { requireReason: true, wait: { mode: 'fixed', seconds: 3 } },
    });
    await control.close();
    tab = await t.browser.newPage();
    await tab.goto(urls.other);
  });
  afterAll(() => t?.close());

  it('leaves redirected sites alone without a session', async () => {
    await tab.goto(urls.x);
    expect(tab.url()).toBe(urls.x);
  });

  it('sends redirected sites to the redirect page during a session', async () => {
    const popup = await openPopup(t, tab);
    const deepWork = await popup.$$('button.profile').then((buttons) => buttons[1]!);
    await deepWork.click();
    await storedWhen(popup, (storage) => storage.activeSession?.profileId === 'deep-work');

    await tab.goto(urls.reddit);
    await tab.waitForSelector('h1');
    expect(tab.url()).toBe(gate(urls.reddit));
    expect(await gateText()).toContain('Focus for 3 seconds first');
    expect(await gateText()).toContain('If you still want Reddit after that, go right ahead.');
    expect(await gateText()).toContain('First try at Reddit today');
    expect(await gateText()).toContain('· DEEP WORK');
  });

  it('counts each attempt', async () => {
    await tab.goto(urls.reddit);
    await tab.waitForSelector('h1');
    expect(await gateText()).toContain('2nd try at Reddit today');
  });

  it('holds the way through behind the wait', async () => {
    expect(await continueButton()).toEqual({ disabled: true, label: 'Go to Reddit' });
  });

  it('goes on to the site with a pass once the wait is up', async () => {
    await tab.type('textarea', 'Check the camping thread');
    await tab.waitForFunction(() => !document.querySelector<HTMLButtonElement>('button.continue')!.disabled, {
      timeout: 15_000,
    });
    expect(await gateText()).toContain("Time's up. If you still want Reddit, go right ahead.");

    await Promise.all([tab.waitForNavigation(), tab.click('button.continue')]);
    expect(tab.url()).toBe(urls.reddit);
    // Redirected, not tamed, so a pass shows the site as it is.
    expect(await visibility(tab, ['feed'])).toEqual({ feed: true, grayscale: false });

    const storage = await stored(await openPopup(t, tab));
    expect(storage.activeSession?.passes?.reddit).toBeGreaterThan(Date.now() + 4 * 60_000);
    expect(storage.gateVisits).toHaveLength(2);
    // Going on ends the wait, so the next trip after the pass waits again.
    expect(storage.pageWaits?.reddit).toBeUndefined();
    expect(storage.gateVisits?.[1]).toMatchObject({
      recipeId: 'reddit',
      destination: 'page',
      continued: true,
      intention: 'Check the camping thread',
    });
  });

  it('shows the pass in the popup', async () => {
    const popup = await openPopup(t, tab);
    expect(await text(popup)).toContain('Reddit is redirected in Deep Work · pass ends in 5 min.');
  });

  it('only lets that site through', async () => {
    await tab.goto(urls.reddit);
    expect(tab.url()).toBe(urls.reddit);
    await tab.goto(urls.x);
    expect(tab.url()).toBe(gate(urls.x));
  });

  it('goes back to the previous page from "Not now"', async () => {
    await tab.waitForSelector('button.back');
    await Promise.all([tab.waitForNavigation(), tab.click('button.back')]);
    expect(tab.url()).toBe(urls.reddit);
  });

  it('redirects the site again once the pass runs out', async () => {
    const popup = await openPopup(t, tab);
    const { activeSession } = await stored(popup);
    await setStored(popup, { activeSession: { ...activeSession, passes: { reddit: Date.now() + 1500 } } });

    // A real wait: the background wakes up for the pass running out by itself.
    await wait(3000);
    await tab.goto(urls.reddit);
    expect(tab.url()).toBe(gate(urls.reddit));
  });

  it('stops redirecting when the session ends', async () => {
    const popup = await openPopup(t, tab);
    await popup.click('button.end');
    await storedWhen(popup, (storage) => !storage.activeSession);

    await tab.goto(urls.reddit);
    expect(tab.url()).toBe(urls.reddit);
    await tab.goto(urls.x);
    expect(tab.url()).toBe(urls.x);
  });
});

describe('redirects from other sites', () => {
  let t: TestBrowser;

  beforeAll(async () => {
    t = await launch();
    const control = await controlPage(t);
    await startSession(control, 'deep-work');
  });
  afterAll(() => t?.close());

  // Chrome only lets a visit be redirected to an extension page if the page it came from
  // may load it. Links to Reddit live on every kind of site.
  it('catches a link to a redirected site from anywhere', async () => {
    const tab = await t.browser.newPage();
    await tab.goto(urls.other);
    await tab.evaluate((href) => {
      const link = Object.assign(document.createElement('a'), { href, id: 'link', textContent: 'Reddit' });
      document.body.append(link);
    }, urls.reddit);
    await Promise.all([tab.waitForNavigation(), tab.click('#link')]);
    await tab.waitForSelector('main');
    expect(tab.url()).toBe(redirectPage(t, urls.reddit));
  });

  it("doesn't let websites detect Seiza by loading its pages", async () => {
    const tab = await t.browser.newPage();
    await tab.goto(urls.other);
    const loaded = await tab.evaluate(
      (id) =>
        fetch(`chrome-extension://${id}/redirect.html`).then(
          () => true,
          () => false,
        ),
      t.extensionId,
    );
    expect(loaded).toBe(false);
  });
});
