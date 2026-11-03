import { describe, expect, it } from 'vitest';
import type { Star } from './sky';
import { starDays, streaks } from './streaks';

let ids = 0;
/** A star on a day of October 2026 (or later, past 31), at `hour` local time. */
function star(day: number, hour = 9): Star {
  const id = `star${++ids}`;
  const startedAt = new Date(2026, 9, day, hour).getTime();
  return {
    id,
    profileId: 'deep-work',
    sessionIds: [id],
    startedAt,
    endedAt: startedAt + 60 * 60_000,
    focusedMs: 60 * 60_000,
    overtimeMs: 0,
  };
}

/** A time on a day of October 2026, local time. */
const on = (day: number, hour = 12) => new Date(2026, 9, day, hour).getTime();

describe('star days', () => {
  it('counts each day once, by its first star, and how far into a streak it is', () => {
    const sky = [star(1), star(1, 15), star(2), star(3), star(5), star(6)];
    expect(starDays([...sky].reverse()).map((day) => [day.id, day.streakDays, day.streakId])).toEqual([
      [sky[0]!.id, 1, sky[0]!.id],
      [sky[2]!.id, 2, sky[0]!.id],
      [sky[3]!.id, 3, sky[0]!.id],
      [sky[4]!.id, 1, sky[4]!.id],
      [sky[5]!.id, 2, sky[4]!.id],
    ]);
  });

  it('keeps a streak going across the end of a month and a daylight saving change', () => {
    // 22 October to 3 November: clocks change on 25 October in Europe and 1 November in the US.
    const days = Array.from({ length: 13 }, (_, i) => i + 1);
    const sky = days.map((n) => star(21 + n, 23));
    expect(starDays(sky).map((day) => day.streakDays)).toEqual(days);
  });
});

describe('streaks', () => {
  const sky = [star(1), star(2), star(3), star(4), star(10), star(11)];

  it('is running through the day after its last day, since that day isn’t over', () => {
    expect(streaks(sky, on(11))).toEqual({ current: 2, longest: 4 });
    expect(streaks(sky, on(12, 23))).toEqual({ current: 2, longest: 4 });
    expect(streaks(sky, on(13, 0))).toEqual({ current: 0, longest: 4 });
  });

  it('only counts stars by then', () => {
    expect(streaks(sky, on(3, 8))).toEqual({ current: 2, longest: 2 });
    expect(streaks(sky, on(3, 10))).toEqual({ current: 3, longest: 3 });
    expect(streaks(sky, on(9))).toEqual({ current: 0, longest: 4 });
    expect(streaks([], on(9))).toEqual({ current: 0, longest: 0 });
  });
});
