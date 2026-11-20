/**
 * "Pergunte às suas reuniões": tipos e funções puras do chat, mais a camada fina sobre os comandos Tauri
 * `ask_meetings`, `ask_status` e `ask_rebuild_index`.
 */
import { invoke } from '@tauri-apps/api/core';

export interface AskCitation {
  /** Número usado no texto da resposta, como `[n]`. */
  n: number;
  meetingId: string;
  meetingTitle: string;
  client: string | null;
  /** RFC 3339. */
  meetingDate: string;
  source: 'transcript' | 'summary';
  startSeconds: number | null;
  excerpt: string;
}

export type AnswerStatus = 'generated' | 'skipped' | 'unavailable' | 'no_results';

export interface AskResponse {
  citations: AskCitation[];
  answer: string | null;
  answerStatus: AnswerStatus;
  answerNote: string | null;
  leavesMachine: boolean;
  provider: string;
  estimatedTokens: number;
  searchMode: 'fts' | 'basic';
  periodLabel: string | null;
  clientFilter: string | null;
  indexedMeetings: number;
}

export interface AskStatus {
  llmConfigured: boolean;
  provider: string;
  model: string;
  leavesMachine: boolean;
  indexedMeetings: number;
  totalMeetings: number;
  searchMode: 'fts' | 'basic';
}

export interface AskTurn {
  question: string;
  answer: string;
}

export type AnswerPart = { type: 'text'; text: string } | { type: 'cite'; n: number };

/** Quebra a resposta em texto e marcadores `[n]` que apontam para uma fonte existente. */
export function splitAnswer(answer: string, citationCount: number): AnswerPart[] {
  const parts: AnswerPart[] = [];
  const pattern = /\[(\d{1,2})\]/g;
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(answer)) !== null) {
    const n = Number(match[1]);
    if (n < 1 || n > citationCount) continue;
    if (match.index > last) parts.push({ type: 'text', text: answer.slice(last, match.index) });
    parts.push({ type: 'cite', n });
    last = match.index + match[0].length;
  }
  if (last < answer.length) parts.push({ type: 'text', text: answer.slice(last) });
  return parts;
}

/** 125 → "02:05"; 3725 → "1:02:05". */
export function formatClock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const rest = String(total % 60).padStart(2, '0');
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, '0')}:${rest}` : `${String(minutes).padStart(2, '0')}:${rest}`;
}

/** Onde o trecho está: "Resumo" ou "Transcrição · 02:05". */
export function citationLocation(citation: Pick<AskCitation, 'source' | 'startSeconds'>): string {
  if (citation.source === 'summary') return 'Resumo';
  return citation.startSeconds === null ? 'Transcrição' : `Transcrição · ${formatClock(citation.startSeconds)}`;
}

/** Mesma regra da barra lateral: ids com hífen são reuniões gravadas. */
export function meetingHref(meetingId: string): string {
  return `/meeting-details?id=${encodeURIComponent(meetingId)}`;
}

export function formatTokens(tokens: number): string {
  if (tokens < 1000) return `${tokens}`;
  return `${(tokens / 1000).toFixed(1).replace('.', ',')} mil`;
}

const PROVIDER_LABELS: Record<string, string> = {
  ollama: 'Ollama (local)',
  openai: 'OpenAI',
  claude: 'Claude',
  groq: 'Groq',
  openrouter: 'OpenRouter',
  'custom-openai': 'servidor OpenAI personalizado',
};

export function providerLabel(provider: string): string {
  return PROVIDER_LABELS[provider] ?? provider;
}

/**
 * Aviso mostrado antes de pedir a resposta redigida. `null` quando tudo fica nesta máquina.
 * Só os trechos achados saem do computador, nunca o áudio nem as reuniões inteiras.
 */
export function consumptionNotice(status: Pick<AskStatus, 'leavesMachine' | 'provider'> | null): string | null {
  if (!status?.leavesMachine) return null;
  return `Para redigir a resposta, os trechos encontrados (não o áudio nem as reuniões inteiras) são enviados a ${providerLabel(status.provider)}. Provedores pagos cobram por uso. Desligue “Redigir resposta” para só buscar, sem enviar nada.`;
}

/** Resposta do modelo vira contexto da próxima pergunta, sem os marcadores de fonte. */
export function toHistory(turns: Array<{ question: string; response: AskResponse | null }>, max = 2): AskTurn[] {
  return turns
    .filter((turn): turn is { question: string; response: AskResponse } => !!turn.response?.answer)
    .slice(-max)
    .map((turn) => ({
      question: turn.question,
      answer: (turn.response.answer ?? '').replace(/\[\d{1,2}\]/g, '').replace(/\s{2,}/g, ' ').trim(),
    }));
}

export function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return 'Não foi possível responder agora.';
}

export async function askMeetings(
  question: string,
  useLlm: boolean,
  history: AskTurn[],
  meetingId?: string | null,
): Promise<AskResponse> {
  return invoke<AskResponse>('ask_meetings', { question, useLlm, history, meetingId: meetingId ?? null });
}

export async function loadAskStatus(): Promise<AskStatus | null> {
  try {
    return await invoke<AskStatus>('ask_status');
  } catch {
    return null;
  }
}

export async function rebuildAskIndex(): Promise<number> {
  return invoke<number>('ask_rebuild_index');
}

export const EXAMPLE_QUESTIONS = [
  'O que decidimos com o cliente na última reunião?',
  'Quais tarefas estão comigo?',
  'O que ficou combinado sobre preço e prazo?',
  'Quais pendências ficaram abertas esta semana?',
];
