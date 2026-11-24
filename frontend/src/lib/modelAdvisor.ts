// Types and text helpers of "Modelos recomendados para este Mac".
// The shapes mirror `model_advisor` in the Rust core (camelCase).

export type AdvisorRole = 'speech' | 'voices' | 'assistant';
export type AdvisorEngine = 'whisper' | 'parakeet' | 'voice' | 'ollama';
export type AdvisorFit =
  | 'fits'
  | 'tooLittleMemory'
  | 'doesNotFitTogether'
  | 'needsAppleSilicon'
  | 'otherLanguage'
  | 'noDiskSpace';
export type AdvisorTier = 'light' | 'medium' | 'high';

export interface AdvisorModel {
  id: string;
  role: AdvisorRole;
  engine: AdvisorEngine;
  name: string;
  about: string;
  downloadBytes: number;
  memoryMb: number;
  license: string;
  licenseNote: string | null;
  source: string;
  installed: boolean;
  active: boolean;
  downloading: boolean;
  fit: AdvisorFit;
}

export interface AdvisorCurrent {
  provider: string;
  model: string;
  name: string;
  inCatalog: boolean;
}

export interface AdvisorRoleReport {
  role: AdvisorRole;
  current: AdvisorCurrent | null;
  recommended: AdvisorModel | null;
  reason: string;
  others: AdvisorModel[];
}

export interface AdvisorHardware {
  ramGb: number;
  totalMemoryBytes: number;
  chip: string;
  appleSilicon: boolean;
  cpuCores: number;
  freeDiskBytes: number | null;
}

export interface AdvisorReport {
  hardware: AdvisorHardware;
  language: string;
  tier: AdvisorTier;
  budgetMb: number;
  totalMemoryMb: number;
  voiceModelEnabled: boolean;
  ollama: { endpoint: string; running: boolean; installedModels: string[] };
  /** Ollama tag suggested for the live help. Absent in a core older than this screen. */
  liveHelpModel?: string;
  roles: AdvisorRoleReport[];
}

export const ROLE_LABEL: Record<AdvisorRole, string> = {
  speech: 'Fala (transcrição)',
  voices: 'Vozes (quem está falando)',
  assistant: 'Organização e resumo (depois da reunião)',
};

const FIT_TEXT: Record<AdvisorFit, string> = {
  fits: '',
  tooLittleMemory: 'Pede mais memória do que este computador tem.',
  doesNotFitTogether: 'Não cabe na memória junto com os outros modelos recomendados.',
  needsAppleSilicon: 'Precisa de um Mac com chip Apple.',
  otherLanguage: 'Não é indicado para o idioma das suas reuniões.',
  noDiskSpace: 'Falta espaço em disco para baixar.',
};

export function fitText(fit: AdvisorFit): string {
  return FIT_TEXT[fit] ?? '';
}

/** "51 MB", "673 MB", "8,0 GB". Decimal units, like the download pages show. */
export function bytesText(bytes: number | null | undefined): string {
  if (!bytes || !Number.isFinite(bytes) || bytes <= 0) return '';
  const megabytes = bytes / 1_000_000;
  if (megabytes < 1000) return `${Math.max(1, Math.round(megabytes))} MB`;
  return `${(megabytes / 1000).toLocaleString('pt-BR', { minimumFractionDigits: 1, maximumFractionDigits: 1 })} GB`;
}

/** Memory estimate in GB with one decimal: "1,5 GB". */
export function memoryText(megabytes: number | null | undefined): string {
  if (!megabytes || !Number.isFinite(megabytes) || megabytes <= 0) return '';
  return `${(megabytes / 1000).toLocaleString('pt-BR', { minimumFractionDigits: 1, maximumFractionDigits: 1 })} GB`;
}

/** "Apple M2 · 8 GB de memória · 120 GB livres". */
export function hardwareText(hardware: AdvisorHardware): string {
  const parts: string[] = [];
  if (hardware.chip) parts.push(hardware.chip);
  parts.push(`${hardware.ramGb} GB de memória`);
  const free = bytesText(hardware.freeDiskBytes);
  if (free) parts.push(`${free} livres em disco`);
  return parts.join(' · ');
}

/** Key of a download in progress. Whisper and Parakeet names never collide, but ids are per engine. */
export function downloadKey(engine: AdvisorEngine, id: string): string {
  return `${engine}:${id}`;
}

export type AdvisorAction = 'active' | 'use' | 'download' | 'needsOllama' | 'unavailable';

/**
 * What the button of a model does. For the assistant the models of this list
 * are the ones of the summaries, so "in use" means in use in the summaries;
 * the live help has its own suggestion (`liveHelpAdvice`).
 */
export function actionFor(model: AdvisorModel, report: AdvisorReport): AdvisorAction {
  if (model.fit !== 'fits') return model.installed ? 'use' : 'unavailable';
  if (model.active) return 'active';
  if (model.installed) return 'use';
  if (model.engine === 'ollama' && !report.ollama.running) return 'needsOllama';
  return 'download';
}

const CLOUD_PROVIDERS = ['openai', 'claude', 'groq', 'openrouter', 'custom-openai'];

/** A catalog model this much bigger than the suggested one is called heavy for the live help. */
const HEAVY_FOR_LIVE_HELP = 1.5;

export interface LiveHelpAdvice {
  /** The suggested model, as the catalog describes it. */
  recommended: AdvisorModel;
  /** The live help already uses the suggested model. */
  inUse: boolean;
  /** The live help runs on this computer (the summaries are not on a cloud provider). */
  local: boolean;
  /**
   * Name of the catalog model the live help uses now, when it is much bigger
   * than the suggested one: during a meeting it takes several times the
   * memory. Null when there is nothing to warn about.
   */
  heavyCurrent: string | null;
  action: AdvisorAction;
}

/**
 * The live help runs during the meeting, next to the speech and voice models,
 * and calls the model every few seconds; the summaries run after it. So the
 * live help gets the lightest model whatever the summaries use. Null when the
 * core did not send a suggestion.
 */
export function liveHelpAdvice(report: AdvisorReport, liveModel?: string | null): LiveHelpAdvice | null {
  const assistant = report.roles.find((role) => role.role === 'assistant');
  if (!assistant || !report.liveHelpModel) return null;
  const catalog = [assistant.recommended, ...assistant.others].filter((model): model is AdvisorModel => !!model);
  const recommended = catalog.find((model) => model.id === report.liveHelpModel);
  if (!recommended) return null;
  const provider = assistant.current?.provider?.trim().toLowerCase() ?? '';
  const local = !CLOUD_PROVIDERS.includes(provider);
  const current = (liveModel ?? '').trim();
  const inUse = current === recommended.id;
  const known = catalog.find((model) => model.id === current);
  const heavy = local && !!known && known.downloadBytes > recommended.downloadBytes * HEAVY_FOR_LIVE_HELP;
  const action: AdvisorAction = inUse
    ? 'active'
    : recommended.installed
      ? 'use'
      : report.ollama.running
        ? 'download'
        : 'needsOllama';
  return { recommended, inUse, local, heavyCurrent: heavy && known ? known.name : null, action };
}

/** The summaries use this Ollama model. */
export function usedInSummaries(model: AdvisorModel, report: AdvisorReport): boolean {
  const current = report.roles.find((role) => role.role === 'assistant')?.current;
  return !!current && current.provider === 'ollama' && current.model === model.id;
}

/** The summaries use a local Ollama model (so switching it is a like-for-like change). */
export function summariesUseOllama(report: AdvisorReport): boolean {
  return report.roles.find((role) => role.role === 'assistant')?.current?.provider === 'ollama';
}
