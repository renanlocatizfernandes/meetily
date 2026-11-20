import { describe, expect, test } from 'bun:test';
import {
  DETECTION_TEXT_LIMIT,
  buildDetectionText,
  shouldApplySuggestion,
  type TemplateSuggestion,
} from '../../src/lib/template-detection';
import { visibleSuggestions } from '../../src/hooks/meeting-details/useSpeakerSuggestions';

const suggestion = (over: Partial<TemplateSuggestion> = {}): TemplateSuggestion => ({
  template_id: 'one_on_one',
  template_name: 'Conversa 1:1',
  confidence: 0.7,
  detected: true,
  ...over,
});

describe('buildDetectionText', () => {
  test('joins non-empty segments and skips blanks', () => {
    expect(buildDetectionText([{ text: ' Olá ' }, { text: '   ' }, { text: 'tudo bem' }])).toBe('Olá\ntudo bem');
  });

  test('stops at the limit', () => {
    const long = 'a'.repeat(DETECTION_TEXT_LIMIT);
    expect(buildDetectionText([{ text: long }, { text: 'b' }]).length).toBe(DETECTION_TEXT_LIMIT);
  });
});

describe('shouldApplySuggestion', () => {
  const ids = ['standard_meeting', 'one_on_one'];

  test('applies a detected, available, different template', () => {
    expect(shouldApplySuggestion(suggestion(), ids, 'standard_meeting')).toBe(true);
  });

  test('ignores the fallback, the current template and unknown ids', () => {
    expect(shouldApplySuggestion(suggestion({ detected: false }), ids, 'standard_meeting')).toBe(false);
    expect(shouldApplySuggestion(suggestion(), ids, 'one_on_one')).toBe(false);
    expect(shouldApplySuggestion(suggestion({ template_id: 'gone' }), ids, 'standard_meeting')).toBe(false);
  });
});

describe('visibleSuggestions', () => {
  const item = { speaker_key: 'a', label: 'Pessoa 1', name: 'Ana', confidence: 0.9, reason: '' };

  test('hides only the dismissed pair', () => {
    expect(visibleSuggestions([item], new Set(['a:Ana']))).toEqual([]);
    expect(visibleSuggestions([item], new Set(['a:Bia']))).toEqual([item]);
  });
});
