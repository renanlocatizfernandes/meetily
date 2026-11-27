/**
 * Área de trabalho em volta das reuniões: tarefas, participantes, clientes,
 * exportação em texto e a conexão com o Obsidian.
 *
 * Camada fina sobre os comandos Tauri do módulo `workspace`. As leituras são
 * tolerantes (devolvem lista vazia ou padrões); as gravações lançam o erro
 * para a tela mostrar.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type TaskDirection = 'mine' | 'theirs' | 'unassigned';
export type TaskStatus = 'open' | 'done';

export interface Task {
  id: string;
  meetingId: string | null;
  meetingTitle: string | null;
  /** RFC 3339. */
  meetingDate: string | null;
  client: string | null;
  text: string;
  owner: string | null;
  /** Prazo como foi dito ("até sexta"). */
  due: string | null;
  /** Prazo resolvido, AAAA-MM-DD. */
  dueDate: string | null;
  direction: TaskDirection;
  status: TaskStatus;
  source: 'auto' | 'manual';
  evidence: string | null;
  createdAt: string;
  doneAt: string | null;
}

export interface TaskInput {
  text: string;
  meetingId?: string | null;
  owner?: string | null;
  dueDate?: string | null;
  direction?: TaskDirection | null;
}

/** Campo ausente mantém o valor; `''` limpa. */
export interface TaskPatch {
  text?: string;
  owner?: string;
  dueDate?: string;
  direction?: TaskDirection;
  status?: TaskStatus;
}

export interface TaskGeneration {
  origin: string;
  added: number;
  leavesMachine: boolean;
  tasks: Task[];
}

export interface Participant {
  id: string;
  meetingId: string;
  name: string;
  email: string | null;
  organization: string | null;
  source: string;
}

export interface SpeakerMatch {
  speakerKey: string;
  name: string;
  /** Parte das falas dessa voz que bateu com a pessoa (0 a 1). */
  share: number;
  segments: number;
}

export interface ImportResult {
  kind: string;
  added: number;
  participants: Participant[];
  matches: SpeakerMatch[];
  alignedByText: boolean;
}

export interface ClientInfo {
  name: string;
  aliases: string[];
  domains: string[];
  notes: string;
  meetings: number;
  lastMeeting: string | null;
  totalSeconds: number;
  openTasks: number;
  people: string[];
}

export interface ClientInput {
  name: string;
  aliases: string[];
  domains: string[];
  notes?: string | null;
}

export interface WorkspaceSettings {
  userName: string;
  userAliases: string[];
  autoTasks: boolean;
  autoParticipants: boolean;
  obsidianVault: string;
  obsidianFolder: string;
  obsidianAuto: boolean;
  obsidianTranscript: boolean;
  obsidianLinks: boolean;
}

export interface VaultInfo {
  name: string;
  path: string;
  isVault: boolean;
}

export interface ObsidianNote {
  path: string;
  exists: boolean;
}

export const DEFAULT_WORKSPACE_SETTINGS: WorkspaceSettings = {
  userName: '',
  userAliases: [],
  autoTasks: true,
  autoParticipants: true,
  obsidianVault: '',
  obsidianFolder: 'Reuniões',
  obsidianAuto: false,
  obsidianTranscript: true,
  obsidianLinks: true,
};

async function read<T>(command: string, args: Record<string, unknown> | undefined, fallback: T): Promise<T> {
  try {
    const value = await invoke<T | null>(command, args);
    return value ?? fallback;
  } catch (error) {
    console.warn(`Falha em ${command}`, error);
    return fallback;
  }
}

/* ---------- Tarefas ---------- */

export const listTasks = (meetingId?: string) =>
  read<Task[]>('tasks_list', { meetingId: meetingId ?? null }, []);

export const generateTasks = (meetingId: string, force = false) =>
  invoke<TaskGeneration>('tasks_generate', { meetingId, force });

export const addTask = (input: TaskInput) => invoke<Task>('tasks_add', { input });

export const updateTask = (id: string, patch: TaskPatch) => invoke<Task>('tasks_update', { id, patch });

export const deleteTask = (id: string) => invoke<boolean>('tasks_delete', { id });

/* ---------- Participantes ---------- */

export const listParticipants = (meetingId: string) =>
  read<Participant[]>('participants_list', { meetingId }, []);

export const addParticipant = (meetingId: string, name: string, email?: string) =>
  invoke<Participant[]>('participants_add', { meetingId, name, email: email || null });

export const removeParticipant = (id: string) => invoke<boolean>('participants_remove', { id });

export const participantsFromCalendar = (meetingId: string) =>
  invoke<Participant[]>('participants_from_calendar', { meetingId });

/** Abre o seletor de arquivo. `null` quando a pessoa cancela. */
export const importParticipantsFile = (meetingId: string) =>
  invoke<ImportResult | null>('participants_import_file', { meetingId });

export const renameSpeaker = (meetingId: string, speakerKey: string, displayName: string) =>
  invoke('rename_speaker', { meetingId, speakerKey, displayName });

/* ---------- Clientes ---------- */

export const listClients = () => read<ClientInfo[]>('clients_list', undefined, []);

export const saveClient = (input: ClientInput) => invoke<ClientInfo[]>('clients_save', { input });

export const renameClient = (from: string, to: string) => invoke<ClientInfo[]>('clients_rename', { from, to });

export const removeClient = (name: string, clearMeetings = false) =>
  invoke<ClientInfo[]>('clients_remove', { name, clearMeetings });

/* ---------- Exportação e Obsidian ---------- */

export type ExportFormat = 'txt' | 'md';

export const exportMeetingText = (meetingId: string, format: ExportFormat) =>
  invoke<string>('export_meeting_text', { meetingId, format });

/** Abre "Salvar como". Devolve o caminho gravado ou `null` se cancelado. */
export const exportMeetingFile = (meetingId: string, format: ExportFormat) =>
  invoke<string | null>('export_meeting_file', { meetingId, format });

export const detectVaults = () => read<VaultInfo[]>('obsidian_detect_vaults', undefined, []);

export const pickVault = () => invoke<VaultInfo | null>('obsidian_pick_vault');

export const exportToObsidian = (meetingId: string, open = false) =>
  invoke<ObsidianNote>('obsidian_export_meeting', { meetingId, open });

export const obsidianNoteFor = (meetingId: string) =>
  read<ObsidianNote | null>('obsidian_note_for_meeting', { meetingId }, null);

export const openObsidianNote = (meetingId: string) => invoke('obsidian_open_note', { meetingId });

/* ---------- Preferências ---------- */

export const getWorkspaceSettings = () =>
  read<WorkspaceSettings>('workspace_get_settings', undefined, DEFAULT_WORKSPACE_SETTINGS);

export const saveWorkspaceSettings = (settings: WorkspaceSettings) =>
  invoke<WorkspaceSettings>('workspace_save_settings', { settings });

/* ---------- Eventos ---------- */

export type WorkspaceEvent = 'tasks-updated' | 'participants-updated' | 'clients-updated' | 'meeting-organized';

/** Chama `handler` quando qualquer um dos eventos chega. Devolve a limpeza. */
export function onWorkspaceEvents(events: WorkspaceEvent[], handler: () => void): () => void {
  let cancelled = false;
  const unlisteners: UnlistenFn[] = [];
  events.forEach((event) => {
    listen(event, () => handler())
      .then((unlisten) => {
        if (cancelled) unlisten();
        else unlisteners.push(unlisten);
      })
      .catch(() => undefined);
  });
  return () => {
    cancelled = true;
    unlisteners.forEach((unlisten) => unlisten());
  };
}

/* ---------- Datas (puras, testáveis) ---------- */

export function toDayKey(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, '0');
  const d = String(date.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

/** Dia local de uma data RFC 3339; `null` se inválida. */
export function dayKeyOf(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? null : toDayKey(date);
}

export function fromDayKey(key: string): Date {
  const [y, m, d] = key.split('-').map(Number);
  return new Date(y, (m || 1) - 1, d || 1);
}

export function addDays(date: Date, days: number): Date {
  const next = new Date(date);
  next.setDate(next.getDate() + days);
  return next;
}

/** Segunda-feira da semana da data. */
export function startOfWeek(date: Date): Date {
  const start = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const weekday = (start.getDay() + 6) % 7;
  return addDays(start, -weekday);
}

export function weekDays(date: Date): Date[] {
  const start = startOfWeek(date);
  return Array.from({ length: 7 }, (_, index) => addDays(start, index));
}

export function formatDuration(seconds: number | null | undefined): string {
  if (!seconds || seconds <= 0) return '';
  const minutes = Math.round(seconds / 60);
  if (minutes < 1) return '<1 min';
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${hours} h ${rest} min` : `${hours} h`;
}

export function formatClock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(2, '0');
  const ss = String(s).padStart(2, '0');
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export type DueState = 'overdue' | 'today' | 'soon' | 'later' | 'none';

export function dueState(task: Pick<Task, 'dueDate' | 'status'>, today = new Date()): DueState {
  if (!task.dueDate || task.status === 'done') return 'none';
  const todayKey = toDayKey(today);
  if (task.dueDate < todayKey) return 'overdue';
  if (task.dueDate === todayKey) return 'today';
  return task.dueDate <= toDayKey(addDays(today, 7)) ? 'soon' : 'later';
}

export function formatDueDate(dueDate: string): string {
  return fromDayKey(dueDate).toLocaleDateString('pt-BR', { day: '2-digit', month: 'short' });
}

/* ---------- Conversa ---------- */

export interface ConversationSegment {
  id: string;
  timestamp: number;
  text: string;
  speaker?: string | null;
  speakerKey?: string | null;
}

export interface ConversationTurn {
  id: string;
  start: number;
  speaker: string;
  speakerKey: string;
  mine: boolean;
  lines: string[];
}

const MIC_KEYS = new Set(['mic', 'microphone', 'me', 'user']);

export function isMineKey(speakerKey: string | null | undefined): boolean {
  return !!speakerKey && MIC_KEYS.has(speakerKey.toLowerCase());
}

/** Junta falas seguidas da mesma voz (até `gapSeconds` de pausa) em um balão. */
const MAX_TURN_LINES = 8;

export function buildTurns(segments: ConversationSegment[], myName = 'Você', gapSeconds = 45): ConversationTurn[] {
  const turns: ConversationTurn[] = [];
  let lastTime = 0;
  for (const segment of segments) {
    const text = segment.text.trim();
    if (!text) continue;
    const speakerKey = segment.speakerKey || 'unknown';
    const mine = isMineKey(speakerKey);
    const speaker = segment.speaker?.trim() || (mine ? myName : 'Participante');
    const last = turns[turns.length - 1];
    // Um balão não passa de MAX_TURN_LINES falas: reunião sem vozes separadas viraria um bloco só.
    if (last && last.speakerKey === speakerKey && last.speaker === speaker && segment.timestamp - lastTime <= gapSeconds && last.lines.length < MAX_TURN_LINES) {
      last.lines.push(text);
    } else {
      turns.push({ id: segment.id, start: segment.timestamp, speaker, speakerKey, mine, lines: [text] });
    }
    lastTime = segment.timestamp;
  }
  return turns;
}

const NAME_COLORS = [
  'text-sky-600 dark:text-sky-400',
  'text-violet-600 dark:text-violet-400',
  'text-amber-600 dark:text-amber-400',
  'text-rose-600 dark:text-rose-400',
  'text-emerald-600 dark:text-emerald-400',
  'text-fuchsia-600 dark:text-fuchsia-400',
  'text-orange-600 dark:text-orange-400',
  'text-cyan-600 dark:text-cyan-400',
];

/** Cor estável por nome, como nos grupos de mensagens. */
export function nameColor(name: string): string {
  let hash = 0;
  for (let index = 0; index < name.length; index += 1) hash = (hash * 31 + name.charCodeAt(index)) >>> 0;
  return NAME_COLORS[hash % NAME_COLORS.length];
}

/** Nomes dos participantes de cada reunião (para a busca). */
export const participantsIndex = () =>
  read<Record<string, string[]>>('participants_index', undefined, {});
