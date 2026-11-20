/**
 * Pure helpers for the cloud LLM providers (Anthropic, OpenAI, OpenRouter):
 * model metadata formatting and the optional fallback chain.
 */

export type CloudProvider = 'claude' | 'openai' | 'openrouter';

export type ReasoningEffort = 'auto' | 'low' | 'medium' | 'high' | 'xhigh' | 'max';

export const REASONING_EFFORTS: { value: ReasoningEffort; label: string }[] = [
  { value: 'auto', label: 'Automático (padrão do modelo)' },
  { value: 'low', label: 'Baixo (mais rápido e barato)' },
  { value: 'medium', label: 'Médio' },
  { value: 'high', label: 'Alto' },
  { value: 'xhigh', label: 'Muito alto' },
  { value: 'max', label: 'Máximo (mais lento e caro)' },
];

export interface ModelMeta {
  label?: string;
  contextLength?: number | null;
  /** USD per token (OpenRouter format). */
  promptPrice?: string | null;
  completionPrice?: string | null;
}

/** "1M", "200k"... Returns '' when unknown. */
export function formatContext(tokens?: number | null): string {
  if (!tokens || tokens <= 0) return '';
  if (tokens >= 1_000_000) return `${+(tokens / 1_000_000).toFixed(1)}M`;
  return `${Math.round(tokens / 1000)}k`;
}

/** Per-token USD string to "US$ 3,00/M". Returns '' when unknown; "grátis" for 0. */
export function formatPricePerMillion(perToken?: string | null): string {
  if (perToken === undefined || perToken === null || perToken === '') return '';
  const value = Number(perToken);
  if (!Number.isFinite(value) || value < 0) return '';
  if (value === 0) return 'grátis';
  return `US$ ${(value * 1_000_000).toFixed(2).replace('.', ',')}/M`;
}

/** One-line description shown under a model in the picker. */
export function describeModel(meta?: ModelMeta): string {
  if (!meta) return '';
  const parts = [
    formatContext(meta.contextLength) && `${formatContext(meta.contextLength)} de contexto`,
    formatPricePerMillion(meta.promptPrice) && `entrada ${formatPricePerMillion(meta.promptPrice)}`,
    formatPricePerMillion(meta.completionPrice) && `saída ${formatPricePerMillion(meta.completionPrice)}`,
  ].filter(Boolean);
  return parts.join(' · ');
}

export interface FallbackEntry {
  provider: string;
  model: string;
}

export interface FallbackChain {
  enabled: boolean;
  entries: FallbackEntry[];
}

export const EMPTY_FALLBACK_CHAIN: FallbackChain = { enabled: false, entries: [] };

const KEYLESS = new Set(['ollama', 'builtin-ai']);
const KNOWN = new Set(['claude', 'openai', 'openrouter', 'groq', 'ollama', 'builtin-ai']);

/** Validates data read from storage; anything odd yields the disabled default. */
export function sanitizeFallbackChain(raw: unknown): FallbackChain {
  if (!raw || typeof raw !== 'object') return EMPTY_FALLBACK_CHAIN;
  const obj = raw as { enabled?: unknown; entries?: unknown };
  const entries = Array.isArray(obj.entries)
    ? obj.entries
        .filter(
          (e): e is FallbackEntry =>
            !!e &&
            typeof (e as FallbackEntry).provider === 'string' &&
            typeof (e as FallbackEntry).model === 'string' &&
            KNOWN.has((e as FallbackEntry).provider) &&
            (e as FallbackEntry).model.trim() !== '',
        )
        .map((e) => ({ provider: e.provider, model: e.model.trim() }))
    : [];
  return { enabled: obj.enabled === true && entries.length > 0, entries };
}

/**
 * Payload for the `set_llm_fallback_chain` command. Empty when the chain is
 * disabled, and entries whose provider needs a key but has none are dropped,
 * so nothing is attempted (or billed) without explicit configuration.
 */
export function buildFallbackPayload(
  chain: FallbackChain,
  apiKeys: Record<string, string | null | undefined>,
): { provider: string; model: string; apiKey: string }[] {
  if (!chain.enabled) return [];
  return chain.entries.flatMap((e) => {
    const key = (apiKeys[e.provider] ?? '').trim();
    if (!KEYLESS.has(e.provider) && !key) return [];
    return [{ provider: e.provider, model: e.model, apiKey: KEYLESS.has(e.provider) ? '' : key }];
  });
}

export const FALLBACK_CHAIN_STORAGE_KEY = 'llmFallbackChain';
export const REASONING_EFFORT_STORAGE_KEY = 'llmReasoningEffort';

export function loadFallbackChain(): FallbackChain {
  try {
    const raw = localStorage.getItem(FALLBACK_CHAIN_STORAGE_KEY);
    return raw ? sanitizeFallbackChain(JSON.parse(raw)) : EMPTY_FALLBACK_CHAIN;
  } catch {
    return EMPTY_FALLBACK_CHAIN;
  }
}

export function loadReasoningEffort(): ReasoningEffort {
  try {
    const v = localStorage.getItem(REASONING_EFFORT_STORAGE_KEY);
    return REASONING_EFFORTS.some((o) => o.value === v) ? (v as ReasoningEffort) : 'auto';
  } catch {
    return 'auto';
  }
}
