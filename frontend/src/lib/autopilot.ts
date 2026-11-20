import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/* Piloto automático pós-reunião: tipos, chamadas ao núcleo Rust e funções puras. */

export type AutopilotStepKey = 'summary' | 'tasks' | 'title_tags' | 'email';
export type AutopilotStepState = 'done' | 'failed' | 'skipped' | 'off';
export type AutopilotRunStatus = 'running' | 'done' | 'partial' | 'skipped' | 'failed';

export interface AutopilotSettings {
  enabled: boolean;
  stepSummary: boolean;
  stepTasks: boolean;
  stepTitleTags: boolean;
  stepEmail: boolean;
  templateId: string;
}

export const DEFAULT_AUTOPILOT_SETTINGS: AutopilotSettings = {
  enabled: false,
  stepSummary: true,
  stepTasks: true,
  stepTitleTags: true,
  stepEmail: true,
  templateId: 'standard_meeting',
};

export interface AutopilotTask {
  text: string;
  owner: string | null;
  due: string | null;
}

export interface AutopilotResult {
  meetingId: string;
  status: AutopilotRunStatus;
  note: string | null;
  steps: Partial<Record<AutopilotStepKey, AutopilotStepState>>;
  tasks: AutopilotTask[];
  decisions: string[];
  pending: string[];
  suggestedTitle: string | null;
  titleApplied: boolean;
  suggestedTags: string[];
  tagsApplied: boolean;
  emailSubject: string | null;
  emailBody: string | null;
  provider: string | null;
  model: string | null;
  leavesMachine: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface AutopilotStatus {
  provider: string;
  model: string;
  paid: boolean;
  hasCredentials: boolean;
  willUse: 'configured' | 'local_fallback' | 'none';
  willUseProvider: string | null;
  willUseModel: string | null;
  message: string | null;
}

export const STEP_LABELS: Record<AutopilotStepKey, string> = {
  summary: 'Resumo',
  tasks: 'Tarefas, decisões e pendências',
  title_tags: 'Título e etiquetas',
  email: 'Rascunho de e-mail',
};

export const STATUS_LABELS: Record<AutopilotRunStatus, string> = {
  running: 'Em andamento',
  done: 'Concluído',
  partial: 'Concluído em parte',
  skipped: 'Não executado',
  failed: 'Falhou',
};

const PROVIDER_NAMES: Record<string, string> = {
  openai: 'OpenAI',
  claude: 'Claude (Anthropic)',
  groq: 'Groq',
  openrouter: 'OpenRouter',
  'custom-openai': 'servidor compatível com OpenAI',
  ollama: 'Ollama (local)',
  'builtin-ai': 'IA integrada (local)',
};

export function providerLabel(provider: string | null | undefined): string {
  if (!provider) return '';
  return PROVIDER_NAMES[provider.toLowerCase()] ?? provider;
}

/** Aviso de custo e privacidade para a tela de configuração. `null` quando é tudo local. */
export function costNotice(status: AutopilotStatus | null): { tone: 'warning' | 'info'; text: string } | null {
  if (!status) return null;
  if (status.willUse === 'none') {
    return {
      tone: 'info',
      text: status.message ?? 'Sem provedor disponível: o piloto automático não vai rodar.',
    };
  }
  if (status.willUse === 'local_fallback') {
    return {
      tone: 'info',
      text: `${providerLabel(status.provider)} está sem chave. O piloto vai usar o modelo local (${status.willUseModel ?? ''}), sem custo.`,
    };
  }
  if (status.paid) {
    return {
      tone: 'warning',
      text: `O provedor de resumo é pago (${providerLabel(status.provider)}). Cada reunião processada consome créditos e envia o texto da conversa para esse serviço.`,
    };
  }
  return null;
}

/* ---------- Normalização (o núcleo pode estar ausente ou antigo) ---------- */

function asString(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value : null;
}

function asStringList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string' && item.trim() !== '') : [];
}

export function normalizeSettings(raw: unknown): AutopilotSettings {
  const value = (raw ?? {}) as Partial<Record<keyof AutopilotSettings, unknown>>;
  const flag = (item: unknown, fallback: boolean) => (typeof item === 'boolean' ? item : fallback);
  return {
    enabled: flag(value.enabled, DEFAULT_AUTOPILOT_SETTINGS.enabled),
    stepSummary: flag(value.stepSummary, true),
    stepTasks: flag(value.stepTasks, true),
    stepTitleTags: flag(value.stepTitleTags, true),
    stepEmail: flag(value.stepEmail, true),
    templateId: asString(value.templateId) ?? DEFAULT_AUTOPILOT_SETTINGS.templateId,
  };
}

export function normalizeResult(raw: unknown): AutopilotResult | null {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as Record<string, unknown>;
  const meetingId = asString(value.meetingId);
  if (!meetingId) return null;
  const tasks = Array.isArray(value.tasks)
    ? value.tasks
        .map((item): AutopilotTask | null => {
          if (!item || typeof item !== 'object') return null;
          const task = item as Record<string, unknown>;
          const text = asString(task.text);
          return text ? { text, owner: asString(task.owner), due: asString(task.due) } : null;
        })
        .filter((item): item is AutopilotTask => item !== null)
    : [];
  const status = (['running', 'done', 'partial', 'skipped', 'failed'] as const).find((s) => s === value.status) ?? 'failed';
  return {
    meetingId,
    status,
    note: asString(value.note),
    steps: (value.steps && typeof value.steps === 'object' ? value.steps : {}) as AutopilotResult['steps'],
    tasks,
    decisions: asStringList(value.decisions),
    pending: asStringList(value.pending),
    suggestedTitle: asString(value.suggestedTitle),
    titleApplied: value.titleApplied === true,
    suggestedTags: asStringList(value.suggestedTags),
    tagsApplied: value.tagsApplied === true,
    emailSubject: asString(value.emailSubject),
    emailBody: asString(value.emailBody),
    provider: asString(value.provider),
    model: asString(value.model),
    leavesMachine: value.leavesMachine === true,
    createdAt: asString(value.createdAt) ?? '',
    updatedAt: asString(value.updatedAt) ?? '',
  };
}

/** True when there is something worth showing in the meeting page. */
export function hasContent(result: AutopilotResult | null): boolean {
  if (!result) return false;
  return (
    result.status === 'running' ||
    result.tasks.length > 0 ||
    result.decisions.length > 0 ||
    result.pending.length > 0 ||
    result.suggestedTitle !== null ||
    result.suggestedTags.length > 0 ||
    result.emailBody !== null ||
    result.note !== null
  );
}

/* ---------- Texto para copiar ---------- */

export function tasksToText(tasks: AutopilotTask[]): string {
  return tasks
    .map((task) => {
      const parts = [`- ${task.text}`];
      if (task.owner) parts.push(`(${task.owner})`);
      if (task.due) parts.push(`até ${task.due}`);
      return parts.join(' ');
    })
    .join('\n');
}

export function listToText(title: string, items: string[]): string {
  if (items.length === 0) return '';
  return `${title}\n${items.map((item) => `- ${item}`).join('\n')}`;
}

export function emailToText(result: Pick<AutopilotResult, 'emailSubject' | 'emailBody'>): string {
  const subject = result.emailSubject ? `Assunto: ${result.emailSubject}\n\n` : '';
  return `${subject}${result.emailBody ?? ''}`.trim();
}

/* ---------- Núcleo Rust ---------- */

export async function getAutopilotSettings(): Promise<AutopilotSettings> {
  try {
    return normalizeSettings(await invoke('get_autopilot_settings'));
  } catch {
    return DEFAULT_AUTOPILOT_SETTINGS;
  }
}

export async function saveAutopilotSettings(settings: AutopilotSettings): Promise<AutopilotSettings> {
  return normalizeSettings(await invoke('save_autopilot_settings', { settings }));
}

export async function getAutopilotStatus(): Promise<AutopilotStatus | null> {
  try {
    const raw = (await invoke('get_autopilot_status')) as Partial<AutopilotStatus> | null;
    if (!raw || typeof raw.provider !== 'string') return null;
    return {
      provider: raw.provider,
      model: raw.model ?? '',
      paid: raw.paid === true,
      hasCredentials: raw.hasCredentials === true,
      willUse: raw.willUse === 'local_fallback' || raw.willUse === 'none' ? raw.willUse : 'configured',
      willUseProvider: raw.willUseProvider ?? null,
      willUseModel: raw.willUseModel ?? null,
      message: raw.message ?? null,
    };
  } catch {
    return null;
  }
}

export async function getAutopilotResult(meetingId: string): Promise<AutopilotResult | null> {
  try {
    return normalizeResult(await invoke('get_autopilot_result', { meetingId }));
  } catch {
    return null;
  }
}

export async function runAutopilotForMeeting(meetingId: string): Promise<boolean> {
  return (await invoke('run_autopilot_for_meeting', { meetingId })) === true;
}

export async function applyAutopilotSuggestion(meetingId: string, kind: 'title' | 'tags'): Promise<boolean> {
  return (await invoke('apply_autopilot_suggestion', { meetingId, kind })) === true;
}

/** Escuta o fim e o progresso do piloto. Devolve a limpeza; nunca lança. */
export async function listenAutopilotEvents(handlers: {
  onProgress?: (event: { meetingId: string; step: string; state: string }) => void;
  onFinished?: (event: { meetingId: string; status: string }) => void;
}): Promise<UnlistenFn> {
  const unlisteners: UnlistenFn[] = [];
  try {
    if (handlers.onProgress) {
      unlisteners.push(
        await listen<{ meetingId: string; step: string; state: string }>('autopilot-progress', (event) => {
          if (event.payload && typeof event.payload.meetingId === 'string') handlers.onProgress?.(event.payload);
        }),
      );
    }
    if (handlers.onFinished) {
      unlisteners.push(
        await listen<{ meetingId: string; status: string }>('autopilot-finished', (event) => {
          if (event.payload && typeof event.payload.meetingId === 'string') handlers.onFinished?.(event.payload);
        }),
      );
    }
  } catch {
    // Fora do app nativo não há eventos.
  }
  return () => unlisteners.forEach((unlisten) => unlisten());
}
