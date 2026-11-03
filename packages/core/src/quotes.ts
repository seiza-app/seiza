import { seeded } from './sky';
import type { OwnQuote, Quote } from './types';

// Quotes people add themselves, in the extension's settings or the app's. They sync between the
// two like the profiles do, so the limits match the app's (apps/desktop/src-tauri/src/quotes.rs).

export const maxOwnQuotes = 200;
export const maxQuoteLength = 280;
export const maxQuoteAuthorLength = 80;

/** How much more likely each of the user's own quotes is than a bundled one. */
const ownQuoteWeight = 3;

export function withOwnQuoteRemoved(list: readonly OwnQuote[], id: string): OwnQuote[] {
  return list.filter((quote) => quote.id !== id);
}

/** The quote for a day, like "2026-10-28": the same one all that day, favoring the user's own. */
export function quoteForDay(bundled: readonly Quote[], own: readonly OwnQuote[], day: string): Quote | undefined {
  return pickQuote(bundled, own, seeded(`quote:${day}`));
}

/** A random quote, favoring the user's own. */
export function pickQuote(
  bundled: readonly Quote[],
  own: readonly OwnQuote[],
  random: () => number = Math.random,
): Quote | undefined {
  const weighted = [...bundled, ...own.flatMap((quote) => Array<Quote>(ownQuoteWeight).fill(quote))];
  return weighted[Math.floor(random() * weighted.length)];
}
