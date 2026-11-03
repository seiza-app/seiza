import { parseHttpUrl } from './match';
import { cutChars } from './text';
import type { GateSettings, GateWait, PageWait, Profile, RedirectSettings } from './types';

/** The longest if-then plan we keep. */
export const maxPlanLength = 140;

/** The longest goal for a session, as in the app's core (session.rs). */
export const maxGoalLength = 140;

export const defaultRedirectSettings: RedirectSettings = {
  gate: {
    requireReason: false,
    wait: { mode: 'fixed', seconds: 300 },
    passMinutes: 5,
    onPassExpiry: 'pause',
  },
  links: [],
  plan: '',
};

/** Defaults for each wait mode, used when switching between them. */
export const defaultWaits = {
  fixed: { mode: 'fixed', seconds: 300 },
  escalating: { mode: 'escalating', startSeconds: 5, stepSeconds: 5, maxSeconds: 60 },
} as const satisfies Record<GateWait['mode'], GateWait>;

const limits = {
  waitSeconds: [0, 600],
  passMinutes: [1, 120],
} as const;

/**
 * A profile's redirect settings with defaults filled in. Values are clamped and cleaned,
 * since they come from storage and may have been edited or imported.
 */
export function redirectSettingsFor(profile: Profile | undefined): RedirectSettings {
  const saved = profile?.redirect;
  return {
    gate: resolveGate(saved?.gate),
    links: (saved?.links ?? []).filter((link) => parseHttpUrl(link.url)),
    plan: typeof saved?.plan === 'string' ? cutChars(saved.plan.trim(), maxPlanLength).trim() : '',
  };
}

/** How long the redirect page waits for this attempt at a site (1 = the first today). */
export function waitSecondsFor(gate: GateSettings, attempt: number): number {
  const { wait } = gate;
  if (wait.mode === 'fixed') return wait.seconds;
  return Math.min(wait.maxSeconds, wait.startSeconds + wait.stepSeconds * Math.max(0, attempt - 1));
}

/**
 * The wait for a trip to a site. The one already running carries on if it's from this session
 * and hasn't lapsed: once it's done, the site stays a click away for as long as the wait was.
 * Otherwise a new one starts.
 */
export function pageWaitFor(previous: PageWait | undefined, sessionId: string, seconds: number, now: number): PageWait {
  const lapsesAt = previous ? previous.startedAt + 2 * previous.seconds * 1000 : 0;
  if (previous?.sessionId === sessionId && now < lapsesAt) return previous;
  return { sessionId, startedAt: now, seconds };
}

/** Whole seconds left on a wait, rounded up so it never shows 0 early. */
export function waitSecondsLeft(wait: PageWait, now: number): number {
  return Math.max(0, Math.ceil((wait.startedAt + wait.seconds * 1000 - now) / 1000));
}

/** A wait in words: "5 minutes", "90 seconds", "1 minute". */
export function waitLabel(seconds: number): string {
  if (seconds >= 60 && seconds % 60 === 0) {
    const minutes = seconds / 60;
    return `${minutes} ${minutes === 1 ? 'minute' : 'minutes'}`;
  }
  return `${seconds} ${seconds === 1 ? 'second' : 'seconds'}`;
}

function resolveGate(saved: Partial<GateSettings> | undefined): GateSettings {
  const defaults = defaultRedirectSettings.gate;
  const onPassExpiry = saved?.onPassExpiry;
  return {
    requireReason: saved?.requireReason ?? defaults.requireReason,
    wait: resolveWait(saved?.wait),
    passMinutes: clamp(saved?.passMinutes, limits.passMinutes, defaults.passMinutes),
    onPassExpiry: onPassExpiry === 'nothing' || onPassExpiry === 'redirect' ? onPassExpiry : 'pause',
  };
}

function resolveWait(saved: GateWait | undefined): GateWait {
  const seconds = (value: number | undefined, fallback: number) => clamp(value, limits.waitSeconds, fallback);
  if (saved?.mode === 'escalating') {
    const defaults = defaultWaits.escalating;
    const startSeconds = seconds(saved.startSeconds, defaults.startSeconds);
    return {
      mode: 'escalating',
      startSeconds,
      stepSeconds: seconds(saved.stepSeconds, defaults.stepSeconds),
      maxSeconds: Math.max(startSeconds, seconds(saved.maxSeconds, defaults.maxSeconds)),
    };
  }
  return { mode: 'fixed', seconds: seconds(saved?.seconds, defaultWaits.fixed.seconds) };
}

function clamp(value: number | undefined, [min, max]: readonly [number, number], fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(max, Math.max(min, Math.round(value)))
    : fallback;
}
