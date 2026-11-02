import {
  desktopHostName,
  linkProtocol,
  type FromDesktop,
  type Profile,
  type Session,
  type SiteSettings,
  type ToDesktop,
} from '@lock-in/core';

// The link to the desktop app. The browser starts the app's bridge when the extension
// connects (Native Messaging), and the bridge says "linked" once it reaches the running app.
// While linked, the profiles, the session and the site settings stay in step with the app and
// every other linked browser: each side sends its changes, and the newest change wins. Without the app,
// none of this does anything and the extension works on its own.

type SyncedKey = 'profiles' | 'activeSession' | 'siteSettings';

export function linkDesktopApp(): void {
  let port: Browser.runtime.Port | undefined;
  let linked = false;
  // What the app last sent for each key, so the watchers can tell its changes from the user's.
  const fromApp: Partial<Record<SyncedKey, string>> = {};

  const send = (message: ToDesktop) => {
    if (linked) port?.postMessage(message);
  };

  async function sendState(keys: SyncedKey[]) {
    const [profileList, session, sites, times, basedOn] = await Promise.all([
      profiles.getValue(),
      activeSession.getValue(),
      siteSettings.getValue(),
      syncedAt.getValue(),
      profilesBasedOn.getValue(),
    ]);
    send({
      type: 'state',
      ...(keys.includes('profiles') && { profiles: { value: profileList, updatedAt: times.profiles } }),
      ...(keys.includes('profiles') && basedOn !== null && { profilesBasedOn: basedOn }),
      ...(keys.includes('activeSession') && { activeSession: { value: session, updatedAt: times.activeSession } }),
      ...(keys.includes('siteSettings') && {
        siteSettings: { value: sites, updatedAt: times.siteSettings ?? 0 },
      }),
    });
  }

  async function applyState(message: Extract<FromDesktop, { type: 'state' }>) {
    const times = await syncedAt.getValue();
    const next = { ...times };
    const updates: Parameters<typeof storage.setItems>[0] = [];
    if (message.profiles.updatedAt > times.profiles) {
      fromApp.profiles = JSON.stringify(message.profiles.value);
      next.profiles = message.profiles.updatedAt;
      updates.push({ item: profiles, value: message.profiles.value });
    }
    // Taken now, or already this side's own, sent back once the app took it.
    if (message.profiles.updatedAt >= times.profiles) {
      updates.push({ item: profilesBasedOn, value: message.profiles.updatedAt });
    }
    if (message.activeSession.updatedAt > times.activeSession) {
      fromApp.activeSession = JSON.stringify(message.activeSession.value);
      next.activeSession = message.activeSession.updatedAt;
      updates.push({ item: activeSession, value: message.activeSession.value });
    }
    // Apps from before site settings don't send any.
    if (message.siteSettings && message.siteSettings.updatedAt > (times.siteSettings ?? 0)) {
      fromApp.siteSettings = JSON.stringify(message.siteSettings.value);
      next.siteSettings = message.siteSettings.updatedAt;
      updates.push({ item: siteSettings, value: message.siteSettings.value });
    }
    if (updates.length) await storage.setItems([...updates, { item: syncedAt, value: next }]);
  }

  async function onMessage(message: FromDesktop) {
    switch (message.type) {
      case 'linked':
        linked = true;
        await desktopLinked.setValue(true);
        send({ type: 'hello', protocol: linkProtocol, browser: import.meta.env.BROWSER });
        await sendState(['profiles', 'activeSession', 'siteSettings']);
        for (const visit of await gateVisits.getValue()) send({ type: 'gateVisit', visit });
        break;
      case 'unlinked':
        linked = false;
        await desktopLinked.setValue(false);
        break;
      case 'state':
        await applyState(message);
        break;
    }
  }

  // A change made here is newer than anything the app has, so it's stamped and sent. This
  // runs even while unlinked, so changes made without the app still win once it's back.
  async function onLocalChange(key: SyncedKey, value: Profile[] | Session | SiteSettings[] | null) {
    if (JSON.stringify(value) === fromApp[key]) return;
    delete fromApp[key];
    const times = await syncedAt.getValue();
    await syncedAt.setValue({ ...times, [key]: Math.max(Date.now(), (times[key] ?? 0) + 1) });
    await sendState([key]);
  }

  profiles.watch((value) => void onLocalChange('profiles', value));
  activeSession.watch((value) => void onLocalChange('activeSession', value));
  siteSettings.watch((value) => void onLocalChange('siteSettings', value));

  // Gate visits go to the app's log as they happen and as the user answers "why?".
  gateVisits.watch((visits, previous) => {
    const before = new Map((previous ?? []).map((visit) => [visit.id, JSON.stringify(visit)]));
    for (const visit of visits) {
      if (before.get(visit.id) !== JSON.stringify(visit)) send({ type: 'gateVisit', visit });
    }
  });

  void desktopLinked.setValue(false);
  port = browser.runtime.connectNative(desktopHostName);
  // The app's messages are handled one at a time, in the order they came. Otherwise its state
  // could be applied while "linked" is still reading this side's state to send, and this side
  // would send the app's own session back as if it were its own.
  let handling = Promise.resolve();
  port.onMessage.addListener(
    (message: FromDesktop) =>
      (handling = handling
        .then(() => onMessage(message))
        .catch((error) => console.error('Seiza: could not handle a message from the app', error))),
  );
  // Fires straight away when the app isn't installed. The extension carries on by itself.
  port.onDisconnect.addListener(() => {
    port = undefined;
    linked = false;
    void desktopLinked.setValue(false);
  });
}
