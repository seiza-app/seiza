import { starterProfiles, type FromDesktop, type Session, type Strictness, type ToDesktop } from '@lock-in/core';
import type { Page } from 'puppeteer';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { fakeDesktopApp, type FakeDesktopApp } from './fakeDesktopApp';
import {
  chromeExtensionId,
  controlPage,
  launch,
  openPopup,
  setStored,
  stored,
  text,
  until,
  urls,
  wait,
  type TestBrowser,
} from './harness';

const minute = 60_000;
const startedAt = Date.now() - 10 * minute;
const appSession: Session = {
  id: 'app-session',
  profileId: 'deep-work',
  startedAt,
  timer: { minutes: 25, onEnd: 'gentle', showSinceDone: false, strictness: 'gentle' },
};

const states = (messages: ToDesktop[]) =>
  messages.filter((m): m is Extract<ToDesktop, { type: 'state' }> => m.type === 'state');

/** Whether the extension says it's linked to the app, from session storage. */
const linked = (extensionPage: Page): Promise<boolean> =>
  extensionPage.evaluate(() =>
    (globalThis as unknown as { chrome: typeof browser }).chrome.storage.session
      .get('desktopLinked')
      .then((items) => items.desktopLinked === true),
  );

// One browser for the whole file, linked to a fake app; each test picks up where the last left off.
describe('the desktop app link', () => {
  let app: FakeDesktopApp;
  let t: TestBrowser;
  let control: Page;
  /** When the session last changed, by the newest state the extension sent the app. */
  const lastSessionChange = async () => states(await app.received()).at(-1)?.activeSession?.updatedAt ?? 0;

  beforeAll(async () => {
    // The app is already running a session, started at its end after the browser last synced.
    app = await fakeDesktopApp([
      { type: 'linked' },
      {
        type: 'state',
        profiles: { value: starterProfiles, updatedAt: 0 },
        activeSession: { value: appSession, updatedAt: startedAt },
      },
    ]);
    t = await launch({ nativeHost: app.host });
    control = await controlPage(t);
    await until('the link', () => linked(control));
  });
  afterAll(async () => {
    await t?.close();
    await app?.remove();
  });

  it('has the id the app allows', () => {
    expect(t.extensionId).toBe(chromeExtensionId);
  });

  it("picks up the app's session and timer", async () => {
    await expect.poll(async () => (await stored(control)).activeSession).toEqual(appSession);

    const tab = await t.browser.newPage();
    await tab.goto(urls.other);
    const popup = await openPopup(t, tab);
    const popupText = await text(popup);
    expect(popupText).toContain('APP LINKED');
    expect(popupText).toContain('LOCKED IN · DEEP WORK');
    expect(await popup.$eval('.elapsed', (el) => el.textContent)).toMatch(/^1[45]:\d\d$/);
    await tab.close();
  });

  it('says hello and sends its own state once linked', async () => {
    const received = await app.received();
    expect(received[0]).toEqual({ type: 'hello', protocol: 1, browser: 'chrome' });
    expect(states(received)[0]).toMatchObject({
      profiles: { updatedAt: 0 },
      activeSession: { value: null, updatedAt: 0 },
    });
  });

  it('reports gate visits', async () => {
    const tab = await t.browser.newPage();
    await tab.goto(urls.x);
    await tab.waitForSelector('main');
    await expect
      .poll(async () => (await app.received()).filter((m) => m.type === 'gateVisit').at(-1))
      .toMatchObject({ type: 'gateVisit', visit: { recipeId: 'x' } });
    await tab.close();
  });

  it('sends sessions ended in the browser to the app', async () => {
    const tab = await t.browser.newPage();
    await tab.goto(urls.other);
    const popup = await openPopup(t, tab);
    // Its timer is still running, so ending it waits out a short pause first.
    await popup.click('button.end');
    expect(await popup.$eval('button.confirm', (button) => button.disabled)).toBe(true);
    await popup.waitForSelector('button.confirm:not([disabled])', { timeout: 15_000 });
    const endedAt = Date.now();
    await popup.click('button.confirm');

    // The first state the extension sent had no session either, so wait for one sent since.
    await expect.poll(() => lastSessionChange()).toBeGreaterThanOrEqual(endedAt);
    const last = states(await app.received()).at(-1)!;
    expect(last.activeSession?.value).toBeNull();
    expect(last.profiles).toBeUndefined();
    await tab.close();
  });

  it('applies sessions started in the app', async () => {
    const timer = { ...appSession.timer!, minutes: 0 };
    const session = { ...appSession, id: 'second', profileId: 'writing', startedAt: Date.now(), timer };
    await app.send({
      type: 'state',
      profiles: { value: starterProfiles, updatedAt: 0 },
      activeSession: { value: session, updatedAt: Date.now() },
    });
    await expect.poll(async () => (await stored(control)).activeSession).toEqual(session);

    // The app's own change doesn't bounce back to it as a new one. An echo would follow the
    // change straight away, so give it a moment to show up.
    await wait(500);
    const echoed = states(await app.received()).filter((m) => m.activeSession?.value?.id === 'second');
    expect(echoed).toEqual([]);
  });

  it('ignores changes older than its own', async () => {
    await app.send({
      type: 'state',
      profiles: { value: [], updatedAt: 0 },
      activeSession: { value: null, updatedAt: startedAt },
    });
    // Nothing should change, so all there is to wait for is a moment.
    await wait(500);
    const storage = await stored(control);
    expect(storage.activeSession?.id).toBe('second');
    expect(storage.profiles).toBeUndefined();
  });

  it("sends its profiles with the app's list they were made from", async () => {
    const sentAt = Date.now();
    const fromApp = starterProfiles.map((p, i) => (i === 0 ? { ...p, name: 'Essays' } : p));
    await app.send({
      type: 'state',
      profiles: { value: fromApp, updatedAt: sentAt },
      activeSession: { value: null, updatedAt: 0 },
    });
    await expect.poll(async () => (await stored(control)).profiles?.[0]?.name).toBe('Essays');

    // So the app can keep what it changed after that, rather than this list undoing it.
    await setStored(control, { profiles: fromApp.map((p, i) => (i === 0 ? { ...p, name: 'Notes' } : p)) });
    await expect.poll(async () => states(await app.received()).at(-1)?.profiles?.value[0]?.name).toBe('Notes');
    expect(states(await app.received()).at(-1)?.profilesBasedOn).toBe(sentAt);
  });

  it('sends a profile and its running session edited together', async () => {
    // Editing the running session's profile saves both in one go, as the actions do.
    const storage = await stored(control);
    const before = storage.syncedAt!;
    const edited = storage.profiles!.map((p) => (p.id === 'writing' ? { ...p, name: 'Drafts' } : p));
    const session = { ...storage.activeSession!, rules: edited.find((p) => p.id === 'writing') };
    await setStored(control, { profiles: edited, activeSession: session });

    // Both go to the app, each newer than before.
    const lastSent = async () => {
      const sent = states(await app.received());
      return { profiles: sent.findLast((m) => m.profiles)?.profiles, session: sent.findLast((m) => m.activeSession) };
    };
    await expect
      .poll(async () => (await lastSent()).profiles?.value.find((p) => p.id === 'writing')?.name)
      .toBe('Drafts');
    await expect.poll(async () => (await lastSent()).session?.activeSession?.value?.rules?.name).toBe('Drafts');
    const { profiles: sentProfiles, session: sentSession } = await lastSent();
    expect(sentProfiles!.updatedAt).toBeGreaterThan(before.profiles);
    expect(sentSession!.activeSession!.updatedAt).toBeGreaterThan(before.activeSession);

    // And neither stamp undid the other's.
    const times = (await stored(control)).syncedAt!;
    expect(times.profiles).toBe(sentProfiles!.updatedAt);
    expect(times.activeSession).toBe(sentSession!.activeSession!.updatedAt);
  });

  it('carries on by itself once the app goes away', async () => {
    await app.send({ type: 'unlinked' });
    await expect.poll(() => linked(control)).toBe(false);
    const before = (await stored(control)).syncedAt!.activeSession;
    const tab = await t.browser.newPage();
    await tab.goto(urls.other);
    const popup = await openPopup(t, tab);
    expect(await text(popup)).not.toContain('APP LINKED');
    await popup.click('button.end');
    await expect.poll(async () => (await stored(control)).activeSession ?? null).toBeNull();
    const storage = await stored(control);
    // Still stamped, so this change wins when the app links again.
    expect(storage.syncedAt!.activeSession).toBeGreaterThan(before);
  });
});

describe('a strict session from the desktop app', () => {
  let app: FakeDesktopApp;
  let t: TestBrowser;
  let control: Page;
  /** When the session last changed, by the newest state the extension sent the app. */
  const lastSessionChange = async () => states(await app.received()).at(-1)?.activeSession?.updatedAt ?? 0;

  const strictSession = (strictness: Strictness): FromDesktop => ({
    type: 'state',
    profiles: { value: starterProfiles, updatedAt: 0 },
    activeSession: {
      value: {
        id: strictness,
        profileId: 'deep-work',
        startedAt: Date.now(),
        timer: { minutes: 50, onEnd: 'gentle', showSinceDone: false, strictness },
      },
      updatedAt: Date.now(),
    },
  });

  async function popupEnding(): Promise<Page> {
    const tab = await t.browser.newPage();
    await tab.goto(urls.other);
    const popup = await openPopup(t, tab);
    await popup.click('button.end');
    await popup.waitForSelector('.end-early');
    return popup;
  }

  beforeAll(async () => {
    app = await fakeDesktopApp([{ type: 'linked' }, strictSession('medium')]);
    t = await launch({ nativeHost: app.host });
    control = await controlPage(t);
    await until('the session', async () => (await stored(control)).activeSession?.id === 'medium');
  });
  afterAll(async () => {
    await t?.close();
    await app?.remove();
  });

  it('ends a Medium session early once the random text is typed out', async () => {
    const popup = await popupEnding();
    const target = await popup.$eval('.target', (el) => el.textContent!.trim());
    await popup.type('.end-early input', 'not it');
    expect(await popup.$eval('button.confirm', (button) => button.disabled)).toBe(true);

    await popup.click('.end-early input', { count: 3 });
    await popup.type('.end-early input', target);
    const endedAt = Date.now();
    await popup.click('button.confirm');
    await expect.poll(async () => (await stored(control)).activeSession ?? null).toBeNull();
    await expect.poll(() => lastSessionChange()).toBeGreaterThanOrEqual(endedAt);
    expect(states(await app.received()).at(-1)?.activeSession?.value).toBeNull();
  });

  it("won't end a Hard session early", async () => {
    await app.send(strictSession('hard'));
    await expect.poll(async () => (await stored(control)).activeSession?.id).toBe('hard');
    const popup = await popupEnding();
    expect(await text(popup)).toContain('Hard mode');
    expect(await popup.$('button.confirm')).toBeNull();

    await popup.click('button.keep');
    await popup.waitForSelector('button.end');
    expect((await stored(control)).activeSession?.id).toBe('hard');
  });
});

describe('without the desktop app', () => {
  it('works on its own', async () => {
    const t = await launch();
    try {
      const control = await controlPage(t);
      const tab = await t.browser.newPage();
      await tab.goto(urls.other);
      const popup = await openPopup(t, tab);
      expect(await text(popup)).not.toContain('APP LINKED');
      await popup.click('button.profile');
      await popup.waitForSelector('.elapsed');
      expect((await stored(control)).activeSession?.profileId).toBe('writing');
    } finally {
      await t.close();
    }
  });
});
