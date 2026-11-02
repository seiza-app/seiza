import {
  starterProfiles,
  type GateVisit,
  type PageWait,
  type Profile,
  type Session,
  type SiteSettings,
} from '@lock-in/core';

/** The running session, shared by the popup and every content script. */
export const activeSession = storage.defineItem<Session | null>('local:activeSession', {
  fallback: null,
});

/** The user's profiles. Until they edit anything, these are the starter profiles. */
export const profiles = storage.defineItem<Profile[]>('local:profiles', {
  fallback: starterProfiles,
});

/**
 * What's set for sites outside any profile: parts of their pages picked to hide, and taming all
 * the time, even with no session running.
 */
export const siteSettings = storage.defineItem<SiteSettings[]>('local:siteSettings', {
  fallback: [],
});

/** Trips to redirected sites, for the attempt counter and the "why?" answers. */
export const gateVisits = storage.defineItem<GateVisit[]>('local:gateVisits', {
  fallback: [],
});

/**
 * The redirect page's running wait for each site, keyed by recipe id, so leaving the page to
 * work and coming back picks the countdown back up.
 */
export const pageWaits = storage.defineItem<Record<string, PageWait>>('local:pageWaits', {
  fallback: {},
});

/**
 * When the profiles, the session and the site settings last changed, for the desktop app link.
 * The newest change wins, whether it was made here, in the app, or in another browser. Saved
 * before site settings existed, it has no time for them, which counts as never changed.
 */
export const syncedAt = storage.defineItem<{ profiles: number; activeSession: number; siteSettings?: number }>(
  'local:syncedAt',
  { fallback: { profiles: 0, activeSession: 0, siteSettings: 0 } },
);

/**
 * The `updatedAt` of the app's profiles this browser last took, sent with its own list so the app
 * can tell what this browser hadn't seen yet. None until the app first sends its profiles.
 */
export const profilesBasedOn = storage.defineItem<number | null>('local:profilesBasedOn', {
  fallback: null,
});

/** Whether the tour for new users has been offered (taken or turned down), so the popup offers it once. */
export const tourOffered = storage.defineItem<boolean>('local:tourOffered', {
  fallback: false,
});

/** Whether the desktop app is linked right now. Kept in session storage, so it starts off false. */
export const desktopLinked = storage.defineItem<boolean>('session:desktopLinked', {
  fallback: false,
});
