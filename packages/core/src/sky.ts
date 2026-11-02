import type { ActiveSession } from './generated/ActiveSession';
import type { SessionRecord } from './generated/SessionRecord';
import { starterProfiles } from './profiles';
import type { Profile } from './types';

// The night sky's reward engine (PLAN §5). Everything here is worked out from the session log
// the desktop app keeps, never stored on its own, so the same log always gives the same sky and
// older sessions count too. The canvas that draws it lives in the desktop app.

const minuteMs = 60_000;
/** A session shorter than this can join the star before it… */
const sharedStarMinutes = 30;
/** …if it starts within this long of that star's last session ending. */
const sharedStarGapMs = 10 * minuteMs;
/** The smallest star. A star with fewer minutes than this isn't drawn. */
export const smallestStarMinutes = 5;
/** Focus at which a star reaches full size and brightness. */
const brightestStarMinutes = 120;
/** Overtime at which a star twinkles its brightest. */
const brightestOvertimeMinutes = 30;
/** Overtime minutes are worth this much stardust (unannounced, PLAN §2). */
const overtimeStardust = 1.5;
/** Focus hours by which the Milky Way has filled in. */
const fullMilkyWayHours = 1500;
/** How long the sky stays clear after a session, as a share of the session's length. */
const afterglowShare = 0.2;
/** How long the haze takes to creep back once the afterglow is over. */
const hazeReturnMs = 5 * minuteMs;

/** One star: a session, or a run of short back-to-back sessions of the same profile. */
export interface Star {
  /** The id of its first session, so a star keeps its id as it grows. */
  id: string;
  profileId: string;
  sessionIds: string[];
  /** When its first session started and its last one ended (epoch ms). */
  startedAt: number;
  endedAt: number;
  /** Time focused before timers were done, and after. Idle tails aren't counted. */
  focusedMs: number;
  overtimeMs: number;
}

/**
 * The stars a session log makes, oldest first. Broken sessions earn nothing. A session under
 * 30 minutes that starts within 10 minutes of the last one (of the same profile) ending joins
 * its star, and a star under 5 minutes isn't drawn, though its minutes still earn stardust.
 */
export function stars(sessions: readonly SessionRecord[]): Star[] {
  const formed: Star[] = [];
  for (const session of [...sessions].sort((a, b) => a.startedAt - b.startedAt)) {
    if (session.broken) continue;
    const last = formed[formed.length - 1];
    const minutes = (session.focusedMs + session.overtimeMs) / minuteMs;
    const joins =
      last &&
      minutes < sharedStarMinutes &&
      last.profileId === session.profileId &&
      session.startedAt - last.endedAt <= sharedStarGapMs;
    if (joins) {
      last.sessionIds.push(session.id);
      last.endedAt = Math.max(last.endedAt, session.endedAt);
      last.focusedMs += session.focusedMs;
      last.overtimeMs += session.overtimeMs;
    } else {
      formed.push({
        id: session.id,
        profileId: session.profileId,
        sessionIds: [session.id],
        startedAt: session.startedAt,
        endedAt: session.endedAt,
        focusedMs: session.focusedMs,
        overtimeMs: session.overtimeMs,
      });
    }
  }
  return formed.filter((star) => star.focusedMs + star.overtimeMs >= smallestStarMinutes * minuteMs);
}

/** How big and bright a star is, from 0 to 1. It follows the star's minutes, so it grows fast at first and then slowly. */
export function starMagnitude(star: Star): number {
  const minutes = (star.focusedMs + star.overtimeMs) / minuteMs;
  return Math.min(1, Math.sqrt(minutes / brightestStarMinutes));
}

/** How much brighter a star twinkles for its overtime, from 0 to 1. The secret overtime bonus. */
export function starSparkle(star: Star): number {
  return Math.min(1, star.overtimeMs / minuteMs / brightestOvertimeMinutes);
}

/** Stardust earned: 1 per focused minute, and a little more for overtime. Broken sessions earn nothing. */
export function stardust(sessions: readonly SessionRecord[]): number {
  let minutes = 0;
  for (const session of sessions) {
    if (session.broken) continue;
    minutes += (session.focusedMs + session.overtimeMs * overtimeStardust) / minuteMs;
  }
  return Math.floor(minutes);
}

/** All the time ever focused, overtime included and broken sessions not. */
export function lifetimeFocusMs(sessions: readonly SessionRecord[]): number {
  let focusedMs = 0;
  for (const session of sessions) if (!session.broken) focusedMs += session.focusedMs + session.overtimeMs;
  return focusedMs;
}

/**
 * How much of the Milky Way has filled in, from 0 to 1, from all the time focused. It fills in
 * quickest at first, so the first hours show, and is all there by 1,500 hours: years of focus.
 */
export function milkyWay(sessions: readonly SessionRecord[]): number {
  return Math.min(1, Math.sqrt(lifetimeFocusMs(sessions) / (fullMilkyWayHours * 60 * minuteMs)));
}

/**
 * The running session as it would be logged if it ended at `now`, so its star can form while
 * you focus. Time past the timer counts as overtime.
 */
export function sessionSoFar(session: ActiveSession, now: number): SessionRecord {
  const elapsed = Math.max(0, now - session.startedAt);
  const timerMs = session.timer && session.timer.minutes > 0 ? session.timer.minutes * minuteMs : Infinity;
  return {
    id: session.id,
    profileId: session.profileId,
    profileName: session.profileName,
    startedAt: session.startedAt,
    endedAt: session.startedAt + elapsed,
    timerMinutes: session.timer?.minutes || null,
    focusedMs: Math.min(elapsed, timerMs),
    overtimeMs: Math.max(0, elapsed - timerMs),
    idleTailMs: 0,
    endReason: 'stopped',
    broken: false,
    goal: session.goal,
    goalMet: null,
  };
}

/**
 * How clear the sky is at `now`, from 0 (hazy with light pollution) to 1 (clear). It's clear
 * during a session and for an afterglow of a fifth of the last session's length, then the haze
 * creeps back over a few minutes. A session ended before its timer was done has no afterglow, so
 * the haze starts creeping back straight away.
 */
export function skyClarity(now: number, running: boolean, last?: SessionRecord): number {
  if (running) return 1;
  if (!last) return 0;
  const length = last.focusedMs + last.overtimeMs;
  const endedEarly = (last.timerMinutes ?? 0) > 0 && length < last.timerMinutes! * minuteMs;
  const clearUntil = last.endedAt + (endedEarly ? 0 : afterglowShare * length);
  if (now <= clearUntil) return 1;
  return Math.max(0, 1 - (now - clearUntil) / hazeReturnMs);
}

/** A calm palette for profiles' star colors, with names for screen readers. */
export const starPalette = [
  { color: '#f5c86b', name: 'Gold' },
  { color: '#cfe0ff', name: 'Blue-white' },
  { color: '#b9a7f5', name: 'Lavender' },
  { color: '#9fe3c9', name: 'Mint' },
  { color: '#f2a7c3', name: 'Rose' },
  { color: '#8fd3f0', name: 'Ice blue' },
  { color: '#f7b58b', name: 'Peach' },
  { color: '#f3f0e6', name: 'Starlight' },
] as const;

export const starColors: readonly string[] = starPalette.map((entry) => entry.color);

const hexColor = /^#[0-9a-f]{6}$/i;

/** Whether `value` can be a star color: a hex color like "#f5c86b". */
export function isStarColor(value: unknown): value is string {
  return typeof value === 'string' && hexColor.test(value);
}

/**
 * A profile's star color: the one it was given, otherwise its starter profile's, otherwise one
 * from the palette picked by its id. Works for deleted profiles too, from their id alone.
 */
export function starColorFor(profileId: string, saved?: string | null): string {
  if (isStarColor(saved)) return saved.toLowerCase();
  const starter = starterProfiles.find((p) => p.id === profileId)?.starColor;
  if (isStarColor(starter)) return starter;
  return starColors[hash(profileId) % starColors.length]!;
}

/** A star color for a new profile: the first in the palette no other profile uses. */
export function newStarColor(profiles: readonly Profile[]): string {
  const used = new Set(profiles.map((p) => starColorFor(p.id, p.starColor)));
  return starColors.find((color) => !used.has(color)) ?? starColors[profiles.length % starColors.length]!;
}

/** A 32-bit FNV-1a hash of a string. */
export function hash(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** Random numbers from 0 to 1 that are always the same for the same seed (mulberry32). */
export function seeded(seed: string): () => number {
  let state = hash(seed);
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}
