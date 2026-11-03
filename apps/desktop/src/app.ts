import type {
  Activity,
  AppRule,
  BlockerStatus,
  ConstellationName,
  GateVisitRecord,
  GlowColor,
  ImportReport,
  InstalledApp,
  OtherSitesLevel,
  PairingState,
  SatelliteAction,
  SavedBackup,
  SessionRecord,
  Settings,
  SiteRule,
  SkyState,
  Snapshot,
  SyncDevice,
  SyncStatus,
  TimerSettings,
  UpdateStatus,
} from '@lock-in/core';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { onUnmounted, ref } from 'vue';

// The window is a view onto the app's Rust core, which owns the session. Changes go to the
// core as commands, and it sends a fresh snapshot back whenever anything changes, whether
// from here or from the menu bar.

/** The app's state, kept up to date as it changes. */
export function useSnapshot() {
  const snapshot = ref<Snapshot>();
  const stopListening = listen<Snapshot>('snapshot', (event) => (snapshot.value = event.payload));
  // A change that lands while this is loading is newer, so it wins.
  void invoke<Snapshot>('snapshot').then((value) => (snapshot.value ??= value));
  onUnmounted(() => void stopListening.then((stop) => stop()));
  return snapshot;
}

/** The current time, ticking once a second while the component is shown. */
/** The time, once a second, for clocks on the page. It doesn't tick while the window is hidden. */
export function useNow() {
  const now = ref(Date.now());
  const update = () => document.visibilityState !== 'hidden' && (now.value = Date.now());
  const tick = setInterval(update, 1000);
  document.addEventListener('visibilitychange', update);
  onUnmounted(() => {
    clearInterval(tick);
    document.removeEventListener('visibilitychange', update);
  });
  return now;
}

/**
 * Each profile's saved star color, by its id, deleted profiles' included, so their stars keep
 * theirs. Profiles with none picked get one from their id (`starColorFor`).
 */
export const savedStarColors = (snapshot?: Snapshot) =>
  new Map<string, string | null>([
    ...Object.entries(snapshot?.deletedStarColors ?? {}),
    ...(snapshot?.profiles ?? []).map((p) => [p.id, p.starColor] as const),
  ]);

/** Starts a session, with a goal if its profile asks for one and one was given. */
export const startSession = (profileId: string, goal?: string) => invoke<void>('start_session', { profileId, goal });

/** Answers "Did you do it?" for the last session's goal. */
export const answerGoal = (sessionId: string, met: boolean) => invoke<void>('answer_goal', { sessionId, met });

export const endSession = () => invoke<void>('end_session');

// Not `use…`, which would read as a Vue composable.
export const spendEmergencyPass = () => invoke<void>('use_emergency_pass');

export const setProfileTimer = (profileId: string, timer: TimerSettings) =>
  invoke<void>('set_profile_timer', { profileId, timer });

export const setProfileApps = (profileId: string, apps: AppRule[]) =>
  invoke<void>('set_profile_apps', { profileId, apps });

// Blocking without the extension (blocker.rs). Installing and removing ask for an
// administrator's password in macOS's own dialog.
export const blockerStatus = () => invoke<BlockerStatus>('blocker_status');
export const installBlocker = () => invoke<void>('install_blocker');
export const removeBlocker = () => invoke<void>('remove_blocker');

// Quotes of the user's own (quotes.rs), shown on the Sky.
export const addOwnQuote = (text: string, author: string) => invoke<void>('add_own_quote', { text, author });
export const removeOwnQuote = (id: string) => invoke<void>('remove_own_quote', { id });

export const setProfileSites = (profileId: string, sites: SiteRule[], otherSites: OtherSitesLevel) =>
  invoke<void>('set_profile_sites', { profileId, sites, otherSites });

export const setProfileAsksGoal = (profileId: string, on: boolean) =>
  invoke<void>('set_profile_asks_goal', { profileId, on });

export const setProfilePlan = (profileId: string, plan: string) =>
  invoke<void>('set_profile_plan', { profileId, plan });

export const setProfileStarColor = (profileId: string, color: string) =>
  invoke<void>('set_profile_star_color', { profileId, color });

/** The apps that can be added to a profile. */
export const installedApps = () => invoke<InstalledApp[]>('installed_apps');

export const setSettings = (settings: Settings) => invoke<void>('set_settings', { settings });

/** Changes the satellite box's hotkey, like "Alt+KeyP", once it's registered. Empty turns it off. */
export const setSatelliteHotkey = (hotkey: string) => invoke<void>('set_satellite_hotkey', { hotkey });

/** Ignores the satellite box's hotkey while a new one is being recorded. */
export const pauseSatelliteHotkey = (paused: boolean) => invoke<void>('pause_satellite_hotkey', { paused });

/** Whether this is a Mac, for showing keys the way it does. */
export const mac = navigator.userAgent.includes('Mac');

export const forgetBrowser = (path: string) => invoke<void>('forget_browser', { path });

/** Launches a thought into orbit. Resolves to whether there was anything to launch. */
export const launchSatellite = (text: string) => invoke<boolean>('launch_satellite', { text });

export const actOnSatellite = (id: string, action: SatelliteAction) => invoke<void>('act_on_satellite', { id, action });

/** Everything logged from `from` up to (not including) `to` (epoch ms), for the weekly report. */
export const activity = (from: number, to: number) => invoke<Activity>('activity', { from, to });

/** Every finished session, oldest first, for the night sky. */
export const sessionLog = () => invoke<SessionRecord[]>('session_log');

/** The names people gave constellations. */
export const constellationNames = () => invoke<ConstellationName[]>('constellation_names');

/** Names a constellation, by its first star's id. A blank name goes back to the made-up one. */
export const nameConstellation = (id: string, name: string) => invoke<void>('name_constellation', { id, name });

/** The night sky's secret for rare events, and when the Sky last showed new ones. */
export const skyState = () => invoke<SkyState>('sky_state');

/** Notes that the Sky has shown everything up to `at` (epoch ms), so each plays once. */
export const skySeen = (at: number) => invoke<void>('sky_seen', { at });

// Backups of everything (backup.rs): saved to the Downloads folder, and brought in from a file's
// text, adding to what's here.
export const exportBackup = (day: string) => invoke<SavedBackup>('export_backup', { day });
export const showBackup = () => invoke<void>('show_backup');
export const importBackup = (json: string) => invoke<ImportReport>('import_backup', { json });

/** Notes that the tour for new users has been offered, so the Sky doesn't offer it again. */
export const markTourOffered = () => invoke<void>('tour_offered');

/** Whether a new version of the app is out (updates.rs), kept up to date as checks run. */
export function useUpdates() {
  const updates = ref<UpdateStatus>();
  const stopListening = listen<UpdateStatus>('updates', (event) => (updates.value = event.payload));
  void invoke<UpdateStatus>('update_status').then((value) => (updates.value ??= value));
  onUnmounted(() => void stopListening.then((stop) => stop()));
  return updates;
}

export const checkForUpdates = () => invoke<UpdateStatus>('check_for_updates');

/** Downloads and installs the new version, then restarts the app. Between sessions only. */
export const installUpdate = () => invoke<void>('install_update');

/** Notes that the Sky has offered this version, so it doesn't offer it again. */
export const markUpdateOffered = (version: string) => invoke<void>('update_offered', { version });

/** A version's page on GitHub: what's new in it, and its downloads. */
export const releasePage = (version: string) => `https://github.com/seiza-app/seiza/releases/tag/v${version}`;

// Settings › Sync (sync/setup.rs). A pairing runs in the core, which sends how it's going as
// "sync-pairing"; whether this computer syncs, and with which server, comes as "sync".

/** Whether this computer syncs, kept up to date. */
export function useSyncStatus() {
  const status = ref<SyncStatus>();
  const stopListening = listen<SyncStatus>('sync', (event) => (status.value = event.payload));
  void invoke<SyncStatus>('sync_status').then((value) => (status.value ??= value));
  onUnmounted(() => void stopListening.then((stop) => stop()));
  return status;
}

/** Calls `then` whenever records from the sync group's other computers arrive, or the group's sky. */
export function onSynced(then: () => void) {
  const stopListening = listen('synced', then);
  onUnmounted(() => void stopListening.then((stop) => stop()));
}

/** How a pairing is going, on either computer, kept up to date. */
export function usePairing() {
  const pairing = ref<PairingState>({ phase: 'idle' });
  const stopListening = listen<PairingState>('sync-pairing', (event) => (pairing.value = event.payload));
  void invoke<PairingState>('sync_pairing').then((value) => {
    if (pairing.value.phase === 'idle') pairing.value = value;
  });
  onUnmounted(() => void stopListening.then((stop) => stop()));
  return pairing;
}

export const startSyncGroup = (address: string, deviceName: string, invite?: string) =>
  invoke<void>('sync_start_group', { address, deviceName, invite });
export const joinSyncGroup = (address: string, code: string, deviceName: string) =>
  invoke<PairingState>('sync_join', { address, code, deviceName });
export const addSyncDevice = () => invoke<PairingState>('sync_add_device');
/** Add or Don't add, once the new computer has typed the code. */
export const answerSyncDevice = (add: boolean) => invoke<void>('sync_answer_device', { add });
/** Stops the pairing that's running, so its code can't be used. */
export const stopPairing = () => invoke<void>('sync_stop_pairing');
export const syncDevices = () => invoke<SyncDevice[]>('sync_devices');
export const removeSyncDevice = (deviceId: string) => invoke<void>('sync_remove_device', { deviceId });
/** Start sessions from my other computers here, on or off. */
export const setSyncFollow = (on: boolean) => invoke<void>('sync_set_follow', { on });
/** Leaves sync. Resolves to whether the server heard. */
export const leaveSync = () => invoke<boolean>('sync_leave');
export const moveSyncServer = (address: string, invite?: string) =>
  invoke<void>('sync_move_server', { address, invite });

/** Trips to redirected sites that went back to focus, oldest first, for comets. */
export const turnedBack = () => invoke<GateVisitRecord[]>('turned_back');

/** Adds six weeks of made-up sessions, in development builds. */
export const addSampleData = () => invoke<void>('add_sample_data');

/** Removes the made-up sessions again, in development builds. */
export const removeSampleData = () => invoke<void>('remove_sample_data');

type WeekInfo = { firstDay: number };

/**
 * The first day of the week as set on this computer, 0 being Sunday: from the system where the
 * core can read it (macOS), otherwise the default for the window's language, otherwise Monday.
 */
export async function firstDayOfWeek(): Promise<number> {
  const system = await invoke<number | null>('first_weekday');
  if (system !== null) return system;
  // Newer engines have getWeekInfo(); older ones had a weekInfo property instead.
  const locale = new Intl.Locale(navigator.language) as Intl.Locale & {
    getWeekInfo?: () => WeekInfo;
    weekInfo?: WeekInfo;
  };
  const firstDay = (locale.getWeekInfo?.() ?? locale.weekInfo)?.firstDay;
  // Counted from 1, for Monday, to 7, for Sunday.
  return firstDay ? firstDay % 7 : 1;
}

/**
 * Shows a glow color in this window's frames' corner ticks and the tabs' dot. Starlight is the
 * stylesheet's own, so it's left to the stylesheet.
 */
export function applyGlow(glow: GlowColor) {
  const root = document.documentElement.style;
  if (glow.hours === 0) {
    root.removeProperty('--glint');
    root.removeProperty('--dot');
  } else {
    root.setProperty('--glint', glow.color);
    root.setProperty('--dot', glow.color);
  }
}

/**
 * Marks the page `resting` while its window isn't focused, as when you work in another app during
 * a session. Looping animations pause then to save battery, as the sky does (NightSky.vue), and
 * carry on where they were when it's focused again.
 */
export function restWhenAway() {
  const rest = () => document.documentElement.classList.toggle('resting', !document.hasFocus());
  window.addEventListener('focus', rest);
  window.addEventListener('blur', rest);
  rest();
}

/** Closes the satellite box and hands focus back to the app you were in. */
export const closeCapture = () => invoke<void>('close_capture');

export { maxSatelliteLength } from '@lock-in/core';
