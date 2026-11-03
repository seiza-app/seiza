export default defineBackground(() => {
  // Syncs run one at a time, so two quick changes can't interleave their rule updates.
  let queue = Promise.resolve();
  const sync = () =>
    (queue = queue.then(syncRedirects).catch((error) => console.error('Seiza: could not update redirects', error)));

  // Sites people add themselves need the content script too, once Seiza may reach them.
  let scripts = Promise.resolve();
  const syncScripts = () =>
    (scripts = scripts
      .then(syncOwnSiteScripts)
      .catch((error) => console.error('Seiza: could not update added sites', error)));
  const syncAll = () => {
    void sync();
    void syncScripts();
  };

  browser.runtime.onInstalled.addListener(syncAll);
  browser.runtime.onStartup.addListener(syncAll);
  activeSession.watch(sync);
  profiles.watch(syncAll);
  // Site settings decide which open tabs the redirect sync covers, as well as the added sites.
  siteSettings.watch(syncAll);
  browser.permissions.onAdded.addListener(syncAll);
  browser.permissions.onRemoved.addListener(syncAll);
  browser.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === passAlarm) void sync();
  });

  browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (isCloseTabRequest(message) && sender.tab?.id !== undefined) {
      void browser.tabs.remove(sender.tab.id);
      return;
    }
    if (!isSyncRequest(message)) return;
    void sync().then(() => sendResponse());
    return true;
  });

  linkDesktopApp();
});
