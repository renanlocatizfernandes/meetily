// Types and small text helpers for the context the live assist is anchored on.
// The Rust side (`live_assist/context.rs`) owns the selection; this file only shows it.

export type OriginKind = 'reuniao' | 'perfil' | 'glossario' | 'internet' | 'pauta' | 'conversa' | 'modelo';

/** Optional `origin` of an item in `live-assist-update` and `live-assist-partial`. */
export interface LiveAssistOrigin {
  kind: OriginKind | string;
  label: string;
  /** Meeting title, glossary term or link. */
  detail?: string;
  meetingId?: string;
}

/** Payload of `live-assist-context` and of `get_live_assist_context`. */
export interface LiveAssistContextSummary {
  meetingName: string | null;
  client: string | null;
  project: string | null;
  category: string | null;
  tags: string[];
  speakers: string[];
  pastMeetings: number;
  memoryNotes: number;
  usePastMeetings: boolean;
  profileEnabled: boolean;
  profileFilled: boolean;
  glossaryTerms: number;
  localOnlyBlocked: boolean;
}

/** `get_live_assist_context_settings` / `save_live_assist_context_settings`. */
export interface LiveAssistContextSettings {
  profileEnabled: boolean;
  about: string;
  company: string;
  offerings: string;
  glossary: string;
  usePastMeetings: boolean;
}

export const DEFAULT_CONTEXT_SETTINGS: LiveAssistContextSettings = {
  profileEnabled: true,
  about: '',
  company: '',
  offerings: '',
  glossary: '',
  usePastMeetings: true,
};

export const GLOSSARY_MAX_CHARS = 4000;

const KNOWN_KINDS = new Set(['reuniao', 'perfil', 'glossario', 'internet', 'pauta', 'conversa', 'modelo']);

/** Accepts only a well-formed origin; anything else is treated as "no origin". */
export function parseOrigin(value: unknown): LiveAssistOrigin | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const raw = value as Record<string, unknown>;
  if (typeof raw.kind !== 'string' || typeof raw.label !== 'string') return undefined;
  const label = raw.label.trim();
  if (!label) return undefined;
  const origin: LiveAssistOrigin = { kind: raw.kind, label };
  if (typeof raw.detail === 'string' && raw.detail.trim()) origin.detail = raw.detail.trim();
  if (typeof raw.meetingId === 'string' && raw.meetingId) origin.meetingId = raw.meetingId;
  return origin;
}

/** Text of the tag. The glossary shows its term: "Glossário · SLA". */
export function originLabel(origin: LiveAssistOrigin): string {
  if (origin.kind === 'glossario' && origin.detail) return `${origin.label} · ${origin.detail}`;
  return origin.label;
}

/** Tooltip of the tag: says in plain words where the suggestion came from. */
export function originTitle(origin: LiveAssistOrigin): string {
  switch (origin.kind) {
    case 'reuniao':
      return origin.detail ? `Veio de uma reunião anterior: ${origin.detail}` : 'Veio de uma reunião anterior';
    case 'perfil':
      return 'Veio do seu perfil, nas configurações da ajuda ao vivo';
    case 'glossario':
      return origin.detail ? `Veio do seu glossário: ${origin.detail}` : 'Veio do seu glossário';
    case 'internet':
      return origin.detail ? `Consulta na internet: ${origin.detail}` : 'Consulta na internet';
    case 'pauta':
      return 'Veio do contexto que você escreveu para esta reunião';
    case 'conversa':
      return 'Veio do que foi dito nesta reunião';
    case 'modelo':
      return 'O modelo não apontou uma fonte local. Confira antes de usar.';
    default:
      return origin.label;
  }
}

/** True for origins the owner should double check. */
export function originNeedsCare(origin: LiveAssistOrigin): boolean {
  return origin.kind === 'modelo' || !KNOWN_KINDS.has(origin.kind);
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** One line for the collapsed "Contexto em uso" row. */
export function contextHeadline(summary: LiveAssistContextSummary | null): string {
  if (!summary) return 'Sem contexto ainda';
  const parts: string[] = [];
  if (summary.client) parts.push(summary.client);
  if (summary.project) parts.push(summary.project);
  if (summary.localOnlyBlocked) {
    parts.push('modelo na nuvem');
  } else {
    if (summary.usePastMeetings && summary.pastMeetings > 0) {
      parts.push(plural(summary.pastMeetings, 'reunião anterior', 'reuniões anteriores'));
    }
    parts.push(profileStatus(summary));
  }
  return parts.join(' · ');
}

export function profileStatus(summary: LiveAssistContextSummary): string {
  if (!summary.profileEnabled) return 'perfil desligado';
  if (!summary.profileFilled && summary.glossaryTerms === 0) return 'perfil vazio';
  return 'perfil ligado';
}

export function pastMeetingsStatus(summary: LiveAssistContextSummary): string {
  if (!summary.usePastMeetings) return 'Desligadas';
  if (!summary.client) return 'Cliente ainda não identificado';
  if (summary.pastMeetings === 0) return 'Nenhuma reunião salva deste cliente';
  return `${plural(summary.pastMeetings, 'reunião', 'reuniões')}, ${plural(summary.memoryNotes, 'anotação', 'anotações')}`;
}

/** Counts glossary lines the app will read: `TERMO: explicação` (also `=` or ` - `). */
export function countGlossaryTerms(glossary: string): number {
  const seen = new Set<string>();
  for (const raw of glossary.split('\n')) {
    const line = raw.trim().replace(/^[-*•]\s*/, '');
    const match = /^(.+?)(?::|=| - )(.+)$/.exec(line);
    if (!match) continue;
    const term = match[1].trim().toLowerCase();
    if (!term || term.length > 60 || !match[2].trim()) continue;
    seen.add(term);
  }
  return seen.size;
}
