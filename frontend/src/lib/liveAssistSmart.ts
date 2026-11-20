// Proactive part of the live assistant: alerts, "O que eu perdi?", consumption and economy.
// Pure helpers and types; the Rust side is `live_assist/` (see `commands.rs`).

export type SmartAlertKind = 'pergunta' | 'decisao' | 'tarefa' | 'divergencia';

export interface SmartAlert {
  id: string;
  kind: SmartAlertKind;
  text: string;
  speaker?: string;
  /** For `divergencia`: what the earlier meeting said, with its origin. */
  detail?: string;
}

export interface SmartSettings {
  questionAlerts: boolean;
  decisionAlerts: boolean;
  taskAlerts: boolean;
  contradictionAlerts: boolean;
  /** Writes an answer to a detected question by itself. Calls the model. */
  autoAnswer: boolean;
  economy: 'auto' | 'on' | 'off';
}

export const DEFAULT_SMART_SETTINGS: SmartSettings = {
  questionAlerts: true,
  decisionAlerts: true,
  taskAlerts: true,
  contradictionAlerts: true,
  autoAnswer: false,
  economy: 'auto',
};

export interface UsageSnapshot {
  calls: number;
  inputTokens: number;
  outputTokens: number;
  /** Triggers that economy mode held back or merged into a later call. */
  grouped: number;
  /** True when the text goes to a cloud provider. */
  paid: boolean;
  economy: boolean;
}

export interface RecapResult {
  minutes: number;
  bullets: string[];
  inputTokens: number;
  outputTokens: number;
  paid: boolean;
}

export const RECAP_MINUTES = [2, 5, 10, 20] as const;
export const MAX_ALERTS = 12;

const KINDS: SmartAlertKind[] = ['pergunta', 'decisao', 'tarefa', 'divergencia'];

export function parseAlert(raw: unknown): SmartAlert | null {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as Record<string, unknown>;
  const kind = value.kind as SmartAlertKind;
  const text = typeof value.text === 'string' ? value.text.trim() : '';
  if (!KINDS.includes(kind) || !text || typeof value.id !== 'string') return null;
  return {
    id: value.id,
    kind,
    text,
    speaker: typeof value.speaker === 'string' && value.speaker ? value.speaker : undefined,
    detail: typeof value.detail === 'string' && value.detail ? value.detail : undefined,
  };
}

/** Newest first, no repeated id or text of the same kind, capped. */
export function addAlert(current: SmartAlert[], incoming: SmartAlert): SmartAlert[] {
  const same = (alert: SmartAlert) =>
    alert.id === incoming.id || (alert.kind === incoming.kind && alert.text.toLowerCase() === incoming.text.toLowerCase());
  if (current.some(same)) return current;
  return [incoming, ...current].slice(0, MAX_ALERTS);
}

export function alertTitle(kind: SmartAlertKind): string {
  switch (kind) {
    case 'pergunta':
      return 'Pergunta para você';
    case 'decisao':
      return 'Decisão';
    case 'tarefa':
      return 'Tarefa';
    case 'divergencia':
      return 'Possível divergência';
  }
}

export function formatTokens(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0';
  if (value < 1000) return String(Math.round(value));
  if (value < 10000) return `${(value / 1000).toFixed(1).replace('.', ',')} mil`;
  return `${Math.round(value / 1000)} mil`;
}

function callsText(calls: number): string {
  return `${calls} ${calls === 1 ? 'chamada' : 'chamadas'}`;
}

/** One line for the header. Local models cost nothing; cloud use is shown in estimated tokens. */
export function usageLabel(usage: UsageSnapshot | null): string {
  if (!usage) return '';
  if (!usage.paid) {
    return usage.calls === 0 ? 'Modelo local, sem custo' : `Modelo local, sem custo · ${callsText(usage.calls)}`;
  }
  const total = usage.inputTokens + usage.outputTokens;
  const base = `Nuvem · ${callsText(usage.calls)} · ~${formatTokens(total)} tokens`;
  return usage.economy ? `${base} · economia ligada` : base;
}

export function usageTitle(usage: UsageSnapshot | null): string {
  if (!usage) return '';
  const parts = [
    `Estimativa (cerca de 1 token a cada 4 caracteres): ${formatTokens(usage.inputTokens)} enviados e ${formatTokens(usage.outputTokens)} recebidos.`,
  ];
  if (usage.grouped > 0) parts.push(`${usage.grouped} disparos agrupados pela economia.`);
  parts.push(usage.paid ? 'O texto vai para o provedor de nuvem configurado no resumo.' : 'Tudo roda nesta máquina.');
  return parts.join(' ');
}

export function recapCostNote(result: Pick<RecapResult, 'inputTokens' | 'outputTokens' | 'paid'>): string {
  const total = result.inputTokens + result.outputTokens;
  return result.paid ? `Nuvem · ~${formatTokens(total)} tokens nesta consulta` : 'Modelo local, sem custo';
}
