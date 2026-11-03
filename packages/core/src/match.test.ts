import { describe, expect, it } from 'vitest';
import { matchesPattern, matchPatternToRegex } from './match';

describe('matchesPattern', () => {
  it('matches subdomains and the bare domain for *. hosts', () => {
    expect(matchesPattern('*://*.reddit.com/*', 'https://www.reddit.com/r/writing')).toBe(true);
    expect(matchesPattern('*://*.reddit.com/*', 'https://old.reddit.com/')).toBe(true);
    expect(matchesPattern('*://*.reddit.com/*', 'https://reddit.com/')).toBe(true);
  });

  it('does not match lookalike domains', () => {
    expect(matchesPattern('*://*.reddit.com/*', 'https://notreddit.com/')).toBe(false);
    expect(matchesPattern('*://*.reddit.com/*', 'https://reddit.com.evil.example/')).toBe(false);
  });

  it('limits * schemes to http and https', () => {
    expect(matchesPattern('*://*.reddit.com/*', 'http://reddit.com/')).toBe(true);
    expect(matchesPattern('*://*.reddit.com/*', 'ftp://reddit.com/')).toBe(false);
    expect(matchesPattern('https://*.reddit.com/*', 'http://reddit.com/')).toBe(false);
  });

  it('matches paths as globs', () => {
    expect(matchesPattern('*://*.reddit.com/r/*', 'https://www.reddit.com/r/writing')).toBe(true);
    expect(matchesPattern('*://*.reddit.com/r/*', 'https://www.reddit.com/user/me')).toBe(false);
  });

  it('ignores a trailing dot on the host', () => {
    expect(matchesPattern('*://*.youtube.com/*', 'https://youtube.com./')).toBe(true);
    expect(matchesPattern('*://*.youtube.com/*', 'https://www.youtube.com./watch')).toBe(true);
    expect(matchesPattern('https://x.com/*', 'https://x.com./home')).toBe(true);
    expect(matchesPattern('*://*.youtube.com/*', 'https://notyoutube.com./')).toBe(false);
  });

  it('rejects malformed patterns and URLs', () => {
    expect(matchesPattern('reddit.com', 'https://reddit.com/')).toBe(false);
    expect(matchesPattern('*://*.reddit.com/*', 'not a url')).toBe(false);
  });
});

describe('matchPatternToRegex', () => {
  const regex = (pattern: string) => new RegExp(matchPatternToRegex(pattern)!);

  it('agrees with matchesPattern', () => {
    const patterns = ['*://*.reddit.com/*', 'https://x.com/*', '*://*.reddit.com/r/*', '*://*/*'];
    const urls = [
      'https://www.reddit.com/r/writing?sort=new',
      'http://reddit.com/',
      'https://old.reddit.com/user/me',
      'https://notreddit.com/',
      'https://reddit.com.evil.example/',
      'https://x.com/home',
      'http://x.com/home',
      'https://www.x.com/home',
      'https://reddit.com./r/all',
      'https://www.reddit.com.:8080/',
      'https://x.com./home',
      'https://reddit.com..evil.example/',
    ];
    for (const pattern of patterns) {
      for (const url of urls) {
        expect(regex(pattern).test(url), `${pattern} vs ${url}`).toBe(matchesPattern(pattern, url));
      }
    }
  });

  it('allows a port, like browsers do', () => {
    expect(regex('*://*.reddit.com/*').test('http://www.reddit.com:8080/')).toBe(true);
  });

  it('escapes regex characters in the pattern', () => {
    expect(regex('*://a+b.example/*').test('https://a+b.example/')).toBe(true);
    expect(regex('*://a+b.example/*').test('https://aab.example/')).toBe(false);
  });

  it('returns null for malformed patterns', () => {
    expect(matchPatternToRegex('reddit.com')).toBeNull();
  });
});
