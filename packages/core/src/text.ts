/**
 * The first `length` characters of `text`, counted in code points as Rust counts chars, so both
 * sides cut at the same place and an emoji is never cut in half (which the app would refuse).
 */
export function cutChars(text: string, length: number): string {
  return Array.from(text).slice(0, length).join('');
}
