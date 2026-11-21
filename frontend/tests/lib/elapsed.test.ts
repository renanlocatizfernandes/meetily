import { describe, expect, test } from 'bun:test';
import { formatElapsed } from '../../src/lib/elapsed';

describe('formatElapsed', () => {
  test('minutes and seconds under an hour', () => {
    expect(formatElapsed(0)).toBe('00:00');
    expect(formatElapsed(65)).toBe('01:05');
    expect(formatElapsed(3599)).toBe('59:59');
  });
  test('hours from one hour on', () => {
    expect(formatElapsed(3600)).toBe('1:00:00');
    expect(formatElapsed(4503)).toBe('1:15:03');
  });
  test('missing or negative values show zero', () => {
    expect(formatElapsed(null)).toBe('00:00');
    expect(formatElapsed(undefined)).toBe('00:00');
    expect(formatElapsed(-5)).toBe('00:00');
    expect(formatElapsed(Number.NaN)).toBe('00:00');
  });
});
