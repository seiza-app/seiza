import { addWeeks, weekStart } from './insights';
import { seeded, type Star } from './sky';
import { cutChars } from './text';

// Constellations (PLAN §5): each week's stars, joined into a shape. Like the stars, they're
// worked out from the session log every time, so the same log always gives the same sky. Only
// the names people give them are saved (by the desktop app), keyed by the constellation's id.

/** The longest name a constellation can have, in characters. */
export const maxConstellationNameLength = 40;
/** How far apart the stars in a constellation try to stay, in its own units (it spans 2 across). */
const starGap = 0.14;
/** How far apart constellations sit on the Sky page, so that none overlap, names included. */
const skySpacing = 1.7;
const goldenAngle = Math.PI * (3 - Math.sqrt(5));

/** One week's stars, joined into a shape. */
export interface Constellation {
  /** Its first star's id, which doesn't change as the week goes on. Names are saved under it. */
  id: string;
  /** When its week starts, local midnight (epoch ms). */
  weekStart: number;
  stars: Star[];
  /**
   * Where each star sits in it, in the same order as `stars`. Across goes through the week, from
   * -1 at its start to 1 at its end; down goes from -0.5 to 0.5.
   */
  points: { x: number; y: number }[];
  /** The lines joining its stars, as pairs of indexes into `stars`. */
  lines: [number, number][];
}

/**
 * The constellations a sky's stars make, oldest first: one for every week with stars, weeks
 * starting on `firstDay` (0 being Sunday). A week with gaps is still one constellation.
 *
 * Given the constellations it made last time (`before`), it hands back the same ones for weeks
 * whose stars haven't changed, so working out the sky again as a session goes on only redoes its
 * week, and what's drawn from the rest can be kept.
 */
export function constellations(
  sky: readonly Star[],
  firstDay = 1,
  before: readonly Constellation[] = [],
): Constellation[] {
  const weeks = new Map<number, Star[]>();
  for (const star of [...sky].sort((a, b) => a.startedAt - b.startedAt)) {
    const start = weekStart(star.startedAt, firstDay);
    const week = weeks.get(start);
    if (week) week.push(star);
    else weeks.set(start, [star]);
  }
  const kept = new Map(before.map((constellation) => [constellation.weekStart, constellation]));
  return [...weeks].map(([start, stars]) => {
    const old = kept.get(start);
    if (old && sameStars(old.stars, stars)) return old;
    const points = shape(stars, start);
    return { id: stars[0]!.id, weekStart: start, stars, points, lines: joined(points) };
  });
}

/** Whether two weeks' stars are the same, down to how long each was focused for. */
function sameStars(a: readonly Star[], b: readonly Star[]): boolean {
  return (
    a.length === b.length &&
    a.every((star, i) => {
      const other = b[i]!;
      return (
        star.id === other.id &&
        star.profileId === other.profileId &&
        star.startedAt === other.startedAt &&
        star.endedAt === other.endedAt &&
        star.focusedMs === other.focusedMs &&
        star.overtimeMs === other.overtimeMs &&
        star.sessionIds.length === other.sessionIds.length
      );
    })
  );
}

/**
 * Where a week's stars sit: each in its day's slice of the week, left to right, at a height of
 * its own. Both come from the star's id, so a star never moves once it forms.
 */
function shape(stars: readonly Star[], start: number): { x: number; y: number }[] {
  const length = addWeeks(start, 1) - start;
  const points: { x: number; y: number }[] = [];
  for (const star of stars) {
    const random = seeded(star.id);
    const day = Math.min(6, Math.max(0, Math.floor(((star.startedAt - start) / length) * 7)));
    const x = -1 + (2 / 7) * (day + 0.15 + 0.7 * random());
    let y = random() - 0.5;
    // A few more tries for a spot that isn't on top of another star.
    for (let tries = 0; tries < 6 && points.some((p) => Math.hypot(p.x - x, p.y - y) < starGap); tries++) {
      y = random() - 0.5;
    }
    points.push({ x, y });
  }
  return points;
}

/** The shortest set of lines that joins every point, so the shape reads as one figure. */
function joined(points: readonly { x: number; y: number }[]): [number, number][] {
  const lines: [number, number][] = [];
  if (points.length < 2) return lines;
  // Prim's algorithm: grow the tree from the first point, always adding the nearest one left.
  const nearest = points.map((p) => Math.hypot(p.x - points[0]!.x, p.y - points[0]!.y));
  const from = points.map(() => 0);
  const inTree = points.map((_, i) => i === 0);
  for (let added = 1; added < points.length; added++) {
    let next = -1;
    for (let i = 0; i < points.length; i++) {
      if (!inTree[i] && (next < 0 || nearest[i]! < nearest[next]!)) next = i;
    }
    inTree[next] = true;
    lines.push([from[next]!, next]);
    for (let i = 0; i < points.length; i++) {
      const distance = Math.hypot(points[i]!.x - points[next]!.x, points[i]!.y - points[next]!.y);
      if (!inTree[i] && distance < nearest[i]!) {
        nearest[i] = distance;
        from[i] = next;
      }
    }
  }
  return lines;
}

/**
 * The month a week belongs to (its start, local midnight on the 1st): the one its middle falls in,
 * so every week belongs to exactly one month. The Sky shows a month's weeks at a time.
 */
export function weekMonth(weekStart: number): number {
  const middle = new Date(weekStart + (addWeeks(weekStart, 1) - weekStart) / 2);
  return new Date(middle.getFullYear(), middle.getMonth(), 1).getTime();
}

/**
 * Where each constellation sits on the Sky page, in the same order as `constellations` gives
 * them: the newest in the middle and older ones spiralling out around it, close together but
 * never overlapping. In a constellation's own units, so each one spans 2 across.
 */
export function skyPlaces(count: number): { x: number; y: number }[] {
  return Array.from({ length: count }, (_, i) => {
    const k = count - 1 - i;
    const radius = skySpacing * Math.sqrt(k + 0.5);
    return { x: radius * Math.cos(k * goldenAngle), y: radius * Math.sin(k * goldenAngle) };
  });
}

const adjectives = [
  'Quiet',
  'Patient',
  'Silver',
  'Wandering',
  'Gentle',
  'Sleepy',
  'Steady',
  'Hidden',
  'Little',
  'Distant',
  'Velvet',
  'Golden',
  'Drifting',
  'Hushed',
  'Faithful',
  'Curious',
];
const nouns = [
  'Heron',
  'Lantern',
  'Kite',
  'Fox',
  'Harp',
  'Willow',
  'Compass',
  'Sparrow',
  'Anchor',
  'Moth',
  'Otter',
  'Teapot',
  'Lighthouse',
  'Owl',
  'Fern',
  'Whale',
  'Violin',
  'Acorn',
  'Feather',
  'Rabbit',
  'Bell',
  'Ship',
  'Crane',
  'Kettle',
];

/**
 * Each constellation's name, in the same order: the one it was given, or else one made up from
 * its id, like "The Quiet Heron". Made-up names skip ones already in use where they can.
 */
export function constellationNames(
  list: readonly Constellation[],
  saved: ReadonlyMap<string, string> = new Map(),
): string[] {
  const used = new Set(saved.values());
  return list.map((constellation) => {
    const given = saved.get(constellation.id);
    if (given) return given;
    const random = seeded(constellation.id);
    const adjective = Math.floor(random() * adjectives.length);
    const noun = Math.floor(random() * nouns.length);
    const options = adjectives.length * nouns.length;
    let name = '';
    // Steps through every other pairing in turn until one is free.
    for (let step = 0; step < options; step++) {
      const pick = (adjective * nouns.length + noun + step) % options;
      name = `The ${adjectives[Math.floor(pick / nouns.length)]} ${nouns[pick % nouns.length]}`;
      if (!used.has(name)) break;
    }
    used.add(name);
    return name;
  });
}

/**
 * The name worth keeping from what was typed: trimmed, on one line and cut down to a sensible
 * length, or undefined if there's nothing there (which goes back to the made-up name).
 */
export function cleanedConstellationName(text: string): string | undefined {
  const cleaned = cutChars(text.replace(/\s+/g, ' ').trim(), maxConstellationNameLength).trimEnd();
  return cleaned || undefined;
}
