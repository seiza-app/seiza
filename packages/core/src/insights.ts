import type { Activity } from './generated/Activity';
import type { GateVisitRecord } from './generated/GateVisitRecord';
import type { SessionRecord } from './generated/SessionRecord';
import { maxPlanLength } from './settings';
import { cutChars } from './text';

// The weekly report, worked out from what the desktop app logged. Weeks are seven days in local
// time, starting on the first day of the week set on the user's computer, so a day or an hour
// here is the one on their clock, and a week with a daylight saving change is still seven days.

const minuteMs = 60_000;

/**
 * Midnight at the start of the week `at` falls in, local time (epoch ms). `firstDay` is the day
 * weeks start on, 0 being Sunday as in `Date.getDay`; Monday unless the computer says otherwise.
 */
export function weekStart(at: number, firstDay = 1): number {
  const date = new Date(at);
  // Built from the date alone: where a day has no midnight (clocks going forward at 00:00, as in
  // Chile), it starts at 01:00, and every day of that week has to land on the same time.
  const offset = (date.getDay() - firstDay + 7) % 7;
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() - offset).getTime();
}

/** The start of the week `weeks` after the one starting at `start`, or before it if negative. */
export function addWeeks(start: number, weeks: number): number {
  const date = new Date(start);
  // From the date alone too, so a week starting at 01:00 doesn't carry that hour to the next.
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + weeks * 7).getTime();
}

/** Which day of the week starting at `start` the time `at` falls on, its first day being 0. */
function dayOfWeek(start: number, at: number): number {
  const day = new Date(at);
  day.setHours(0, 0, 0, 0);
  // Rounded, since a day with a daylight saving change is 23 or 25 hours long.
  return Math.round((day.getTime() - start) / (24 * 60 * minuteMs));
}

/** A length of time in hours and minutes: "4h 05m", "35m" or "0m". */
export function formatDuration(ms: number): string {
  const minutes = Math.round(Math.max(0, ms) / minuteMs);
  const hours = Math.floor(minutes / 60);
  return hours ? `${hours}h ${String(minutes % 60).padStart(2, '0')}m` : `${minutes}m`;
}

/** An hour of the day (0–23) on a 12-hour clock: "2pm" or "12am". */
export function formatHour(hour: number): string {
  return `${hour % 12 || 12}${hour < 12 ? 'am' : 'pm'}`;
}

/** A two-hour stretch of the day, starting at `hour` (0–23): "2–4pm", "11am–1pm" or "12–2am". */
export function formatHourRange(hour: number): string {
  const end = (hour + 2) % 24;
  const clock = (h: number) => String(h % 12 || 12);
  const half = (h: number) => (h < 12 ? 'am' : 'pm');
  const from = half(hour) === half(end) ? clock(hour) : clock(hour) + half(hour);
  return `${from}–${clock(end)}${half(end)}`;
}

export interface WeeklyReport {
  /** Midnight at the start of the week, local time (epoch ms). */
  start: number;
  /** Midnight at the start of the next week. */
  end: number;
  focus: FocusSummary;
  distractions: DistractionSummary;
  satellites: SatelliteSummary;
  /** Up to three tips, each only when there's enough data behind it. */
  suggestions: string[];
  /** An if-then plan drafted from the distraction tip, if there's one and a profile to give it to. */
  planIdea?: PlanIdea;
}

/** An if-then plan for the profile whose sessions a site pulled at most, drafted from why it did. */
export interface PlanIdea {
  profileId: string;
  recipeId: string;
  plan: string;
}

export interface FocusSummary {
  focusedMs: number;
  /** Focused time the week before, for the trend, and each of its days. */
  lastWeekMs: number;
  lastWeekDays: number[];
  /** Time spent after the timer was done. */
  overtimeMs: number;
  sessions: number;
  longestMs: number;
  /** Focused time each day, the week's first day first. A session counts on the day it started. */
  days: number[];
  /** The most focused first, by id under their latest name. */
  profiles: { id: string; name: string; focusedMs: number; sessions: number }[];
  /** Sessions ended early with an emergency pass. */
  emergencyPasses: number;
  /** Sessions ended for being idle. */
  idle: number;
  /** Sessions that had a browser closed for running without the extension. */
  broken: number;
  /** Goals set at the start of sessions (PLAN §7), and how "Did you do it?" was answered. */
  goals: { set: number; met: number; missed: number };
}

export interface DistractionSummary {
  /** Trips to redirected sites. */
  visits: number;
  lastWeekVisits: number;
  /** Visits that went back to focus instead of continuing to the site. */
  turnedBack: number;
  /** The most visited first. */
  sites: SiteDistractions[];
  /** Visits in each hour of the day, midnight first. */
  hours: number[];
  /** Answers to "why are you here?", the most common first. */
  whys: { why: string; visits: number }[];
  /** What was typed at the intention gate, grouped ignoring case; the five most common. */
  intentions: { text: string; count: number }[];
}

export interface SiteDistractions {
  recipeId: string;
  visits: number;
  /** The reason given for most visits, if one stands out. */
  why?: string;
  /** When most visits happen, as the first hour of a two-hour stretch, if they cluster. */
  peakHour?: number;
}

/** The satellites launched in the week, and what became of them. */
export interface SatelliteSummary {
  launched: number;
  /** Still in orbit, or back and waiting to be sorted. */
  waiting: number;
  tasks: number;
  done: number;
  deleted: number;
  /** Times they were sent back into orbit. */
  snoozes: number;
}

/**
 * The report for the week starting at `start`. `activity` should cover that week and the one
 * before it, for the trends. `siteName` turns a recipe id into the name the suggestions use.
 */
export function weeklyReport(
  activity: Activity,
  start: number,
  siteName: (recipeId: string) => string = (id) => id,
): WeeklyReport {
  const end = addWeeks(start, 1);
  const inWeek = (at: number) => at >= start && at < end;
  const lastWeek = (at: number) => at >= addWeeks(start, -1) && at < start;
  const sessions = activity.sessions.filter((s) => inWeek(s.startedAt));
  const visits = activity.gateVisits.filter((v) => inWeek(v.at));
  const satellites = activity.satellites.filter((s) => inWeek(s.launchedAt));

  const focus = focusSummary(sessions, start);
  const before = focusSummary(
    activity.sessions.filter((s) => lastWeek(s.startedAt)),
    addWeeks(start, -1),
  );
  focus.lastWeekMs = before.focusedMs;
  focus.lastWeekDays = before.days;

  const distractions = distractionSummary(visits);
  distractions.lastWeekVisits = activity.gateVisits.filter((v) => lastWeek(v.at)).length;

  const count = (...states: string[]) => satellites.filter((s) => states.includes(s.state)).length;
  const satelliteSummary: SatelliteSummary = {
    launched: satellites.length,
    waiting: count('orbiting', 'back'),
    tasks: count('task'),
    done: count('done'),
    deleted: count('deleted'),
    snoozes: sum(satellites.map((s) => s.snoozes)),
  };

  const tipSite = distractionTipSite(distractions);
  const suggestions = [
    tipSite && distractionTip(tipSite, siteName),
    bestTimeTip(sessions, focus),
    timerTip(sessions),
    distractions.visits >= 5 && satellites.length === 0
      ? 'Next time a thought pulls you away, launch it as a satellite instead. It comes back after the session.'
      : undefined,
  ].filter((tip) => tip !== undefined);

  return {
    start,
    end,
    focus,
    distractions,
    satellites: satelliteSummary,
    suggestions: suggestions.slice(0, 3),
    planIdea: tipSite && planIdea(tipSite, visits, activity.sessions, siteName),
  };
}

function focusSummary(sessions: SessionRecord[], start: number): FocusSummary {
  const days = Array<number>(7).fill(0);
  const profiles = new Map<string, FocusSummary['profiles'][number]>();
  for (const session of sessions) {
    const day = dayOfWeek(start, session.startedAt);
    if (day >= 0 && day < 7) days[day]! += session.focusedMs;
    // By id, under the latest name, so a renamed profile isn't split in two.
    const profile = profiles.get(session.profileId) ?? { id: session.profileId, name: '', focusedMs: 0, sessions: 0 };
    profile.name = session.profileName;
    profile.focusedMs += session.focusedMs;
    profile.sessions += 1;
    profiles.set(session.profileId, profile);
  }
  return {
    focusedMs: sum(sessions.map((s) => s.focusedMs)),
    lastWeekMs: 0,
    lastWeekDays: Array<number>(7).fill(0),
    overtimeMs: sum(sessions.map((s) => s.overtimeMs)),
    sessions: sessions.length,
    longestMs: Math.max(0, ...sessions.map((s) => s.focusedMs + s.overtimeMs)),
    days,
    profiles: [...profiles.values()].sort((a, b) => b.focusedMs - a.focusedMs),
    emergencyPasses: sessions.filter((s) => s.endReason === 'emergencyPass').length,
    idle: sessions.filter((s) => s.endReason === 'idle').length,
    broken: sessions.filter((s) => s.broken).length,
    goals: {
      set: sessions.filter((s) => s.goal).length,
      met: sessions.filter((s) => s.goal && s.goalMet === true).length,
      missed: sessions.filter((s) => s.goal && s.goalMet === false).length,
    },
  };
}

function distractionSummary(visits: GateVisitRecord[]): DistractionSummary {
  const bySite = groupBy(visits, (v) => v.recipeId);
  const sites = [...bySite].map(([recipeId, siteVisits]) => ({
    recipeId,
    visits: siteVisits.length,
    why: mainWhy(siteVisits),
    peakHour: peakHour(siteVisits),
  }));

  const intentions = groupBy(
    visits.flatMap((v) => {
      const text = v.intention?.trim().replace(/\s+/g, ' ');
      return text ? [text] : [];
    }),
    (text) => text.toLowerCase(),
  );

  return {
    visits: visits.length,
    lastWeekVisits: 0,
    turnedBack: visits.filter((v) => !v.continued).length,
    sites: sites.sort((a, b) => b.visits - a.visits),
    hours: hourCounts(visits),
    whys: [
      ...groupBy(
        visits.filter((v) => v.why),
        (v) => v.why!,
      ),
    ]
      .map(([why, answered]) => ({ why, visits: answered.length }))
      .sort((a, b) => b.visits - a.visits),
    intentions: [...intentions.values()]
      // Shown as most recently typed.
      .map((texts) => ({ text: texts[texts.length - 1]!, count: texts.length }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 5),
  };
}

/** The answer to "why?" given for at least half of the answered visits, if there are three. */
function mainWhy(visits: GateVisitRecord[]): string | undefined {
  const answered = visits.flatMap((v) => (v.why ? [v.why] : []));
  if (answered.length < 3) return undefined;
  const [why, times] = [...groupBy(answered, (w) => w)].sort((a, b) => b[1].length - a[1].length)[0]!;
  return times.length * 2 >= answered.length ? why : undefined;
}

/** The two-hour stretch with at least half of four or more visits, by its first hour. */
function peakHour(visits: GateVisitRecord[]): number | undefined {
  if (visits.length < 4) return undefined;
  const hours = hourCounts(visits);
  // Running past midnight, so 11pm–1am counts as one stretch.
  const visitsAt = (hour: number) => hours[hour % 24]!;
  const stretch = (hour: number) => visitsAt(hour) + visitsAt(hour + 1);
  // On a tie, the stretch that starts with the busier hour, so visits all at 2pm read as 2–4pm.
  const busier = (a: number, b: number) => stretch(b) - stretch(a) || visitsAt(b) - visitsAt(a);
  const best = hours.map((_, hour) => hour).reduce((a, b) => (busier(a, b) > 0 ? b : a));
  return stretch(best) * 2 >= visits.length ? best : undefined;
}

function hourCounts(visits: GateVisitRecord[]): number[] {
  const hours = Array<number>(24).fill(0);
  for (const visit of visits) hours[new Date(visit.at).getHours()]! += 1;
  return hours;
}

const whyTips: Record<string, string> = {
  stuck: 'Next time, try writing down the smallest next step instead.',
  bored: 'A shorter timer might stop the task from dragging.',
  tired: 'A real break, away from the screen, might help more.',
  curious: 'Try launching the question as a satellite and looking it up after the session.',
};

/** The most visited site with a reason that stands out, for the distraction tip. */
function distractionTipSite(distractions: DistractionSummary): SiteDistractions | undefined {
  return distractions.sites.find((s) => s.visits >= 5 && s.why && whyTips[s.why]);
}

function distractionTip(site: SiteDistractions, siteName: (id: string) => string): string {
  const when = site.peakHour === undefined ? '' : `, mostly ${formatHourRange(site.peakHour)}`;
  return `You reach for ${siteName(site.recipeId)} when ${site.why}${when}. ${whyTips[site.why!]}`;
}

// The same advice as the tips, as a plan to decide on ahead of time (PLAN §12).
const planDrafts: Record<string, (site: string) => string> = {
  stuck: (site) => `When I'm stuck and want ${site}, I'll write down the smallest next step.`,
  bored: (site) => `When I'm bored and want ${site}, I'll switch to another part of the task.`,
  tired: (site) => `When I'm tired and want ${site}, I'll take five minutes away from the screen.`,
  curious: (site) => `When I'm curious and want ${site}, I'll launch the question as a satellite.`,
};

/**
 * A plan for the tip's site, given to the profile whose sessions most of its visits came in.
 * Visits only happen during sessions, so each is matched to the one it fell in.
 */
function planIdea(
  site: SiteDistractions,
  visits: GateVisitRecord[],
  sessions: SessionRecord[],
  siteName: (id: string) => string,
): PlanIdea | undefined {
  const draft = planDrafts[site.why!];
  const profileIds = visits.flatMap((v) => {
    const session = v.recipeId === site.recipeId && sessions.find((s) => v.at >= s.startedAt && v.at <= s.endedAt);
    return session ? [session.profileId] : [];
  });
  const [profileId] = [...groupBy(profileIds, (id) => id)].sort((a, b) => b[1].length - a[1].length)[0] ?? [];
  if (!draft || !profileId) return undefined;
  return { profileId, recipeId: site.recipeId, plan: cutChars(draft(siteName(site.recipeId)), maxPlanLength) };
}

const partsOfDay = [
  { name: 'morning', from: 5, to: 12 },
  { name: 'afternoon', from: 12, to: 17 },
  // Running past midnight, until 5am.
  { name: 'evening', from: 17, to: 29 },
];

/** When at least five sessions run clearly longest at one part of the day. */
function bestTimeTip(sessions: SessionRecord[], focus: FocusSummary): string | undefined {
  if (sessions.length < 5) return undefined;
  const partOf = (s: SessionRecord) => {
    const hour = new Date(s.startedAt).getHours();
    const shifted = hour < 5 ? hour + 24 : hour;
    return partsOfDay.find((p) => shifted >= p.from && shifted < p.to)!;
  };
  const average = (list: SessionRecord[]) => sum(list.map((s) => s.focusedMs)) / list.length;
  const parts = partsOfDay
    .map((part) => ({ part, sessions: sessions.filter((s) => partOf(s) === part) }))
    .filter((p) => p.sessions.length >= 2);
  if (parts.length < 2) return undefined;
  const best = parts.reduce((a, b) => (average(b.sessions) > average(a.sessions) ? b : a));
  const others = sessions.filter((s) => partOf(s) !== best.part);
  if (average(best.sessions) < average(others) * 1.25) return undefined;
  const minutes = Math.round(average(best.sessions) / minuteMs);
  return (
    `Your longest focus comes in the ${best.part.name}, about ${minutes} minutes a session. ` +
    `Try scheduling ${focus.profiles[0]!.name} then.`
  );
}

/** When most of a profile's timed sessions run well past the timer, or stop well short of it. */
function timerTip(sessions: SessionRecord[]): string | undefined {
  const timed = sessions.filter((s) => s.timerMinutes && s.endReason !== 'idle');
  const byProfile = [...groupBy(timed, (s) => s.profileId).values()].sort((a, b) => b.length - a.length);
  const runs = byProfile[0];
  if (!runs || runs.length < 3) return undefined;
  const name = runs[runs.length - 1]!.profileName;
  const timerMs = median(runs.map((s) => s.timerMinutes! * minuteMs));
  const roundedMinutes = (ms: number) => Math.max(10, Math.round(ms / minuteMs / 5) * 5);

  const over = runs.filter((s) => s.overtimeMs >= 5 * minuteMs);
  if (over.length * 5 >= runs.length * 3) {
    const minutes = roundedMinutes(median(runs.map((s) => s.focusedMs + s.overtimeMs)));
    if (minutes * minuteMs > timerMs) {
      return `Your ${name} sessions usually run past the timer. A ${minutes}-minute timer might fit better.`;
    }
  }
  const short = runs.filter((s) => s.focusedMs <= s.timerMinutes! * minuteMs - 5 * minuteMs);
  if (short.length * 5 >= runs.length * 3) {
    const minutes = roundedMinutes(median(runs.map((s) => s.focusedMs)));
    if (minutes * minuteMs < timerMs) {
      return `Your ${name} sessions usually end before the timer. Try a ${minutes}-minute timer instead.`;
    }
  }
  return undefined;
}

function sum(values: number[]): number {
  return values.reduce((a, b) => a + b, 0);
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle]! : (sorted[middle - 1]! + sorted[middle]!) / 2;
}

/** Items grouped by key, keeping the order they came in. */
function groupBy<T>(items: T[], key: (item: T) => string): Map<string, T[]> {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const k = key(item);
    groups.set(k, [...(groups.get(k) ?? []), item]);
  }
  return groups;
}
