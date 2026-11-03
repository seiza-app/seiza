import type { GateVisitRecord } from './generated/GateVisitRecord';
import type { SessionRecord } from './generated/SessionRecord';
import { monthStart } from './planets';
import { seeded, stars } from './sky';
import { dayStart, starDays } from './streaks';

// Comets and rare events (PLAN §5). Like the rest of the sky, they're worked out from the log every
// time, but with this device's secret mixed into every roll: the same log and secret always give
// the same sky, and nobody can tell from the log alone what's coming. Each roll only looks at what
// came before it, so what already happened never changes as the log grows. The exception is a
// session from before them that sync brings in from another computer (on joining a group, or
// after time offline): the events, planets and constellations after it are worked out again, as
// that computer already showed them. That's accepted: it was the user's own session.
//
// Changing the order things are drawn from a roll changes everyone's past events, so new looks
// go at the end.

const hourMs = 60 * 60_000;
/** The chance of a comet for the first trip to a site each day that goes back to focus. */
const cometChance = 1 / 4;
/** The chance of a rare event for each hour of focus, early in a day… */
const chancePerHour = 0.025;
/** …which grows to this many times as likely by this many hours of focus that day. */
const busyDayBoost = 4;
const busyDayHours = 4;
/** How many sessions there have to be before a longer one counts as a personal best. */
const bestAfterSessions = 10;
/** A streak counts as a personal best from this many days, once it's longer than any before. */
const bestStreakDays = 3;
/** How many chance events happen to the month's planet instead of the sky, once it's round. */
const planetEventShare = 1 / 4;
const roundPlanetHours = 10;

/** Something that happens in the sky. Rarer toward the end of the list. */
export type SkyEventKind = 'shootingStar' | 'meteorShower' | 'aurora' | 'supernova';
const skyEventOdds: readonly [SkyEventKind, number][] = [
  ['shootingStar', 0.5],
  ['meteorShower', 0.25],
  ['aurora', 0.17],
  ['supernova', 0.08],
];
/** What a personal best brings: one of the bigger sky events. */
const bestEventKinds: readonly SkyEventKind[] = ['meteorShower', 'aurora', 'supernova'];
/**
 * Something that happens to the month's planet and stays: a moon crashing into it and leaving a
 * glowing crater, a chunk breaking off and drifting beside it, or shattering into a ring of debris.
 */
export type PlanetEventKind = 'moonCrash' | 'split' | 'shatter';
export const planetEventKinds: readonly PlanetEventKind[] = ['moonCrash', 'split', 'shatter'];
export type RareEventKind = SkyEventKind | PlanetEventKind;

/** A rare event, brought by a finished session. */
export interface RareEvent {
  /** The session that brought it. */
  id: string;
  kind: RareEventKind;
  /** When that session ended (epoch ms). */
  at: number;
  /** Whether it came by chance or for a personal best: the longest session or streak of days yet. */
  reason: 'chance' | 'longestSession' | 'longestStreak';
  /** For a streak's personal best, how many days in a row it had reached. */
  streakDays?: number;
  /** For something that happens to a planet, that planet's id (its month's first star's). */
  planetId?: string;
  /**
   * Where it happens and how it looks, from the roll: across and down (each from 0 to 1, a spot in
   * its week's constellation, or on its planet), which way it goes (radians) and its hue (degrees).
   */
  x: number;
  y: number;
  angle: number;
  hue: number;
}

/** A comet, for going back to focus instead of continuing to a site. */
export interface Comet {
  /** The trip it came from. */
  id: string;
  /** When (epoch ms). */
  at: number;
  /** Where its head sits in its week's constellation, from -1 to 1 across and -0.5 to 0.5 down. */
  x: number;
  y: number;
  /** Which way its tail streams (radians), and how long it is, in its constellation's units. */
  angle: number;
  length: number;
  /** Its hue (degrees): most comets are a pale blue, some a pale green. */
  hue: number;
}

/**
 * The comets trips to redirected sites have brought, oldest first. Only the first trip to each
 * site each day that went back to focus has a chance, and the chance doesn't grow with more trips,
 * so going to a site just to turn back earns nothing.
 */
export function comets(visits: readonly GateVisitRecord[], secret: string): Comet[] {
  const counted = new Set<string>();
  const list: Comet[] = [];
  for (const visit of [...visits].sort((a, b) => a.at - b.at)) {
    if (visit.continued) continue;
    const key = `${visit.recipeId}|${dayStart(visit.at)}`;
    if (counted.has(key)) continue;
    counted.add(key);
    const random = seeded(`${secret}:comet:${visit.id}`);
    if (random() >= cometChance) continue;
    list.push({
      id: visit.id,
      at: visit.at,
      x: random() * 1.8 - 0.9,
      y: random() * 0.8 - 0.4,
      // Streaming up and away, to one side or the other.
      angle: -Math.PI / 2 + (random() - 0.5) * 2.2,
      length: 0.5 + random() * 0.3,
      hue: random() < 0.75 ? 195 + random() * 25 : 150 + random() * 20,
    });
  }
  return list;
}

/**
 * The rare events finished sessions have brought, oldest first. Each session rolls a small chance,
 * for its minutes, so splitting one session into several earns nothing extra. It grows with the
 * time focused that day. A new longest session (after the first few) or longest streak of days
 * always brings one. Now and then, a chance event happens to the month's planet instead, once it's
 * round, and only once a month. Broken sessions earn nothing.
 */
export function rareEvents(sessions: readonly SessionRecord[], secret: string): RareEvent[] {
  const sorted = sessions.filter((session) => !session.broken).sort((a, b) => a.startedAt - b.startedAt);
  // A streak counts days with a star, as moons do, and a month's planet is its first star's.
  const sky = stars(sorted);
  const streakOn = new Map(starDays(sky).map((day) => [day.id, day.streakDays]));
  const planetIds = new Map<number, string>();
  for (const star of sky) {
    const month = monthStart(star.startedAt);
    if (!planetIds.has(month)) planetIds.set(month, star.id);
  }

  const events: RareEvent[] = [];
  const planetsHit = new Set<string>();
  let longestMs = 0;
  let count = 0;
  let day = 0;
  let dayMs = 0;
  let month = 0;
  let monthMs = 0;
  let streak = 0;
  let longestStreak = 0;
  let recordBefore = 0;
  for (const session of sorted) {
    const ms = session.focusedMs + session.overtimeMs;
    const today = dayStart(session.startedAt);
    if (today !== day) [day, dayMs] = [today, 0];
    const thisMonth = monthStart(session.startedAt);
    if (thisMonth !== month) [month, monthMs] = [thisMonth, 0];

    let streakBest = false;
    // On the first session of each day with a star.
    const streakDays = streakOn.get(session.id);
    if (streakDays !== undefined) {
      if (streakDays === 1) recordBefore = longestStreak;
      streak = streakDays;
      longestStreak = Math.max(longestStreak, streak);
      // Only on the day it passes the record, not every day after.
      streakBest = streak === Math.max(recordBefore + 1, bestStreakDays);
    }
    const sessionBest = count >= bestAfterSessions && ms > longestMs;

    // Everything is drawn whatever happens, in the same order.
    const random = seeded(`${secret}:${session.id}`);
    const roll = random();
    const which = random();
    const onPlanet = random();
    const look = { x: random(), y: random(), angle: random() * Math.PI * 2, hue: random() * 360 };

    const hours = ms / hourMs;
    const dayHours = (dayMs + ms / 2) / hourMs;
    const rate = chancePerHour * (1 + (busyDayBoost - 1) * Math.min(1, dayHours / busyDayHours));
    const reason = sessionBest
      ? 'longestSession'
      : streakBest
        ? 'longestStreak'
        : roll < 1 - Math.exp(-hours * rate)
          ? 'chance'
          : undefined;
    dayMs += ms;
    monthMs += ms;
    longestMs = Math.max(longestMs, ms);
    count++;
    if (!reason) continue;

    const planetId = planetIds.get(thisMonth);
    const hitsPlanet =
      reason === 'chance' &&
      onPlanet < planetEventShare &&
      planetId !== undefined &&
      !planetsHit.has(planetId) &&
      monthMs / hourMs >= roundPlanetHours;
    let kind: RareEventKind;
    if (hitsPlanet) {
      kind = planetEventKinds[Math.floor(which * planetEventKinds.length)]!;
      planetsHit.add(planetId);
    } else if (reason === 'chance') {
      kind = weighted(which);
    } else {
      kind = bestEventKinds[Math.floor(which * bestEventKinds.length)]!;
    }
    events.push({
      id: session.id,
      kind,
      at: session.endedAt,
      reason,
      ...(reason === 'longestStreak' && { streakDays: streak }),
      ...(hitsPlanet && { planetId }),
      ...look,
    });
  }
  return events;
}

function weighted(value: number): SkyEventKind {
  let total = 0;
  for (const [kind, odds] of skyEventOdds) {
    total += odds;
    if (value < total) return kind;
  }
  return skyEventOdds[skyEventOdds.length - 1]![0];
}
