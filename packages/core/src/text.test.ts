import { expect, it } from 'vitest';
import { cutChars } from './text';

it('cuts by characters, so an emoji is kept whole or left out', () => {
  expect(cutChars('Deep work', 4)).toBe('Deep');
  expect(cutChars('x'.repeat(39) + '🌙', 40)).toBe('x'.repeat(39) + '🌙');
  expect(cutChars('x'.repeat(39) + '🌙🌙', 40)).toBe('x'.repeat(39) + '🌙');
  expect(cutChars('🌙'.repeat(3), 2)).toBe('🌙🌙');
  expect(cutChars('short', 40)).toBe('short');
});
