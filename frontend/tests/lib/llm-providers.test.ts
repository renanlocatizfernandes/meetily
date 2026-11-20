import { describe, expect, test } from 'bun:test';
import {
  buildFallbackPayload,
  describeModel,
  formatContext,
  formatPricePerMillion,
  sanitizeFallbackChain,
} from '../../src/lib/llmProviders';

describe('model metadata formatting', () => {
  test('formats context and price', () => {
    expect(formatContext(1_000_000)).toBe('1M');
    expect(formatContext(200_000)).toBe('200k');
    expect(formatContext(null)).toBe('');
    expect(formatPricePerMillion('0.000003')).toBe('US$ 3,00/M');
    expect(formatPricePerMillion('0')).toBe('grátis');
    expect(formatPricePerMillion('-1')).toBe('');
    expect(formatPricePerMillion(undefined)).toBe('');
  });

  test('describes only what the provider informed', () => {
    expect(describeModel(undefined)).toBe('');
    expect(describeModel({ contextLength: 128_000 })).toBe('128k de contexto');
    expect(
      describeModel({ contextLength: 1_000_000, promptPrice: '0.000002', completionPrice: '0.00001' }),
    ).toBe('1M de contexto · entrada US$ 2,00/M · saída US$ 10,00/M');
  });
});

describe('fallback chain', () => {
  test('is disabled by default and tolerates garbage', () => {
    expect(sanitizeFallbackChain(null).enabled).toBe(false);
    expect(sanitizeFallbackChain('x').entries).toEqual([]);
    expect(sanitizeFallbackChain({ enabled: true, entries: [] }).enabled).toBe(false);
    expect(
      sanitizeFallbackChain({ enabled: true, entries: [{ provider: 'evil', model: 'm' }, { provider: 'claude' }] })
        .entries,
    ).toEqual([]);
  });

  test('payload is empty when disabled and skips providers without keys', () => {
    const chain = sanitizeFallbackChain({
      enabled: true,
      entries: [
        { provider: 'claude', model: 'claude-opus-5-5' },
        { provider: 'openrouter', model: 'anthropic/x' },
        { provider: 'ollama', model: 'llama3.2:latest' },
      ],
    });
    expect(buildFallbackPayload({ ...chain, enabled: false }, { claude: 'k' })).toEqual([]);
    expect(buildFallbackPayload(chain, { claude: ' sk-1 ', openrouter: null })).toEqual([
      { provider: 'claude', model: 'claude-opus-5-5', apiKey: 'sk-1' },
      { provider: 'ollama', model: 'llama3.2:latest', apiKey: '' },
    ]);
  });
});
