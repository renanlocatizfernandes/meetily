import { describe, expect, test } from 'bun:test';
import {
  citationLocation,
  consumptionNotice,
  formatClock,
  formatTokens,
  meetingHref,
  splitAnswer,
  toHistory,
  type AskResponse,
} from '../../src/lib/ask';

describe('splitAnswer', () => {
  test('turns [n] markers into citation parts', () => {
    expect(splitAnswer('Fechou o contrato [1] e o prazo [2][3].', 3)).toEqual([
      { type: 'text', text: 'Fechou o contrato ' },
      { type: 'cite', n: 1 },
      { type: 'text', text: ' e o prazo ' },
      { type: 'cite', n: 2 },
      { type: 'cite', n: 3 },
      { type: 'text', text: '.' },
    ]);
  });

  test('keeps markers that point nowhere as plain text', () => {
    const parts = splitAnswer('Algo [7] aqui', 2);
    expect(parts).toEqual([{ type: 'text', text: 'Algo [7] aqui' }]);
  });

  test('plain text and empty answers', () => {
    expect(splitAnswer('sem fonte', 0)).toEqual([{ type: 'text', text: 'sem fonte' }]);
    expect(splitAnswer('', 3)).toEqual([]);
  });
});

describe('formatting', () => {
  test('clock', () => {
    expect(formatClock(125)).toBe('02:05');
    expect(formatClock(3725)).toBe('1:02:05');
    expect(formatClock(-4)).toBe('00:00');
  });

  test('citation location', () => {
    expect(citationLocation({ source: 'summary', startSeconds: null })).toBe('Resumo');
    expect(citationLocation({ source: 'transcript', startSeconds: 65 })).toBe('Transcrição · 01:05');
    expect(citationLocation({ source: 'transcript', startSeconds: null })).toBe('Transcrição');
  });

  test('tokens and href', () => {
    expect(formatTokens(420)).toBe('420');
    expect(formatTokens(1250)).toBe('1,3 mil');
    expect(meetingHref('a-b')).toBe('/meeting-details?id=a-b');
  });
});

describe('consumptionNotice', () => {
  test('silent when everything stays local', () => {
    expect(consumptionNotice({ leavesMachine: false, provider: 'ollama' })).toBeNull();
    expect(consumptionNotice(null)).toBeNull();
  });

  test('names the paid provider', () => {
    const notice = consumptionNotice({ leavesMachine: true, provider: 'openai' });
    expect(notice).toContain('OpenAI');
    expect(notice).toContain('cobram por uso');
  });
});

describe('toHistory', () => {
  const response = (answer: string | null): AskResponse => ({
    citations: [],
    answer,
    answerStatus: answer ? 'generated' : 'skipped',
    answerNote: null,
    leavesMachine: false,
    provider: 'ollama',
    estimatedTokens: 0,
    searchMode: 'fts',
    periodLabel: null,
    clientFilter: null,
    indexedMeetings: 0,
  });

  test('keeps the last answered turns without source markers', () => {
    const history = toHistory([
      { question: 'p1', response: response('r1 [1]') },
      { question: 'p2', response: response(null) },
      { question: 'p3', response: response('r3  [2][3] fim') },
      { question: 'p4', response: response('r4') },
    ]);
    expect(history).toEqual([
      { question: 'p3', answer: 'r3 fim' },
      { question: 'p4', answer: 'r4' },
    ]);
  });
});
