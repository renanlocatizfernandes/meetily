import { describe, expect, test } from 'bun:test';
import { evidenceText, minutesLabel, parseSilenceMinutes, secondsLeft, stoppedText } from '../../src/lib/meetingEnd';

describe('meeting end helpers', () => {
  test('silence minutes are rounded and kept inside the range', () => {
    expect(parseSilenceMinutes('3')).toBe(3);
    expect(parseSilenceMinutes(' 4,6 ')).toBe(5);
    expect(parseSilenceMinutes('0')).toBe(1);
    expect(parseSilenceMinutes('500')).toBe(60);
    expect(parseSilenceMinutes('')).toBeNull();
    expect(parseSilenceMinutes('abc')).toBeNull();
  });

  test('the countdown never goes below zero', () => {
    expect(secondsLeft(60_000, 0)).toBe(60);
    expect(secondsLeft(60_000, 59_100)).toBe(1);
    expect(secondsLeft(60_000, 60_000)).toBe(0);
    expect(secondsLeft(60_000, 90_000)).toBe(0);
  });

  test('texts are in Portuguese, without a dash, and name the evidence', () => {
    expect(minutesLabel(1)).toBe('1 minuto');
    expect(minutesLabel(3)).toBe('3 minutos');
    expect(evidenceText({ reason: 'signal', silenceMinutes: 3 })).toBe('A chamada terminou.');
    expect(evidenceText({ reason: 'silence', silenceMinutes: 3 })).toBe('Ninguém fala há 3 minutos.');
    expect(stoppedText('user')).toBeNull();
    for (const reason of ['signal', 'silence', 'timeout'] as const) {
      const text = stoppedText(reason);
      expect(text).toBeTruthy();
      expect(text).not.toMatch(/[—–]/);
    }
  });
});
