import { describe, expect, it } from 'vitest';
import type { Activity } from './generated/Activity';
import type { GateVisitRecord } from './generated/GateVisitRecord';
import type { SatelliteRecord } from './generated/SatelliteRecord';
import type { SessionRecord } from './generated/SessionRecord';
import { addWeeks, formatDuration, formatHour, formatHourRange, weeklyReport, weekStart } from './insights';

const MIN = 60_000;
// Times are built in local time, so the tests pass in any time zone.
const monday = new Date(2026, 9, 26).getTime();
/** `day` days after Monday 26 October 2026, at `hour`:`minute` local time. */
const at = (day: number, hour: number, minute = 0) => new Date(2026, 9, 26 + day, hour, minute).getTime();

let ids = 0;
function session(startedAt: number, minutes: number, extra: Partial<SessionRecord> = {}): SessionRecord {
  return {
    id: String(++ids),
    profileId: 'deep',
    profileName: 'Deep Work',
    startedAt,
    endedAt: startedAt + minutes * MIN,
    timerMinutes: null,
    focusedMs: minutes * MIN,
    overtimeMs: 0,
    idleTailMs: 0,
    endReason: 'stopped',
    broken: false,
    goal: null,
    goalMet: null,
    ...extra,
  };
}

function visit(when: number, extra: Partial<GateVisitRecord> = {}): GateVisitRecord {
  return {
    id: String(++ids),
    recipeId: 'reddit',
    at: when,
    destination: 'gate',
    why: null,
    intention: null,
    continued: false,
    ...extra,
  };
}

function satellite(launchedAt: number, state: SatelliteRecord['state'], snoozes = 0): SatelliteRecord {
  return { launchedAt, state, stateAt: launchedAt, snoozes, taskedAt: null };
}

const activity = (parts: Partial<Activity>): Activity => ({
  sessions: [],
  gateVisits: [],
  satellites: [],
  firstLoggedAt: null,
  ...parts,
});

const siteNames: Record<string, string> = { reddit: 'Reddit', youtube: 'YouTube' };
const report = (parts: Partial<Activity>) => weeklyReport(activity(parts), monday, (id) => siteNames[id] ?? id);

describe('weeks', () => {
  it('start at midnight on Monday, local time', () => {
    expect(weekStart(at(2, 15, 30))).toBe(monday);
    expect(weekStart(at(6, 23, 59))).toBe(monday);
    expect(weekStart(monday)).toBe(monday);
    expect(weekStart(monday - 1)).toBe(new Date(2026, 9, 19).getTime());
  });

  it('start on the first day of the week set on the computer', () => {
    const sunday = new Date(2026, 9, 25).getTime();
    expect(weekStart(at(2, 15, 30), 0)).toBe(sunday);
    expect(weekStart(sunday, 0)).toBe(sunday);
    expect(weekStart(sunday - 1, 0)).toBe(new Date(2026, 9, 18).getTime());
    // Saturday, as in much of the Middle East.
    expect(weekStart(at(-1, 9), 6)).toBe(new Date(2026, 9, 24).getTime());
  });

  it('have one start, even where a day has no midnight', () => {
    // Chile's clocks go forward at midnight, so in Santiago (one of the zones the test script
    // runs in) Sunday 6 September 2026 starts at 01:00. Elsewhere this passes without testing much.
    if (Intl.DateTimeFormat().resolvedOptions().timeZone === 'America/Santiago') {
      expect(new Date(2026, 8, 6).getHours()).toBe(1);
    }
    const day = (date: number, hour = 12) => new Date(2026, 8, date, hour).getTime();
    // The week from Monday 31 August, ending on the day with no midnight.
    for (const date of [0, 1, 2, 3, 4, 5]) {
      expect(weekStart(day(date))).toBe(weekStart(day(6, 9)));
    }
    // The week starting on that day, and the ones either side of it.
    const sunday = weekStart(day(6), 0);
    expect(weekStart(day(12), 0)).toBe(sunday);
    expect(addWeeks(sunday, 1)).toBe(weekStart(day(13), 0));
    expect(addWeeks(sunday, -1)).toBe(weekStart(day(5), 0));
  });

  it('step a calendar week at a time, even over a daylight saving change', () => {
    for (const weeks of [-2, -1, 1, 2]) {
      expect(addWeeks(monday, weeks)).toBe(new Date(2026, 9, 26 + weeks * 7).getTime());
    }
  });
});

describe('formatDuration', () => {
  it('shows hours and minutes', () => {
    expect(formatDuration(0)).toBe('0m');
    expect(formatDuration(35 * MIN)).toBe('35m');
    expect(formatDuration(245 * MIN + 20_000)).toBe('4h 05m');
    expect(formatDuration(119.6 * MIN)).toBe('2h 00m');
  });
});

describe('formatHourRange', () => {
  it('reads like a clock', () => {
    expect([0, 9, 12, 23].map(formatHour)).toEqual(['12am', '9am', '12pm', '11pm']);
    expect(formatHourRange(14)).toBe('2–4pm');
    expect(formatHourRange(10)).toBe('10am–12pm');
    expect(formatHourRange(11)).toBe('11am–1pm');
    expect(formatHourRange(0)).toBe('12–2am');
    expect(formatHourRange(23)).toBe('11pm–1am');
  });
});

describe('weeklyReport', () => {
  it('counts days from the start of the week, whichever day it is', () => {
    const sunday = new Date(2026, 9, 25).getTime();
    const sessions = [session(at(-1, 9), 30), session(at(0, 9), 20)];
    const { focus } = weeklyReport(activity({ sessions }), sunday);
    expect(focus.days).toEqual([30, 20, 0, 0, 0, 0, 0].map((m) => m * MIN));
  });

  it('sums up focus by day and profile, with last week for the trend', () => {
    const { focus } = report({
      sessions: [
        session(at(-3, 9), 40),
        session(at(0, 9), 50, { overtimeMs: 10 * MIN }),
        session(at(0, 14), 25, { profileId: 'write', profileName: 'Writing', endReason: 'idle' }),
        // Renamed later in the week, and still the same profile.
        session(at(2, 9), 30, { profileName: 'Deep work', endReason: 'emergencyPass', broken: true }),
        session(at(7, 9), 60),
      ],
    });
    expect(focus).toEqual({
      focusedMs: 105 * MIN,
      lastWeekMs: 40 * MIN,
      lastWeekDays: [0, 0, 0, 0, 40, 0, 0].map((m) => m * MIN),
      overtimeMs: 10 * MIN,
      sessions: 3,
      longestMs: 60 * MIN,
      days: [75, 0, 30, 0, 0, 0, 0].map((m) => m * MIN),
      profiles: [
        { id: 'deep', name: 'Deep work', focusedMs: 80 * MIN, sessions: 2 },
        { id: 'write', name: 'Writing', focusedMs: 25 * MIN, sessions: 1 },
      ],
      emergencyPasses: 1,
      idle: 1,
      broken: 1,
      goals: { set: 0, met: 0, missed: 0 },
    });
  });

  it('finds when, where and why distractions happen', () => {
    const { distractions } = report({
      gateVisits: [
        visit(at(-1, 14)),
        visit(at(0, 14, 5), { why: 'stuck', intention: 'Check  messages' }),
        visit(at(1, 15, 30), { why: 'stuck', intention: 'check messages ' }),
        visit(at(2, 14, 45), { why: 'bored', continued: true }),
        visit(at(3, 21), { why: 'stuck' }),
        visit(at(4, 10), { recipeId: 'youtube', intention: 'one video' }),
      ],
    });
    expect(distractions).toMatchObject({
      visits: 5,
      lastWeekVisits: 1,
      turnedBack: 4,
      sites: [
        { recipeId: 'reddit', visits: 4, why: 'stuck', peakHour: 14 },
        { recipeId: 'youtube', visits: 1, why: undefined, peakHour: undefined },
      ],
      whys: [
        { why: 'stuck', visits: 3 },
        { why: 'bored', visits: 1 },
      ],
      intentions: [
        { text: 'check messages', count: 2 },
        { text: 'one video', count: 1 },
      ],
    });
    expect(distractions.hours[14]).toBe(2);
    expect(distractions.hours[15]).toBe(1);
  });

  it("doesn't call a reason or time a pattern without enough visits behind it", () => {
    const { distractions } = report({
      gateVisits: [
        visit(at(0, 9), { why: 'stuck' }),
        visit(at(1, 13), { why: 'bored' }),
        visit(at(2, 17), { why: 'tired' }),
        visit(at(3, 21), { why: 'stuck' }),
      ],
    });
    expect(distractions.sites[0]).toMatchObject({ why: 'stuck', peakHour: undefined });
    const fewer = report({ gateVisits: [visit(at(0, 9), { why: 'stuck' }), visit(at(1, 9), { why: 'stuck' })] });
    expect(fewer.distractions.sites[0]).toMatchObject({ why: undefined, peakHour: undefined });
  });

  it('counts what became of the satellites launched', () => {
    const { satellites } = report({
      satellites: [
        satellite(at(-1, 9), 'done'),
        satellite(at(0, 9), 'orbiting', 2),
        satellite(at(0, 10), 'back'),
        satellite(at(1, 9), 'task', 1),
        satellite(at(2, 9), 'done'),
        satellite(at(3, 9), 'deleted'),
      ],
    });
    expect(satellites).toEqual({ launched: 5, waiting: 2, tasks: 1, done: 1, deleted: 1, snoozes: 3 });
  });

  it('makes no suggestions without enough data', () => {
    expect(report({}).suggestions).toEqual([]);
    expect(
      report({
        sessions: [session(at(0, 9), 50), session(at(1, 15), 20)],
        gateVisits: [visit(at(0, 14), { why: 'stuck' })],
        satellites: [satellite(at(0, 9), 'back')],
      }).suggestions,
    ).toEqual([]);
  });

  it('suggests something for the reason a site keeps pulling you in', () => {
    const gateVisits = [0, 1, 2, 3, 4].map((day) => visit(at(day, 14, 10), { why: 'stuck' }));
    expect(report({ gateVisits }).suggestions).toEqual([
      'You reach for Reddit when stuck, mostly 2–4pm. Next time, try writing down the smallest next step instead.',
      'Next time a thought pulls you away, launch it as a satellite instead. It comes back after the session.',
    ]);
  });

  it('drafts an if-then plan for the profile whose sessions a site pulls at most', () => {
    const sessions = [0, 1, 2].map((day) => session(at(day, 13), 120));
    sessions.push(session(at(3, 13), 120, { profileId: 'writing', profileName: 'Writing' }));
    const gateVisits = [0, 1, 2, 3, 4].map((day) => visit(at(day, 14, 10), { why: 'stuck' }));
    expect(report({ sessions, gateVisits }).planIdea).toEqual({
      profileId: 'deep',
      recipeId: 'reddit',
      plan: "When I'm stuck and want Reddit, I'll write down the smallest next step.",
    });

    // Not without the tip, or without a session to tell which profile it was.
    expect(report({ sessions, gateVisits: gateVisits.slice(0, 4) }).planIdea).toBeUndefined();
    expect(report({ gateVisits }).planIdea).toBeUndefined();
  });

  it('counts goals and how they went', () => {
    const sessions = [
      session(at(0, 9), 50, { goal: 'Draft the intro', goalMet: true }),
      session(at(1, 9), 50, { goal: 'Outline part two', goalMet: false }),
      session(at(2, 9), 50, { goal: 'Edit chapter one' }),
      session(at(3, 9), 50),
      session(at(-1, 9), 50, { goal: 'Last week', goalMet: true }),
    ];
    expect(report({ sessions }).focus.goals).toEqual({ set: 3, met: 1, missed: 1 });
    expect(report({}).focus.goals).toEqual({ set: 0, met: 0, missed: 0 });
  });

  it('suggests the time of day focus goes best', () => {
    const sessions = [
      session(at(0, 9), 60),
      session(at(1, 9), 50),
      session(at(2, 14), 25),
      session(at(3, 14), 30),
      session(at(4, 20), 20),
    ];
    expect(report({ sessions }).suggestions).toEqual([
      'Your longest focus comes in the morning, about 55 minutes a session. Try scheduling Deep Work then.',
    ]);
    // Close enough that it's not worth saying.
    sessions[2] = session(at(2, 14), 50);
    sessions[3] = session(at(3, 14), 45);
    sessions[4] = session(at(4, 20), 50);
    expect(report({ sessions }).suggestions).toEqual([]);
  });

  it('suggests a timer that fits how long sessions really run', () => {
    const timed = (day: number, minutes: number, overtime: number) =>
      session(at(day, 9), minutes, { timerMinutes: 25, overtimeMs: overtime * MIN });
    const over = [timed(0, 25, 30), timed(1, 25, 20), timed(2, 25, 35), timed(3, 25, 0)];
    expect(report({ sessions: over }).suggestions).toEqual([
      'Your Deep Work sessions usually run past the timer. A 50-minute timer might fit better.',
    ]);

    const short = [timed(0, 10, 0), timed(1, 15, 0), timed(2, 12, 0)];
    expect(report({ sessions: short }).suggestions).toEqual([
      'Your Deep Work sessions usually end before the timer. Try a 10-minute timer instead.',
    ]);
  });

  it('keeps to three suggestions', () => {
    const sessions = [
      ...[0, 1, 2, 3, 4, 5].map((day) => session(at(day, 9), 25, { timerMinutes: 25, overtimeMs: 35 * MIN })),
      session(at(3, 15), 15),
      session(at(4, 15), 15),
    ];
    const gateVisits = [0, 1, 2, 3, 4].map((day) => visit(at(day, 14), { why: 'bored' }));
    expect(report({ sessions, gateVisits }).suggestions).toHaveLength(3);
  });
});
