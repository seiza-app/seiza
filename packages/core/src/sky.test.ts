import { describe, expect, it } from 'vitest';
import type { ActiveSession } from './generated/ActiveSession';
import type { SessionRecord } from './generated/SessionRecord';
import { withNewProfile, withStarColor } from './edits';
import {
  lifetimeFocusMs,
  milkyWay,
  newStarColor,
  sessionSoFar,
  skyClarity,
  starColorFor,
  starColors,
  stardust,
  starMagnitude,
  stars,
  starSparkle,
} from './sky';
import type { Profile } from './types';

const MIN = 60_000;
const start = Date.UTC(2026, 9, 26, 9);

let ids = 0;
/** A session starting `offset` minutes after 9am, lasting `minutes`. */
function session(offset: number, minutes: number, extra: Partial<SessionRecord> = {}): SessionRecord {
  const startedAt = start + offset * MIN;
  return {
    id: `s${++ids}`,
    profileId: 'deep-work',
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

describe('stars', () => {
  it('gives every session its own star, oldest first', () => {
    const later = session(120, 45);
    const earlier = session(0, 50);
    const sky = stars([later, earlier]);
    expect(sky.map((s) => s.id)).toEqual([earlier.id, later.id]);
    expect(sky[0]).toMatchObject({ profileId: 'deep-work', sessionIds: [earlier.id], focusedMs: 50 * MIN });
  });

  it('joins short back-to-back sessions into one star that grows', () => {
    const first = session(0, 20);
    const second = session(25, 15, { overtimeMs: 2 * MIN });
    const sky = stars([first, second]);
    expect(sky).toHaveLength(1);
    expect(sky[0]).toMatchObject({
      id: first.id,
      sessionIds: [first.id, second.id],
      endedAt: second.endedAt,
      focusedMs: 35 * MIN,
      overtimeMs: 2 * MIN,
    });
  });

  it('keeps sessions apart that are long, late or for another profile', () => {
    const first = session(0, 20);
    expect(stars([first, session(25, 30)])).toHaveLength(2);
    expect(stars([first, session(31, 20)])).toHaveLength(2);
    expect(stars([first, session(25, 20, { profileId: 'writing' })])).toHaveLength(2);
  });

  it("doesn't draw a lone session under 5 minutes, but a run of them makes a star", () => {
    expect(stars([session(0, 4)])).toEqual([]);
    expect(stars([session(0, 3), session(5, 3)])).toHaveLength(1);
  });

  it('leaves out broken sessions', () => {
    expect(stars([session(0, 50, { broken: true })])).toEqual([]);
  });

  it('grows with minutes, fast at first, and sparkles with overtime', () => {
    const [short, long, full] = stars([session(0, 5), session(100, 90), session(300, 200)]);
    expect(starMagnitude(short!)).toBeCloseTo(0.2);
    expect(starMagnitude(long!)).toBeCloseTo(0.87, 2);
    expect(starMagnitude(full!)).toBe(1);
    expect(starSparkle(long!)).toBe(0);
    const [overtime] = stars([session(0, 60, { focusedMs: 45 * MIN, overtimeMs: 15 * MIN })]);
    expect(starSparkle(overtime!)).toBeCloseTo(0.5);
  });
});

describe('stardust', () => {
  it('counts focused minutes, overtime at 1.5×, and nothing for broken sessions', () => {
    const sessions = [
      session(0, 50, { idleTailMs: 20 * MIN }),
      session(100, 30, { focusedMs: 20 * MIN, overtimeMs: 10 * MIN }),
      session(200, 60, { broken: true }),
      session(300, 2),
    ];
    expect(stardust(sessions)).toBe(50 + 20 + 15 + 2);
    expect(stardust([])).toBe(0);
  });
});

describe('lifetimeFocusMs', () => {
  it('adds up focus and overtime, leaving out broken sessions', () => {
    expect(lifetimeFocusMs([])).toBe(0);
    expect(
      lifetimeFocusMs([
        session(0, 50, { focusedMs: 25 * MIN, overtimeMs: 25 * MIN }),
        session(60, 30),
        session(120, 90, { broken: true }),
      ]),
    ).toBe(80 * MIN);
  });
});

describe('milkyWay', () => {
  it('fills in with all the time focused, quickest at first, and nothing for broken sessions', () => {
    expect(milkyWay([])).toBe(0);
    const hours = (n: number, extra: Partial<SessionRecord> = {}) =>
      session(0, n * 60, { focusedMs: n * 60 * MIN, ...extra });
    expect(milkyWay([hours(15)])).toBeCloseTo(0.1);
    expect(milkyWay([hours(15), hours(400, { broken: true })])).toBeCloseTo(0.1);
    expect(milkyWay([hours(400, { focusedMs: 300 * 60 * MIN, overtimeMs: 100 * 60 * MIN })])).toBeCloseTo(
      Math.sqrt(400 / 1500),
    );
    expect(milkyWay([hours(1000), hours(500)])).toBe(1);
  });
});

describe('sessionSoFar', () => {
  const active: ActiveSession = {
    id: 'now',
    profileId: 'writing',
    profileName: 'Writing',
    startedAt: start,
    timer: { minutes: 45, onEnd: 'silent', strictness: 'gentle', showSinceDone: false },
    goal: null,
  };

  it('counts time before the timer is done as focus and after it as overtime', () => {
    expect(sessionSoFar(active, start + 30 * MIN)).toMatchObject({ focusedMs: 30 * MIN, overtimeMs: 0 });
    expect(sessionSoFar(active, start + 50 * MIN)).toMatchObject({
      endedAt: start + 50 * MIN,
      focusedMs: 45 * MIN,
      overtimeMs: 5 * MIN,
    });
  });

  it('counts it all as focus without a timer', () => {
    const untimed = { ...active, timer: null };
    expect(sessionSoFar(untimed, start + 90 * MIN)).toMatchObject({ focusedMs: 90 * MIN, overtimeMs: 0 });
  });
});

describe('skyClarity', () => {
  const last = session(0, 50);

  it('is clear during a session and hazy before the first', () => {
    expect(skyClarity(start, true, last)).toBe(1);
    expect(skyClarity(start, false)).toBe(0);
  });

  it('stays clear for a fifth of the last session, then the haze creeps back', () => {
    expect(skyClarity(last.endedAt + 10 * MIN, false, last)).toBe(1);
    expect(skyClarity(last.endedAt + 12.5 * MIN, false, last)).toBeCloseTo(0.5);
    expect(skyClarity(last.endedAt + 15 * MIN, false, last)).toBe(0);
    expect(skyClarity(last.endedAt + 600 * MIN, false, last)).toBe(0);
  });

  it('has no afterglow after a session ended before its timer was done', () => {
    const early = session(0, 30, { timerMinutes: 50 });
    expect(skyClarity(early.endedAt, false, early)).toBe(1);
    expect(skyClarity(early.endedAt + 2.5 * MIN, false, early)).toBeCloseTo(0.5);
    expect(skyClarity(early.endedAt + 5 * MIN, false, early)).toBe(0);
    // Finishing the timer, or going past it, keeps the afterglow.
    const done = session(0, 50, { timerMinutes: 50 });
    expect(skyClarity(done.endedAt + 10 * MIN, false, done)).toBe(1);
    const over = session(0, 50, { timerMinutes: 45, focusedMs: 45 * MIN, overtimeMs: 5 * MIN });
    expect(skyClarity(over.endedAt + 10 * MIN, false, over)).toBe(1);
  });
});

describe('star colors', () => {
  it("uses a profile's own color, then its starter default, then one picked by its id", () => {
    expect(starColorFor('deep-work', '#ABCDEF')).toBe('#abcdef');
    expect(starColorFor('writing')).toBe('#f5c86b');
    expect(starColorFor('deep-work', 'red')).toBe('#cfe0ff');
    const picked = starColorFor('my-own-profile');
    expect(starColors).toContain(picked);
    expect(starColorFor('my-own-profile', null)).toBe(picked);
  });

  it('gives a new profile a color no other profile has', () => {
    const starters: Profile[] = ['writing', 'deep-work', 'wind-down'].map((id) => ({ id, name: id, sites: [] }));
    expect(newStarColor(starters)).toBe('#9fe3c9');
    const next = withNewProfile(starters, 'new', 'Reading');
    expect(next.at(-1)!.starColor).toBe('#9fe3c9');
    expect(newStarColor(next)).toBe('#f2a7c3');
  });

  it('sets a color, ignoring anything that is not a hex color', () => {
    const profiles: Profile[] = [{ id: 'a', name: 'A', sites: [] }];
    expect(withStarColor(profiles, 'a', '#F2A7C3')[0]!.starColor).toBe('#f2a7c3');
    expect(withStarColor(profiles, 'a', 'url(evil)')[0]!.starColor).toBeUndefined();
  });
});
