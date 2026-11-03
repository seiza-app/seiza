import { describe, expect, it } from 'vitest';
import { pickQuote, quoteForDay, withOwnQuoteRemoved } from './quotes';

it('removes a quote by id', () => {
  const list = [
    { id: 'a', text: 'A' },
    { id: 'b', text: 'B' },
  ];
  expect(withOwnQuoteRemoved(list, 'a')).toEqual([{ id: 'b', text: 'B' }]);
});

it('picks the same quote all day', () => {
  const bundled = [{ text: 'A' }, { text: 'B' }, { text: 'C' }];
  const own = [{ id: 'm', text: 'Mine' }];
  expect(quoteForDay(bundled, own, '2026-10-28')).toBe(quoteForDay(bundled, own, '2026-10-28'));
  const days = Array.from({ length: 30 }, (_, i) => quoteForDay(bundled, own, `2026-11-${i + 1}`)?.text);
  expect(new Set(days).size).toBeGreaterThan(1);
  expect(quoteForDay([], [], '2026-10-28')).toBeUndefined();
});

describe('pickQuote', () => {
  const bundled = [{ text: 'A' }, { text: 'B' }];
  const own = [{ id: 'o', text: 'Mine' }];
  const sequence =
    (...values: number[]) =>
    () =>
      values.shift() ?? 0;

  it("weights the user's own quotes three times", () => {
    // Two bundled quotes plus three copies of the user's one.
    expect(pickQuote(bundled, own, sequence(0))?.text).toBe('A');
    expect(pickQuote(bundled, own, sequence(0.39))?.text).toBe('B');
    expect(pickQuote(bundled, own, sequence(0.4))?.text).toBe('Mine');
    expect(pickQuote(bundled, own, sequence(0.99))?.text).toBe('Mine');
  });

  it('returns nothing when there are no quotes', () => {
    expect(pickQuote([], [])).toBeUndefined();
  });
});
