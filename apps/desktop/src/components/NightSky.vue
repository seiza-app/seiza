<script setup lang="ts">
import { seeded, skyPlaces, type Comet, type Planet, type RareEvent, type RareEventKind } from '@lock-in/core';
import { onMounted, onUnmounted, ref, watch } from 'vue';
import { drawMilkyWay } from '../milkyWay';
import { forgetPlanets, stampPlanet } from '../planet';
import { drawSatellite, leaveMs } from '../satellite';
import { drawComet, drawPassing, drawSupernova, playMs } from '../skyEvents';

// The night sky behind the window (PLAN §5): one canvas, drawn by code. The constellations come
// from the reward engine (packages/core/src/constellations.ts); this only draws them, with light
// pollution washing them out as `clarity` falls. It redraws 60 times a second while stars twinkle
// (30 with `batterySaver`), every frame the screen shows while something glides or plays, and
// only when something changes under reduced motion. While the window isn't focused it pauses,
// still drawn as it was, and nothing is drawn at all while it's hidden.
//
// It has two layouts, and constellations glide from one to the other. On the Sky (`sky`), they
// gather close together, the newest in the middle, like one starry collection. Exploring spreads
// them out, and hovering one there brightens it and shows its name beneath it. In the weekly report (`week`), they line up in week
// order, with the week being looked at large and soft behind the page's header.
//
// With `drift` on (exploring the Sky page), the sky leans and zooms gently toward the pointer, like
// nudging a telescope, with brighter (nearer) stars moving a little more for depth. It can also be
// dragged around and zoomed (the wheel, a pinch, the keyboard or `zoom`), out as far as the oldest
// constellation.
//
// To stay smooth after years of focus (PLAN §5, performance at scale), the Sky shows a month's
// constellations at a time, the latest unless `month` steps back, and the rest fold away into the
// Milky Way, which fills in behind everything as total focus grows. Exploring starts on the latest
// weeks, and zooming out finds the rest; far away, a constellation's stars fold into a soft glow. Only what's on screen is drawn,
// and still things are drawn once and reused.
//
// Planets (one a month) sit nearer than the stars, around the edge of the constellations, the
// newest largest. On the Sky, the last few months' are still; exploring shows them all, slowly
// circling, with their moons going round. In the weekly report, the week's month's planet sits
// faintly beside its constellation.
//
// Comets and rare events (skyEvents.ts) play once, the first time they're in `play`. Comets and
// the nebulae supernovae leave stay beside their week's constellation, behind its stars.
//
// Satellites (satellite.ts) slowly circle the constellations on the Sky and while exploring, each
// on its own orbit, nearest of all. They fade in when launched and out when deleted, and one
// that went onto the task list drifts away the first time it's in `play`. What one says shows beneath it on hover
// only with `satelliteText` (exploring between sessions). Under reduced motion they sit still.

export interface SkyStar {
  id: string;
  /** Where it sits in its constellation: from -1 to 1 across, and -0.5 to 0.5 down. */
  x: number;
  y: number;
  color: string;
  /** Size and brightness, from 0 to 1. */
  magnitude: number;
  /** Extra twinkle from overtime, from 0 to 1. */
  sparkle: number;
}

export interface SkyConstellation {
  id: string;
  name: string;
  /** When its week starts (epoch ms). */
  weekStart: number;
  /** The month its week belongs to (its start, epoch ms), which the Sky shows a month at a time. */
  month: number;
  stars: SkyStar[];
  /** Lines joining its stars, as pairs of indexes into `stars`. */
  lines: [number, number][];
}

/**
 * A month's planet, with what to show beneath it on hover (its month and focus time) and the star
 * color of its month's top profile, which some rings take.
 */
export type SkyPlanet = Planet & { label: string; tint: string };
/** A comet, beside its week's constellation. */
export type SkyComet = Comet & { constellationId: string };
/** A rare event, with its week's constellation if it has one, where a supernova's nebula stays. */
export type SkyEvent = RareEvent & { constellationId?: string };
/** A satellite: a thought in orbit or waiting to be sorted, or one that just went onto the task
 * list (`tasked`), which drifts away. */
export interface SkySatellite {
  id: string;
  text: string;
  tasked: boolean;
}

const props = defineProps<{
  /** Oldest first. */
  constellations: SkyConstellation[];
  /** On the Sky, the month whose weeks it shows (its start, epoch ms). The latest if not given. */
  month?: number;
  /** Oldest first. */
  planets?: SkyPlanet[];
  /** How filled in the Milky Way is, from 0 to 1. */
  milkyWay?: number;
  /** How clear the sky should be, from 0 (hazy) to 1 (clear). The haze drifts there slowly. */
  clarity: number;
  /** Whether the sky drifts toward the pointer. */
  drift?: boolean;
  /** Whether hovering a constellation brightens it and shows its name beneath it. */
  names?: boolean;
  /** In the week layout, the start of the week being looked at (epoch ms). Otherwise the Sky layout. */
  week?: number;
  comets?: SkyComet[];
  events?: SkyEvent[];
  satellites?: SkySatellite[];
  /** Whether hovering a satellite shows what it says beneath it. */
  satelliteText?: boolean;
  /** Whether stars twinkle at 30 frames a second instead of 60, to save battery. */
  batterySaver?: boolean;
  /**
   * Comets (`comet:` and their id), events (`event:` and their id) and satellites that went onto
   * the task list (`satellite:` and their id) to play, the first time they're here.
   */
  play?: string[];
}>();
const emit = defineEmits<{
  /** While exploring, how far in it's zoomed and the constellation in the middle of the view, if any. */
  view: [view: { zoom: number; aimed?: string }];
}>();

const canvas = ref<HTMLCanvasElement>();
const still = window.matchMedia('(prefers-reduced-motion: reduce)');
/** How long the haze takes to drift most of the way to a new clarity. */
const hazeDriftMs = 2500;
/**
 * How often to redraw while only twinkling: 60 times a second, or 30 to save battery. A little
 * under the time between frames, so a 120 Hz screen draws every other one (or every fourth).
 */
const frameMs = { smooth: 1000 / 60 - 2, saver: 1000 / 30 - 2 };
/** How far the view pans toward the pointer, as a share of the sky, and how much it zooms in. */
const driftPan = 0.04;
const driftZoom = 0.06;
/** How long the view takes to catch up with the pointer. */
const driftMs = 900;
/** How long constellations take to glide most of the way to a new layout, and to brighten. */
const glideMs = 1200;
const shineMs = 350;
const weekMs = 7 * 24 * 60 * 60_000;
/** How far a constellation reaches from its middle, in its own units: across, then up and down with its name. */
const reach = { x: 1.15, up: 0.6, down: 1 };
/** How much closer together constellations sit on the Sky than while exploring. They overlap a little. */
const skyGather = 0.5;
/** How many months' planets the Sky shows; exploring shows them all. */
const skyPlanets = 6;
/** How long planets take to circle the sky while exploring. */
const planetLapMs = 8 * 60_000;
const goldenAngle = Math.PI * (3 - Math.sqrt(5));
/** How long after one comet or event starts playing the next one does. */
const playGapMs = 1400;
/** How long satellites take to go round, at the quickest, and how much longer some take. */
const satelliteLapMs = 4 * 60_000;
const satelliteLapSpreadMs = 3 * 60_000;
/** How long a satellite takes to fade in when launched, or out when deleted. */
const satelliteFadeMs = 1500;
/** The widest a satellite's text is shown, in pixels before the screen's scale. */
const satelliteTextWidth = 260;
/** How many of the latest weeks exploring starts on. Zooming out finds the rest. */
const exploreWeeks = 12;
/** How far in exploring can zoom, and how long the view takes to catch up when zoomed or let go. */
const mostZoom = 4;
const viewMs = 500;
/**
 * When a constellation's unit is this many pixels or fewer (before the screen's scale), its stars
 * start folding into a soft glow, and by `foldTo` they're all glow.
 */
const foldFrom = 13;
const foldTo = 8;
/** Below this many pixels per unit, a constellation is too small to see twinkle, so it's drawn once and reused. */
const twinkleFrom = 17;

let context: CanvasRenderingContext2D | null = null;
let frame = 0;
/**
 * While the window isn't focused (e.g. while you work in another app during a session), the sky
 * pauses to save battery: nothing twinkles, circles or plays, though it's still drawn again when
 * something changes. The animation clock stops too, so it all carries on from where it was.
 */
let paused = !document.hasFocus();
let pausedAt = performance.now();
/** How long the sky has been paused altogether, which the animation clock leaves out. */
let pausedMs = 0;
/** The animation clock (ms), from the time on the page's clock. */
const clock = (now: number) => (paused ? pausedAt : now) - pausedMs;
/** Whether things settle straight away instead of easing: under reduced motion, or while paused. */
const resting = () => still.matches || paused;
/**
 * While only twinkling (and planets circling), the loop sleeps until its next frame is due rather
 * than waking for every one the screen shows (120 a second on some screens). `frame` is -1 then.
 */
let nap: ReturnType<typeof setTimeout> | undefined;
let lastDrawn = 0;
let lastTime = 0;
let shownClarity = props.clarity;
let font = 'system-ui, sans-serif';
// Where the pointer is (from -1 to 1 across and down) and how far in to zoom (0 to 1), and
// where the view has got to on its way there.
let aim = { x: 0, y: 0, zoom: 0 };
const lens = { x: 0, y: 0, zoom: 0 };
/**
 * Where exploring has panned and zoomed to: the middle of the view (as shares of the sky) and how
 * far in. `view` is where it's headed and `seen` where it has got to.
 */
interface View {
  x: number;
  y: number;
  zoom: number;
}
const home: View = { x: 0.5, y: 0.5, zoom: 1 };
let view = { ...home };
const seen = { ...home };
/** What there is to explore, as shares of the sky at the first zoom, so the view stays over it. */
let world = { left: 0, right: 1, top: 0, bottom: 1 };
/** A drag panning the sky: where it started, and how fast it was going, for a soft stop. */
let dragging: { id: number; x: number; y: number; from: View; speed: { x: number; y: number }; at: number } | undefined;
const sprites = new Map<string, HTMLCanvasElement>();
/** How far planets have gone round while exploring, in radians. Planets spin and moons go round with it. */
let orbit = 0;
/** What's playing for the first time, by its key in `play`, and when it starts (on the animation clock). */
type PlayKind = RareEventKind | 'comet' | 'satellite';
const plays = new Map<string, { start: number; kind: PlayKind }>();
/** What has played (or started to), so nothing plays twice. */
const started = new Set<string>();
const playLength = (kind: PlayKind) => (kind === 'satellite' ? leaveMs : playMs[kind]);

/**
 * How far something has played, from 0 to 1, while it plays: negative while it waits its turn, and
 * undefined once it's done (or never played).
 */
function played(key: string, time: number): number | undefined {
  const playing = plays.get(key);
  return playing && Math.min(1, (time - playing.start) / playLength(playing.kind));
}

/**
 * Where satellites circle: around a middle (as shares of the sky's width and height), reaching
 * as far across and down as the constellations do (as shares too), and how visible they are.
 */
interface Ring {
  x: number;
  y: number;
  across: number;
  down: number;
  alpha: number;
}
/** Where the ring has got to on its way to where the layout puts it. */
let ring: Ring | undefined;
/** A satellite's own orbit, from its id: a tilted ellipse, gone round at its own pace. */
interface SatelliteOrbit {
  /** How far across and down it reaches, as shares of the ring's. */
  across: number;
  down: number;
  tilt: number;
  /** Where it starts (radians), which way it goes round, and how long it takes. */
  phase: number;
  direction: 1 | -1;
  lapMs: number;
  /** Which way its panels point, and when its light blinks. */
  angle: number;
  blink: number;
}
function satelliteOrbit(id: string): SatelliteOrbit {
  const random = seeded(`satellite:${id}`);
  return {
    across: 0.55 + random() * 0.4,
    down: 0.35 + random() * 0.45,
    tilt: (random() - 0.5) * 0.4,
    phase: random() * Math.PI * 2,
    direction: random() < 0.75 ? 1 : -1,
    lapMs: satelliteLapMs + random() * satelliteLapSpreadMs,
    angle: (random() - 0.5) * 0.7,
    blink: random(),
  };
}
/**
 * The satellites being drawn, by id, with how visible each is (fading in when launched and out
 * once gone) and how brightly it shines from being hovered, from 0 to 1.
 */
const orbiters = new Map<
  string,
  { satellite: SkySatellite; orbit: SatelliteOrbit; alpha: number; glow: number; gone: boolean }
>();
function syncSatellites(list: readonly SkySatellite[]) {
  const ids = new Set(list.map((satellite) => satellite.id));
  for (const satellite of list) {
    const orbiter = orbiters.get(satellite.id);
    if (orbiter) Object.assign(orbiter, { satellite, gone: false });
    else orbiters.set(satellite.id, { satellite, orbit: satelliteOrbit(satellite.id), alpha: 0, glow: 0, gone: false });
  }
  for (const [id, orbiter] of orbiters) if (!ids.has(id)) orbiter.gone = true;
}

/**
 * Where a constellation or planet is: its middle (as shares of the sky's width and height), how
 * big one of its units is or the planet's radius (as a share of the width), how bright it is
 * overall, and how visible its name is.
 */
interface Place {
  x: number;
  y: number;
  unit: number;
  alpha: number;
  label: number;
  /** How much it stands out, as if hovered, from 0 to 1: the week the report is showing. */
  lift: number;
}
/**
 * Where each constellation and planet has got to on its way to where the layout puts it. Planets
 * are kept under `planet:` and their id, since a month's first star can also start a week.
 */
const shown = new Map<string, Place>();
/** How brightly each constellation and planet shines from being hovered, from 0 to 1. */
const shine = new Map<string, number>();
let hovered: string | undefined;
/** Where each constellation was last drawn, in canvas pixels, to find the one under the pointer. */
let drawnBoxes: { id: string; left: number; right: number; top: number; bottom: number }[] = [];
/** Where each planet was last drawn, in canvas pixels. */
let drawnPlanets: { key: string; x: number; y: number; radius: number }[] = [];
/** Where each satellite going round was last drawn, in canvas pixels. */
let drawnSatellites: { key: string; x: number; y: number }[] = [];
const planetKey = (planet: SkyPlanet) => `planet:${planet.id}`;
/** The month a time falls in, counted from year 0, so months can be subtracted. */
function monthNumber(at: number): number {
  const date = new Date(at);
  return date.getFullYear() * 12 + date.getMonth();
}

/** A soft glow with a bright core in `color`, drawn once and stamped for every star of that color. */
function sprite(color: string): HTMLCanvasElement {
  let glow = sprites.get(color);
  if (glow) return glow;
  glow = document.createElement('canvas');
  glow.width = glow.height = 64;
  const g = glow.getContext('2d')!;
  const gradient = g.createRadialGradient(32, 32, 0, 32, 32, 32);
  gradient.addColorStop(0, '#ffffff');
  gradient.addColorStop(0.08, color);
  gradient.addColorStop(0.22, `${color}66`);
  gradient.addColorStop(0.5, `${color}14`);
  gradient.addColorStop(1, `${color}00`);
  g.fillStyle = gradient;
  g.fillRect(0, 0, 64, 64);
  sprites.set(color, glow);
  return glow;
}

/** Where the layout puts each constellation and planet, and the satellites' ring, for a sky `width` by `height`. */
function targets(width: number, height: number): { places: Map<string, Place>; ring: Ring } {
  const places = new Map<string, Place>();
  const list = props.constellations;
  if (props.week !== undefined) {
    // In week order, the week being looked at behind the header and the rest off to its sides.
    const scale = window.devicePixelRatio || 1;
    for (const constellation of list) {
      const weeks = Math.round((constellation.weekStart - props.week) / weekMs);
      places.set(constellation.id, {
        x: 0.5 + weeks * 0.86,
        y: Math.min(0.16, (48 * scale) / height),
        unit: 0.34,
        alpha: weeks === 0 ? 1 : 0.4,
        label: 0,
        lift: weeks === 0 ? 1 : 0,
      });
    }
    // The week's month's planet off to the right of its constellation's name, clear of the
    // header's text, and the others in month order past it, out of sight.
    const month = monthNumber(props.week + weekMs / 2);
    for (const planet of props.planets ?? []) {
      const months = monthNumber(planet.monthStart) - month;
      const radius = Math.min(width, height) * 0.05 * (0.45 + 0.55 * planet.size);
      places.set(planetKey(planet), {
        x: Math.min(0.88, 1 - (radius * 2.8) / width) + months * 0.9,
        y: (128 * scale) / height,
        unit: radius / width,
        alpha: months === 0 ? 0.8 : 0,
        label: 0,
        lift: months === 0 ? 1 : 0,
      });
    }
    // Satellites stay out of the report.
    return { places, ring: { ...(ring ?? { x: 0.5, y: 0.35, across: 0.44, down: 0.22 }), alpha: 0 } };
  }
  // On the Sky, a month's weeks at a time (the latest month, or `month`), packed close so they read
  // as one starry collection, fitted between the header and the cards at the bottom. Every other
  // month sits in the same spots unseen, so stepping from one month to another fades between them.
  // Exploring clears the cards away, and they all spread out into separate constellations to
  // admire one at a time, starting on the latest weeks, with older ones further out.
  const shownMonth = props.month ?? list[list.length - 1]?.month;
  let spots: { x: number; y: number }[];
  let fitted: { x: number; y: number }[];
  if (props.drift) {
    spots = skyPlaces(list.length);
    fitted = spots.slice(-exploreWeeks);
  } else {
    // Each month's weeks spiral out from its newest.
    const months = new Map<number, number[]>();
    list.forEach((constellation, i) =>
      months.set(constellation.month, [...(months.get(constellation.month) ?? []), i]),
    );
    spots = [];
    for (const indexes of months.values()) {
      const near = skyPlaces(indexes.length);
      indexes.forEach((index, j) => (spots[index] = { x: near[j]!.x * skyGather, y: near[j]!.y * skyGather }));
    }
    fitted = (shownMonth === undefined ? [] : (months.get(shownMonth) ?? [])).map((index) => spots[index]!);
  }
  const bounds = (some: readonly { x: number; y: number }[]) => {
    if (!some.length) some = [{ x: 0, y: 0 }];
    return {
      left: Math.min(...some.map((s) => s.x)) - reach.x,
      right: Math.max(...some.map((s) => s.x)) + reach.x,
      top: Math.min(...some.map((s) => s.y)) - reach.up,
      bottom: Math.max(...some.map((s) => s.y)) + reach.down,
    };
  };
  const box = bounds(fitted);
  const area = {
    left: 0.06 * width,
    right: 0.94 * width,
    // On the Sky, between the day at the top of the page and the profiles.
    top: (props.drift ? 0.13 : 0.28) * height,
    bottom: (props.drift ? 0.9 : 0.56) * height,
  };
  const fitX = (area.right - area.left) / (box.right - box.left);
  const fitY = (area.bottom - area.top) / (box.bottom - box.top);
  const unit = Math.min(fitX, fitY, (props.drift ? 0.26 : 0.34) * width);
  // While exploring, the middles may spread a little further apart than the constellations
  // grow, to fill the sky.
  const spreadX = props.drift ? Math.min(fitX, unit * 1.4) : unit;
  const spreadY = props.drift ? Math.min(fitY, unit * 1.6) : unit;
  const middle = { x: (area.left + area.right) / 2, y: (area.top + area.bottom) / 2 };
  const boxMiddle = { x: (box.left + box.right) / 2, y: (box.top + box.bottom) / 2 };
  const spread = (spot: { x: number; y: number }) => ({
    x: (middle.x + (spot.x - boxMiddle.x) * spreadX) / width,
    y: (middle.y + (spot.y - boxMiddle.y) * spreadY) / height,
  });
  list.forEach((constellation, i) => {
    places.set(constellation.id, {
      ...spread(spots[i]!),
      unit: unit / width,
      alpha: props.drift || constellation.month === shownMonth ? 1 : 0,
      label: props.names ? 1 : 0,
      lift: 0,
    });
  });
  if (list.length) {
    const all = bounds(spots);
    const [topLeft, bottomRight] = [spread({ x: all.left, y: all.top }), spread({ x: all.right, y: all.bottom })];
    world = { left: topLeft.x, right: bottomRight.x, top: topLeft.y, bottom: bottomRight.y };
  } else world = { left: 0, right: 1, top: 0, bottom: 1 };
  // Planets around the edge of the constellations, the newest up to the right and older ones
  // further round, smaller and fainter, as if further away. A little in from the area's edge on
  // the bottom, so they don't sit under the cards.
  const planets = props.planets ?? [];
  const across = ((area.right - area.left) / 2) * 0.92;
  const down = ((area.bottom - area.top) / 2) * 0.95;
  const base = Math.min(width, height) * (props.drift ? 0.065 : 0.055);
  // On the Sky, the shown month's planet is the newest, with the few months before it.
  let newest = planets.length - 1;
  if (!props.drift && shownMonth !== undefined) {
    while (newest >= 0 && planets[newest]!.monthStart > shownMonth) newest--;
  }
  planets.forEach((planet, i) => {
    const age = newest - i;
    const angle = -0.3 * Math.PI + age * goldenAngle + orbit;
    const radius = (base * (0.45 + 0.55 * planet.size)) / (1 + 0.25 * Math.min(Math.max(age, 0), 8));
    // Room for its rings and moons, so it isn't cut off at the window's edges.
    const room = radius * 2.8;
    // While exploring, older ones sit further out, among their months' constellations.
    const out = props.drift ? Math.sqrt(Math.max(1, (age + 1) / skyPlanets)) : 1;
    let x = middle.x + Math.cos(angle) * across * out;
    let y = middle.y + Math.sin(angle) * down * out * (Math.sin(angle) > 0 ? 0.8 : 1);
    if (out === 1) {
      x = Math.min(width - room, Math.max(room, x));
      y = Math.max(room, y);
    } else {
      world.left = Math.min(world.left, (x - room) / width);
      world.right = Math.max(world.right, (x + room) / width);
      world.top = Math.min(world.top, (y - room) / height);
      world.bottom = Math.max(world.bottom, (y + room) / height);
    }
    const shownHere = props.drift || (age >= 0 && age < skyPlanets);
    places.set(planetKey(planet), {
      x: x / width,
      y: y / height,
      unit: radius / width,
      alpha: shownHere ? Math.max(0.35, 1 - (props.drift ? 0.06 : 0.1) * age) : 0,
      label: props.names ? 1 : 0,
      lift: 0,
    });
  });
  const satelliteRing = {
    x: middle.x / width,
    y: middle.y / height,
    across: (area.right - area.left) / 2 / width,
    down: (area.bottom - area.top) / 2 / height,
    alpha: 1,
  };
  return { places, ring: satelliteRing };
}

/** Moves each constellation a step toward where the layout puts it. Returns whether any are still on their way. */
function glide(elapsed: number, width: number, height: number): boolean {
  const follow = resting() ? 1 : 1 - Math.exp(-elapsed / (glideMs / 3));
  let moving = false;
  const { places: goals, ring: ringGoal } = targets(width, height);
  if (!ring) ring = { ...ringGoal };
  for (const key of ['x', 'y', 'across', 'down', 'alpha'] as const) {
    const gap = ringGoal[key] - ring[key];
    if (Math.abs(gap) < 0.0005) ring[key] = ringGoal[key];
    else {
      ring[key] += gap * follow;
      moving = true;
    }
  }
  for (const [id, target] of goals) {
    const place = shown.get(id);
    if (!place) {
      shown.set(id, { ...target });
      continue;
    }
    for (const key of ['x', 'y', 'unit', 'alpha', 'label', 'lift'] as const) {
      const gap = target[key] - place[key];
      if (Math.abs(gap) < 0.0005) place[key] = target[key];
      else {
        place[key] += gap * follow;
        moving = true;
      }
    }
  }
  // Forget constellations and planets that are gone, e.g. with the sample data.
  for (const id of shown.keys()) {
    if (!goals.has(id)) {
      shown.delete(id);
      shine.delete(id);
      stills.delete(id);
    }
  }
  const brighten = resting() ? 1 : 1 - Math.exp(-elapsed / (shineMs / 3));
  for (const id of goals.keys()) {
    const level = shine.get(id) ?? 0;
    const target = id === hovered ? 1 : 0;
    if (Math.abs(target - level) < 0.005) shine.set(id, target);
    else {
      shine.set(id, level + (target - level) * brighten);
      moving = true;
    }
  }
  const fade = resting() ? 1 : 1 - Math.exp(-elapsed / (satelliteFadeMs / 3));
  for (const [id, orbiter] of orbiters) {
    const visible = orbiter.gone ? 0 : 1;
    const glow = `satellite:${id}` === hovered ? 1 : 0;
    if (orbiter.gone && orbiter.alpha < 0.01) orbiters.delete(id);
    if (Math.abs(visible - orbiter.alpha) + Math.abs(glow - orbiter.glow) < 0.005) {
      [orbiter.alpha, orbiter.glow] = [visible, glow];
      continue;
    }
    orbiter.alpha += (visible - orbiter.alpha) * fade;
    orbiter.glow += (glow - orbiter.glow) * brighten;
    moving = true;
  }
  return moving;
}

function resize() {
  const sky = canvas.value;
  if (!sky) return;
  const scale = window.devicePixelRatio || 1;
  sky.width = Math.round(sky.clientWidth * scale);
  sky.height = Math.round(sky.clientHeight * scale);
  draw(clock(performance.now()));
}

// Moving the window to a screen with another pixel ratio doesn't change its size in CSS pixels,
// so the observer misses it. A query matches only the ratio it was made for, so it's made again
// each time the ratio changes.
let ratio: MediaQueryList | undefined;
function watchRatio() {
  ratio?.removeEventListener('change', ratioChanged);
  ratio = window.matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
  ratio.addEventListener('change', ratioChanged);
}
function ratioChanged() {
  watchRatio();
  resize();
}

/**
 * Where a point (as shares of the sky) ends up where exploring has panned and zoomed to, and with
 * the telescope drift, as shares too, with how much the drift zooms in.
 */
function drifted(x: number, y: number, depth: number) {
  x = 0.5 + (x - seen.x) * seen.zoom;
  y = 0.5 + (y - seen.y) * seen.zoom;
  const zoom = 1 + driftZoom * lens.zoom * depth;
  const centerX = 0.5 + lens.x / 2;
  const centerY = 0.5 + lens.y / 2;
  return {
    x: centerX + (x - centerX) * zoom - lens.x * driftPan * depth,
    y: centerY + (y - centerY) * zoom - lens.y * driftPan * depth,
    zoom,
  };
}

/** How far a constellation with a unit of `unit` canvas pixels has folded into a soft glow, from 0 to 1. */
function folded(unit: number): number {
  const pixels = unit / (window.devicePixelRatio || 1);
  return Math.min(1, Math.max(0, (foldFrom - pixels) / (foldFrom - foldTo)));
}

/** Whether a spot (in canvas pixels) is on a `width` by `height` canvas, or within `room` of it. */
const onScreen = (x: number, y: number, room: number, width: number, height: number) =>
  x + room >= 0 && x - room <= width && y + room >= 0 && y - room <= height;

/** A soft glow with no bright core, for a constellation folded away. */
function smudge(color: string): HTMLCanvasElement {
  const key = `smudge:${color}`;
  let glow = sprites.get(key);
  if (glow) return glow;
  glow = document.createElement('canvas');
  glow.width = glow.height = 64;
  const g = glow.getContext('2d')!;
  const gradient = g.createRadialGradient(32, 32, 0, 32, 32, 32);
  gradient.addColorStop(0, `${color}66`);
  gradient.addColorStop(0.4, `${color}26`);
  gradient.addColorStop(1, `${color}00`);
  g.fillStyle = gradient;
  g.fillRect(0, 0, 64, 64);
  sprites.set(key, glow);
  return glow;
}

/**
 * A constellation's glow from far away: the color of its stars together, and how bright it is,
 * from how many there are and how bright. Kept for each week's stars, which stay the same.
 */
const glows = new WeakMap<readonly SkyStar[], { color: string; light: number }>();
function farGlow(stars: readonly SkyStar[]) {
  let glow = glows.get(stars);
  if (glow) return glow;
  const sum = [0, 0, 0];
  let weight = 0;
  for (const star of stars) {
    const w = 0.3 + star.magnitude;
    for (let i = 0; i < 3; i++) sum[i]! += parseInt(star.color.slice(1 + i * 2, 3 + i * 2), 16) * w;
    weight += w;
  }
  const hex = sum
    .map((v) =>
      Math.round(v / (weight || 1))
        .toString(16)
        .padStart(2, '0'),
    )
    .join('');
  glow = {
    color: `#${hex}`,
    light: Math.min(1, 0.3 + 0.12 * Math.sqrt(stars.reduce((total, star) => total + star.magnitude, 0))),
  };
  glows.set(stars, glow);
  return glow;
}

/**
 * Images of constellations far enough away to be folding into a glow, where twinkling wouldn't
 * show, kept by id: drawn at about the size they're shown, and reused while it stays about the same.
 */
const stills = new Map<
  string,
  { canvas: HTMLCanvasElement; stars: readonly SkyStar[]; key: string; middle: { x: number; y: number } }
>();
/** How much a constellation's size can change before its image is drawn again. */
const stillStep = Math.log(1.15);

/**
 * Draws a constellation from its image, its middle at `x`, `y` and its unit `unit` canvas pixels,
 * `unzoomed` before exploring zoomed it.
 */
function drawStill(
  c: CanvasRenderingContext2D,
  constellation: SkyConstellation,
  x: number,
  y: number,
  unit: number,
  unzoomed: number,
) {
  const scale = window.devicePixelRatio || 1;
  const size = Math.exp(Math.round(Math.log(unit) / stillStep) * stillStep);
  const key = `${size.toFixed(2)}:${unzoomed.toFixed(1)}`;
  let image = stills.get(constellation.id);
  // Its stars are new whenever the week changes, e.g. as a session goes on.
  if (!image || image.key !== key || image.stars !== constellation.stars) {
    // Stars grow more slowly than the sky as you zoom in, so their size follows the square root.
    const starScale = scale * Math.sqrt(size / unzoomed);
    const pad = 18 * starScale;
    const canvas = image?.canvas ?? document.createElement('canvas');
    canvas.width = Math.ceil(2 * reach.x * size + pad * 2);
    canvas.height = Math.ceil(size + pad * 2);
    const g = canvas.getContext('2d')!;
    const middle = { x: canvas.width / 2, y: canvas.height / 2 };
    const at = (star: SkyStar) => ({ x: middle.x + star.x * size, y: middle.y + star.y * size });
    g.lineWidth = scale;
    g.strokeStyle = 'rgb(205 215 255)';
    g.globalAlpha = 0.14;
    g.beginPath();
    for (const [from, to] of constellation.lines) {
      const a = constellation.stars[from];
      const b = constellation.stars[to];
      if (!a || !b) continue;
      g.moveTo(at(a).x, at(a).y);
      g.lineTo(at(b).x, at(b).y);
    }
    g.stroke();
    for (const star of constellation.stars) {
      const { x: starX, y: starY } = at(star);
      const starSize = (10 + star.magnitude * 26) * starScale;
      g.globalAlpha = (0.35 + 0.65 * star.magnitude) * 0.85;
      g.drawImage(sprite(star.color), starX - starSize / 2, starY - starSize / 2, starSize, starSize);
    }
    image = { canvas, stars: constellation.stars, key, middle };
    stills.set(constellation.id, image);
  }
  const shrink = unit / size;
  c.drawImage(
    image.canvas,
    x - image.middle.x * shrink,
    y - image.middle.y * shrink,
    image.canvas.width * shrink,
    image.canvas.height * shrink,
  );
}

function draw(time: number) {
  const sky = canvas.value;
  // Nothing to draw while the window has no size (e.g. hidden).
  if (!sky || !context || !sky.width || !sky.height) return;
  const { width, height } = sky;
  const scale = window.devicePixelRatio || 1;
  const haze = 1 - shownClarity;
  const c = context;

  const night = c.createLinearGradient(0, 0, 0, height);
  night.addColorStop(0, '#060914');
  night.addColorStop(1, '#0f1530');
  c.globalAlpha = 1;
  c.fillStyle = night;
  c.fillRect(0, 0, width, height);
  // Light pollution washes the Milky Way out first.
  drawMilkyWay(c, { fill: props.milkyWay ?? 0, alpha: 1 - 0.85 * haze, ...seen });

  // Nebulae from supernovae and comets, beside their week's constellation and behind its stars.
  for (const event of props.events ?? []) {
    if (event.kind !== 'supernova' || !event.constellationId) continue;
    const progress = played(`event:${event.id}`, time);
    const spot = markSpot(event.constellationId, (event.x * 2 - 1) * 0.9, (event.y - 0.5) * 0.8, width, height, haze);
    if (spot && onScreen(spot.x, spot.y, spot.unit * 2, width, height) && (progress === undefined || progress >= 0)) {
      drawSupernova(c, event, spot.x, spot.y, spot.unit, spot.alpha, progress);
    }
  }
  for (const comet of props.comets ?? []) {
    const progress = played(`comet:${comet.id}`, time);
    const spot = markSpot(comet.constellationId, comet.x, comet.y, width, height, haze);
    // Far away, a comet folds away with its constellation's stars.
    const near = spot ? 1 - (exploringSky() ? folded(spot.unit) : 0) : 0;
    if (
      spot &&
      near > 0.01 &&
      onScreen(spot.x, spot.y, spot.unit * 4, width, height) &&
      (progress === undefined || progress >= 0)
    ) {
      drawComet(c, comet, spot.x, spot.y, spot.unit, spot.alpha * 0.85 * near, progress);
    }
  }
  c.globalAlpha = 1;

  const boxes: typeof drawnBoxes = [];
  c.font = `${12 * scale}px ${font}`;
  c.textAlign = 'center';
  c.textBaseline = 'top';
  c.lineWidth = scale;
  c.strokeStyle = 'rgb(205 215 255)';
  for (const constellation of props.constellations) {
    const place = shown.get(constellation.id);
    if (!place || place.alpha <= 0.01) continue;
    const unit = place.unit * width;
    const middleX = place.x * width;
    const middleY = place.y * height;
    const center = drifted(place.x, place.y, 1);
    const seenUnit = unit * seen.zoom;
    // Only what's on screen, with room for its stars' glow.
    if (!onScreen(center.x * width, center.y * height, reach.x * seenUnit + 40 * scale, width, height)) continue;
    const glow = Math.max(shine.get(constellation.id) ?? 0, place.lift);
    // The week the report shows cuts through the haze a little, so it reads behind the header.
    const dimmed = haze * (1 - 0.5 * place.lift);
    // Far away while exploring, its stars fold into a soft glow. The Sky always twinkles.
    const exploring = exploringSky();
    const fold = exploring ? folded(seenUnit) : 0;
    const box = {
      id: constellation.id,
      left: center.x * width - reach.x * seenUnit,
      right: center.x * width + reach.x * seenUnit,
      top: center.y * height - 0.5 * seenUnit,
      bottom: center.y * height + 0.5 * seenUnit + 16 * scale,
    };
    let below = box.bottom - 4 * scale;
    if (fold > 0) {
      const far = farGlow(constellation.stars);
      const across = 2 * reach.x * seenUnit + 24 * scale;
      const down = seenUnit + 24 * scale;
      c.globalAlpha = fold * far.light * (0.7 + 0.3 * glow) * place.alpha * (1 - 0.7 * dimmed);
      c.drawImage(smudge(far.color), center.x * width - across / 2, center.y * height - down / 2, across, down);
    }
    if (exploring && seenUnit < twinkleFrom * scale) {
      // Too small to see twinkle: drawn once and reused, fading as it folds into a glow.
      if (fold < 1) {
        c.globalAlpha = (1 - fold) * (0.85 + 0.15 * glow) * place.alpha * (1 - 0.7 * dimmed);
        drawStill(c, constellation, center.x * width, center.y * height, seenUnit, unit);
      }
    } else {
      const positions = constellation.stars.map((star) => {
        const depth = 0.5 + star.magnitude;
        const at = drifted((middleX + star.x * unit) / width, (middleY + star.y * unit) / height, depth);
        return { x: at.x * width, y: at.y * height, zoom: at.zoom };
      });

      // Faint lines first, so the stars sit on top of them.
      c.globalAlpha = (0.14 + 0.26 * glow) * place.alpha * (1 - 0.6 * dimmed);
      c.beginPath();
      for (const [from, to] of constellation.lines) {
        const a = positions[from];
        const b = positions[to];
        if (!a || !b) continue;
        c.moveTo(a.x, a.y);
        c.lineTo(b.x, b.y);
      }
      c.stroke();

      const grow = Math.sqrt(seen.zoom);
      constellation.stars.forEach((star, i) => {
        const at = positions[i]!;
        const size = (10 + star.magnitude * 26) * scale * (1 + (at.zoom - 1) / 2) * (1 + 0.2 * glow) * grow;
        let alpha = (0.35 + 0.65 * star.magnitude) * (1 - 0.7 * dimmed) * (0.85 + 0.15 * glow);
        if (!still.matches) {
          // Each star twinkles at its own pace, from where it sits.
          const wave = Math.sin((time / 1000) * (0.5 + (star.y + 0.5) * 0.9) + star.x * 100);
          alpha *= 0.82 + 0.18 * wave + star.sparkle * 0.25 * Math.max(0, wave);
        }
        c.globalAlpha = Math.min(1, Math.max(0, alpha * place.alpha));
        c.drawImage(sprite(star.color), at.x - size / 2, at.y - size / 2, size, size);
      });

      if (positions.length) {
        const xs = positions.map((p) => p.x);
        const ys = positions.map((p) => p.y);
        box.left = Math.min(...xs) - 12 * scale;
        box.right = Math.max(...xs) + 12 * scale;
        box.top = Math.min(...ys) - 12 * scale;
        box.bottom = Math.max(...ys) + 28 * scale;
        below = Math.max(...ys) + 12 * scale;
      }
    }
    boxes.push(box);
    if (place.label * glow > 0.01) {
      // Its name, beneath it while it's hovered.
      c.globalAlpha = place.label * glow * 0.95 * (1 - 0.4 * haze);
      c.fillStyle = '#f3f0e6';
      // Kept inside the window, so a name near the edge isn't cut off.
      const half = c.measureText(constellation.name).width / 2 + 6 * scale;
      const x = Math.min(width - half, Math.max(half, (box.left + box.right) / 2));
      c.fillText(constellation.name, x, below);
    }
  }
  drawnBoxes = boxes;
  aimAt(width, height);

  // Planets, nearer than the stars, so drawn over them.
  const planetSpots: typeof drawnPlanets = [];
  for (const planet of props.planets ?? []) {
    const key = planetKey(planet);
    const place = shown.get(key);
    if (!place || place.alpha <= 0.01) continue;
    const at = drifted(place.x, place.y, 1.6);
    const glow = shine.get(key) ?? 0;
    const x = at.x * width;
    const y = at.y * height;
    const radius = place.unit * width * (1 + (at.zoom - 1) / 2) * (1 + 0.06 * glow) * seen.zoom ** 0.8;
    if (!onScreen(x, y, radius * 3, width, height)) continue;
    // Too small to see turning, far away while exploring, it's drawn once and reused.
    const lively = circling() && radius > 6 * scale;
    // Like the week's constellation, the week's month's planet cuts through the haze a little.
    const alpha = place.alpha * (1 - 0.6 * haze * (1 - 0.5 * place.lift)) * (0.9 + 0.1 * glow);
    const happening = planet.event && played(`event:${planet.event.id}`, time);
    stampPlanet(c, planet, {
      x,
      y,
      radius,
      alpha,
      glow,
      tint: planet.tint,
      turn: lively ? orbit : 0,
      time,
      lively,
      event: happening === undefined ? undefined : Math.max(0, happening),
    });
    planetSpots.push({ key, x, y, radius });
    if (place.label * glow > 0.01) {
      // Its month and focus time, beneath it while it's hovered.
      c.globalAlpha = place.label * glow * 0.95 * (1 - 0.4 * haze);
      c.fillStyle = '#f3f0e6';
      const half = c.measureText(planet.label).width / 2 + 6 * scale;
      c.fillText(planet.label, Math.min(width - half, Math.max(half, x)), y + radius * 1.3 + 6 * scale);
    }
  }
  drawnPlanets = planetSpots;
  drawnSatellites = drawSatellites(c, time, width, height, haze);

  // Shooting stars, meteor showers and auroras, across the whole sky, and a supernova's flare
  // where its week has no constellation for it to stay in.
  for (const event of props.events ?? []) {
    const progress = played(`event:${event.id}`, time);
    if (progress === undefined || progress < 0) continue;
    if (event.kind === 'supernova' && !event.constellationId) {
      const unit = Math.min(width, height) * 0.3;
      drawSupernova(c, event, event.x * width, (0.1 + 0.4 * event.y) * height, unit, 1, progress);
    } else drawPassing(c, event, progress, width, height, time, still.matches);
  }
  c.globalAlpha = 1;

  if (haze > 0.01) {
    // Light pollution: a pale wash, thickest near the horizon, with the town's warm glow under it.
    c.globalAlpha = haze;
    const wash = c.createLinearGradient(0, 0, 0, height);
    wash.addColorStop(0, 'rgb(46 54 92 / 0.18)');
    wash.addColorStop(0.6, 'rgb(84 88 128 / 0.3)');
    wash.addColorStop(1, 'rgb(150 130 150 / 0.42)');
    c.fillStyle = wash;
    c.fillRect(0, 0, width, height);
    const town = c.createRadialGradient(width / 2, height * 1.25, 0, width / 2, height * 1.25, height * 0.9);
    town.addColorStop(0, 'rgb(230 170 120 / 0.28)');
    town.addColorStop(1, 'rgb(230 170 120 / 0)');
    c.fillStyle = town;
    c.fillRect(0, 0, width, height);
  }
  lastDrawn = time;
}

/** Draws the satellites going round, nearest of all, and returns where each was drawn. */
function drawSatellites(c: CanvasRenderingContext2D, time: number, width: number, height: number, haze: number) {
  const spots: typeof drawnSatellites = [];
  if (!ring || ring.alpha <= 0.01) return spots;
  const scale = window.devicePixelRatio || 1;
  for (const [id, { satellite, orbit, alpha: shownAlpha, glow }] of orbiters) {
    const key = `satellite:${id}`;
    const progress = played(key, time);
    // One that went onto the task list goes round as before until it leaves, and is gone after.
    if (satellite.tasked && progress === undefined && started.has(key)) continue;
    const leaving = progress !== undefined && progress >= 0 ? progress : undefined;
    const away = leaving ?? 0;
    const turn = still.matches ? 0 : (time / orbit.lapMs) * Math.PI * 2;
    // Leaving, it speeds up and spirals out.
    const theta = orbit.phase + orbit.direction * (turn + 1.6 * away * away);
    const out = 1 + 1.6 * away * away;
    const across = Math.cos(theta) * ring.across * width * orbit.across * out;
    const down = Math.sin(theta) * ring.down * height * orbit.down * out;
    const x = ring.x * width + across * Math.cos(orbit.tilt) - down * Math.sin(orbit.tilt);
    const y = ring.y * height + across * Math.sin(orbit.tilt) + down * Math.cos(orbit.tilt);
    // The lower half of its orbit is the near side, so it's a little bigger and brighter there.
    const near = Math.sin(theta);
    const at = drifted(x / width, y / height, 1.8);
    if (!Number.isFinite(at.x + at.y)) continue;
    const alpha =
      shownAlpha * ring.alpha * (1 - 0.5 * haze) * (0.7 + 0.3 * near) * (1 - away) ** 1.5 * (0.9 + 0.1 * glow);
    const spot = { key, x: at.x * width, y: at.y * height };
    if (!onScreen(spot.x, spot.y, 40 * scale, width, height)) continue;
    drawSatellite(c, {
      ...spot,
      size: 2.2 * scale * (0.85 + 0.15 * near) * (1 + (at.zoom - 1) / 2) * (1 + 0.15 * glow) * Math.sqrt(seen.zoom),
      angle: orbit.tilt + orbit.angle,
      alpha,
      blink: orbit.blink,
      time,
      still: still.matches,
      leaving,
    });
    if (leaving === undefined) spots.push(spot);
    if (props.satelliteText && glow > 0.01) {
      // What it says, beneath it while it's hovered, cut short if it's long.
      c.globalAlpha = glow * 0.95 * (1 - 0.4 * haze) * ring.alpha;
      c.fillStyle = '#f3f0e6';
      const text = fitText(c, satellite.text, satelliteTextWidth * scale);
      const half = c.measureText(text).width / 2 + 6 * scale;
      c.fillText(text, Math.min(width - half, Math.max(half, spot.x)), spot.y + 12 * scale);
    }
  }
  c.globalAlpha = 1;
  return spots;
}

/** `text`, cut short with an ellipsis to fit in `width` pixels. */
function fitText(c: CanvasRenderingContext2D, text: string, width: number): string {
  if (c.measureText(text).width <= width) return text;
  let length = text.length;
  while (length > 1 && c.measureText(`${text.slice(0, length).trimEnd()}…`).width > width) length--;
  return `${text.slice(0, length).trimEnd()}…`;
}

/**
 * Where a spot in a constellation (in its own units) is on the canvas, with the constellation's unit
 * in pixels and how visible it is, or undefined while the constellation isn't shown.
 */
function markSpot(id: string, x: number, y: number, width: number, height: number, haze: number) {
  const place = shown.get(id);
  if (!place || place.alpha <= 0.01) return undefined;
  const unit = place.unit * width;
  const at = drifted((place.x * width + x * unit) / width, (place.y * height + y * unit) / height, 0.7);
  // Gradients throw on a position that isn't a number, which would stop the sky drawing.
  if (!Number.isFinite(at.x + at.y + unit)) return undefined;
  return {
    x: at.x * width,
    y: at.y * height,
    unit: unit * seen.zoom * (1 + (at.zoom - 1) / 2),
    alpha: place.alpha * (1 - 0.6 * haze * (1 - 0.5 * place.lift)),
  };
}

/** Starts playing what's new in `play`, one after another. Under reduced motion, what stays just appears. */
function startPlaying(keys: readonly string[]) {
  let next = 0;
  const now = clock(performance.now());
  for (const key of keys) {
    if (started.has(key)) continue;
    started.add(key);
    const [kind, id] = [key.slice(0, key.indexOf(':')), key.slice(key.indexOf(':') + 1)];
    const found =
      kind === 'comet'
        ? props.comets?.find((c) => c.id === id) && 'comet'
        : kind === 'satellite'
          ? props.satellites?.find((s) => s.id === id && s.tasked) && 'satellite'
          : props.events?.find((e) => e.id === id)?.kind;
    if (!found) continue;
    const passing = found === 'shootingStar' || found === 'meteorShower' || found === 'aurora';
    if (still.matches && !passing) continue;
    plays.set(key, { start: now + next * playGapMs, kind: found });
    next++;
  }
  wake();
}

/** Whether planets circle and spin: only while exploring, and not at all under reduced motion. */
const circling = () => !!props.drift && props.week === undefined && !still.matches && !!props.planets?.length;

function tick(now: number) {
  const time = clock(now);
  const elapsed = lastTime ? time - lastTime : 0;
  lastTime = time;
  const turning = circling() && !paused;
  if (turning) orbit += (elapsed / planetLapMs) * Math.PI * 2;
  if (paused) shownClarity = props.clarity;
  const hazing = Math.abs(props.clarity - shownClarity) > 0.002;
  if (hazing) {
    shownClarity += (props.clarity - shownClarity) * (1 - Math.exp(-elapsed / (hazeDriftMs / 3)));
  } else {
    shownClarity = props.clarity;
  }
  const follow = 1 - Math.exp(-elapsed / (driftMs / 3));
  const panning = Math.abs(aim.x - lens.x) + Math.abs(aim.y - lens.y) + Math.abs(aim.zoom - lens.zoom) > 0.001;
  if (panning) {
    lens.x += (aim.x - lens.x) * follow;
    lens.y += (aim.y - lens.y) * follow;
    lens.zoom += (aim.zoom - lens.zoom) * follow;
  }
  // Exploring's pan and zoom: straight there while dragging, otherwise easing in, zooming evenly.
  if (!exploringSky()) view = { ...home };
  else if (!dragging) view = kept(view);
  const viewing =
    Math.abs(view.x - seen.x) + Math.abs(view.y - seen.y) + Math.abs(Math.log(view.zoom / seen.zoom)) > 0.0005;
  if (viewing && !dragging && !resting()) {
    const toward = 1 - Math.exp(-elapsed / (viewMs / 3));
    seen.x += (view.x - seen.x) * toward;
    seen.y += (view.y - seen.y) * toward;
    seen.zoom *= (view.zoom / seen.zoom) ** toward;
  } else Object.assign(seen, view);
  for (const [key, playing] of plays) if (time - playing.start > playLength(playing.kind)) plays.delete(key);
  const sky = canvas.value;
  // Places are shares of the sky's size, so they're only worked out while it has one.
  const sized = !!sky && sky.width > 0 && sky.height > 0;
  // Gliding and playing things draw every frame the screen does. Planets circling are too slow
  // to need that, so like twinkling they draw 60 times a second (or 20, behind other windows).
  const gliding = (sized ? glide(elapsed, sky.width, sky.height) : false) || (plays.size > 0 && !paused);
  // Under reduced motion or paused nothing twinkles, so the loop stops once everything has settled.
  const moving = !resting() || hazing;
  const every = props.batterySaver ? frameMs.saver : frameMs.smooth;
  if (!moving || panning || viewing || gliding || time - lastDrawn >= every) draw(time);
  // Nothing is drawn while the window is hidden; showing it again wakes the sky.
  const going = (moving || panning || viewing || gliding || turning) && document.visibilityState !== 'hidden';
  if (!going) frame = 0;
  else if (panning || viewing || gliding) frame = requestAnimationFrame(tick);
  else {
    frame = -1;
    nap = setTimeout(
      () => {
        nap = undefined;
        frame = requestAnimationFrame(tick);
      },
      Math.max(0, lastDrawn + every - time - 4),
    );
  }
}

function wake() {
  if (resting()) shownClarity = props.clarity;
  // Something changed, so no more sleeping until the next frame.
  if (nap !== undefined) {
    clearTimeout(nap);
    nap = undefined;
    frame = requestAnimationFrame(tick);
    return;
  }
  if (!frame) {
    lastTime = 0;
    frame = requestAnimationFrame(tick);
  }
}

const exploringSky = () => !!props.drift && props.week === undefined;
/** Whether the pointer is on a card or button, rather than the sky. */
const onPage = (target: EventTarget | null) =>
  target instanceof Element && !!target.closest('button, a, input, textarea, select, nav, .card');

/** The furthest out exploring can zoom: far enough to see everything. */
function leastZoom(): number {
  const across = world.right - world.left;
  const down = world.bottom - world.top;
  return Math.max(0.1, Math.min(1, 0.92 / across, 0.85 / down));
}

/**
 * `to`, kept zoomed between the least and most, and over what there is to see: its edge can come
 * in a little way from the window's, but no further.
 */
function kept(to: View): View {
  const zoom = Math.min(mostZoom, Math.max(leastZoom(), to.zoom));
  const inside = 0.35 / zoom;
  const between = (value: number, low: number, high: number) => {
    // Where it starts is always allowed, so going in and out of Explore doesn't jump.
    [low, high] = low + inside <= high - inside ? [low + inside, high - inside] : [(low + high) / 2, (low + high) / 2];
    return Math.min(Math.max(high, 0.5), Math.max(Math.min(low, 0.5), value));
  };
  return { x: between(to.x, world.left, world.right), y: between(to.y, world.top, world.bottom), zoom };
}

/** What was last told about the view, so it's only told again when that changes. */
let told = { zoom: 1, aimed: undefined as string | undefined };
/**
 * Tells the page how far in exploring is zoomed and which constellation is in the middle of the
 * view: the one there, or else the nearest within a little way of it.
 */
function aimAt(width: number, height: number) {
  let aimed: string | undefined;
  let zoom = 1;
  if (exploringSky()) {
    zoom = Math.round(seen.zoom * 100) / 100;
    const near = Math.min(width, height) * 0.2;
    let nearest = Infinity;
    for (const box of drawnBoxes) {
      const inside =
        box.left <= width / 2 && box.right >= width / 2 && box.top <= height / 2 && box.bottom >= height / 2;
      const away = Math.hypot((box.left + box.right - width) / 2, (box.top + box.bottom - height) / 2);
      if ((inside || away < near) && away < nearest) {
        nearest = away;
        aimed = box.id;
      }
    }
  }
  if (zoom === told.zoom && aimed === told.aimed) return;
  told = { zoom, aimed };
  emit('view', told);
}

/** Zooms by `factor` (above 1 is in) toward a spot on the sky (as shares of it), which stays put. */
function zoomAt(x: number, y: number, factor: number) {
  if (!exploringSky()) return;
  const zoom = Math.min(mostZoom, Math.max(leastZoom(), view.zoom * factor));
  const spot = { x: view.x + (x - 0.5) / view.zoom, y: view.y + (y - 0.5) / view.zoom };
  view = kept({ x: spot.x - (x - 0.5) / zoom, y: spot.y - (y - 0.5) / zoom, zoom });
  wake();
}

/** Starts dragging the sky around, while exploring. */
function grab(event: PointerEvent) {
  if (!exploringSky() || event.button !== 0 || onPage(event.target)) return;
  // Not selecting the page's text while dragging.
  event.preventDefault();
  dragging = {
    id: event.pointerId,
    x: event.clientX,
    y: event.clientY,
    from: { ...seen },
    speed: { x: 0, y: 0 },
    at: event.timeStamp,
  };
  view = { ...seen };
  document.documentElement.style.cursor = 'grabbing';
}

function drag(event: PointerEvent) {
  if (!dragging) return;
  const to = kept({
    x: dragging.from.x - (event.clientX - dragging.x) / window.innerWidth / dragging.from.zoom,
    y: dragging.from.y - (event.clientY - dragging.y) / window.innerHeight / dragging.from.zoom,
    zoom: dragging.from.zoom,
  });
  // How fast it's going, smoothed, for a soft stop when let go.
  const since = Math.max(1, event.timeStamp - dragging.at);
  dragging.speed.x = 0.7 * dragging.speed.x + (0.3 * (to.x - view.x)) / since;
  dragging.speed.y = 0.7 * dragging.speed.y + (0.3 * (to.y - view.y)) / since;
  dragging.at = event.timeStamp;
  view = to;
  wake();
}

/** Lets go of the sky, which drifts on a little and comes to a stop (except under reduced motion). */
function letGo(event: PointerEvent) {
  if (!dragging || event.pointerId !== dragging.id) return;
  const coast = still.matches || event.timeStamp - dragging.at > 80 ? 0 : 250;
  // Even a quick flick only drifts on a little way.
  const most = 0.2 / view.zoom;
  const on = (speed: number) => Math.min(most, Math.max(-most, speed * coast));
  view = kept({ x: view.x + on(dragging.speed.x), y: view.y + on(dragging.speed.y), zoom: view.zoom });
  dragging = undefined;
  document.documentElement.style.cursor = '';
  wake();
}

function wheel(event: WheelEvent) {
  if (!exploringSky() || onPage(event.target)) return;
  event.preventDefault();
  const lines = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? window.innerHeight : 1;
  // A pinch on a trackpad comes as the wheel with ctrl held, in smaller steps.
  const factor = Math.exp(-event.deltaY * lines * (event.ctrlKey ? 0.01 : 0.002));
  zoomAt(event.clientX / window.innerWidth, event.clientY / window.innerHeight, factor);
}

/** Zooms with + and -, pans with the arrow keys, and goes back to the start with 0. */
function press(event: KeyboardEvent) {
  if (!exploringSky() || event.metaKey || event.ctrlKey || event.altKey) return;
  if (event.target instanceof Element && event.target.closest('input, textarea, select')) return;
  const step = 0.15 / view.zoom;
  const pan = (x: number, y: number) => {
    view = kept({ x: view.x + x, y: view.y + y, zoom: view.zoom });
    wake();
  };
  const keys: Record<string, () => void> = {
    '+': () => zoomAt(0.5, 0.5, 1.4),
    '=': () => zoomAt(0.5, 0.5, 1.4),
    '-': () => zoomAt(0.5, 0.5, 1 / 1.4),
    '0': recenter,
    ArrowLeft: () => pan(-step, 0),
    ArrowRight: () => pan(step, 0),
    ArrowUp: () => pan(0, -step),
    ArrowDown: () => pan(0, step),
  };
  const action = keys[event.key];
  if (!action) return;
  event.preventDefault();
  action();
}

/** Glides back to where exploring started, around the latest weeks. */
function recenter() {
  if (!exploringSky()) return;
  view = kept(home);
  wake();
}

defineExpose({
  /** Zooms in (`factor` above 1) or out toward the middle, while exploring. */
  zoom: (factor: number) => zoomAt(0.5, 0.5, factor),
  recenter,
});

function point(event: PointerEvent) {
  if (dragging) {
    if (event.pointerId === dragging.id) drag(event);
    return;
  }
  hover(event);
  if (!props.drift || still.matches || event.pointerType !== 'mouse') return;
  aim = { x: (event.clientX / window.innerWidth) * 2 - 1, y: (event.clientY / window.innerHeight) * 2 - 1, zoom: 1 };
  wake();
}

/** Brightens the constellation under the pointer, unless the pointer is on a card or button. */
function hover(event: PointerEvent) {
  let found: string | undefined;
  if (props.names && props.week === undefined && !onPage(event.target)) {
    const scale = window.devicePixelRatio || 1;
    const x = event.clientX * scale;
    const y = event.clientY * scale;
    // Satellites first, then planets, since they're drawn over the stars.
    const satellite = props.satelliteText
      ? drawnSatellites.find((s) => Math.hypot(x - s.x, y - s.y) <= 12 * scale)?.key
      : undefined;
    found =
      satellite ??
      [...drawnPlanets].reverse().find((p) => Math.hypot(x - p.x, y - p.y) <= p.radius * 1.3)?.key ??
      drawnBoxes.find((box) => x >= box.left && x <= box.right && y >= box.top && y <= box.bottom)?.id;
  }
  if (found !== hovered) {
    hovered = found;
    wake();
  }
}

/** Pauses the sky while the window isn't focused, and carries on when it is again. */
function focusChanged() {
  const away = !document.hasFocus();
  if (away === paused) return;
  const now = performance.now();
  if (away) pausedAt = now;
  else pausedMs += now - pausedAt;
  paused = away;
  lastTime = 0;
  wake();
}

function settle() {
  aim = { x: 0, y: 0, zoom: 0 };
  hovered = undefined;
  wake();
}

/** Leaving Explore lets go of the sky, which glides back to where it was, and forgets far-away images. */
function stopExploring() {
  dragging = undefined;
  stills.clear();
  document.documentElement.style.cursor = '';
  settle();
}

watch(
  () => [
    props.constellations,
    props.planets,
    props.clarity,
    props.names,
    props.week,
    props.month,
    props.drift,
    props.comets,
    props.events,
    props.satellites,
    props.batterySaver,
  ],
  wake,
);
watch(
  () => props.satellites,
  (satellites) => syncSatellites(satellites ?? []),
  { immediate: true },
);
watch(
  () => props.play,
  (keys) => keys && startPlaying(keys),
  { immediate: true },
);
watch(
  () => props.planets,
  (planets) => forgetPlanets(new Set(planets?.map((planet) => planet.id))),
);
watch(
  () => props.drift,
  (drift) => !drift && stopExploring(),
);
watch(
  () => props.names,
  (names) => !names && (hovered = undefined),
);
const observer = new ResizeObserver(resize);

onMounted(() => {
  context = canvas.value!.getContext('2d');
  font = getComputedStyle(document.body).fontFamily || font;
  observer.observe(canvas.value!);
  watchRatio();
  still.addEventListener('change', wake);
  window.addEventListener('pointermove', point);
  window.addEventListener('pointerdown', grab);
  window.addEventListener('pointerup', letGo);
  window.addEventListener('pointercancel', letGo);
  window.addEventListener('wheel', wheel, { passive: false });
  window.addEventListener('keydown', press);
  window.addEventListener('focus', focusChanged);
  window.addEventListener('blur', focusChanged);
  document.addEventListener('visibilitychange', wake);
  document.documentElement.addEventListener('pointerleave', settle);
  wake();
});

onUnmounted(() => {
  cancelAnimationFrame(frame);
  clearTimeout(nap);
  observer.disconnect();
  ratio?.removeEventListener('change', ratioChanged);
  still.removeEventListener('change', wake);
  window.removeEventListener('pointermove', point);
  window.removeEventListener('pointerdown', grab);
  window.removeEventListener('pointerup', letGo);
  window.removeEventListener('pointercancel', letGo);
  window.removeEventListener('wheel', wheel);
  window.removeEventListener('keydown', press);
  window.removeEventListener('focus', focusChanged);
  window.removeEventListener('blur', focusChanged);
  document.removeEventListener('visibilitychange', wake);
  document.documentElement.removeEventListener('pointerleave', settle);
});
</script>

<template>
  <canvas ref="canvas" class="sky" aria-hidden="true" />
</template>

<style scoped>
.sky {
  position: fixed;
  inset: 0;
  z-index: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}
</style>
