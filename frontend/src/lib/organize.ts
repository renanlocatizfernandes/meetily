/**
 * Organização das reuniões (título automático, cliente, projeto, categoria, etiquetas, fixadas).
 *
 * Camada de dados fina sobre os comandos Tauri. Toda chamada é tolerante: se o comando não existir
 * (app nativo mais antigo) ou falhar, quem chama recebe `null`/padrões e a interface volta para a
 * lista simples de `api_get_meetings`.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type TitleSource = 'default' | 'auto' | 'manual';
export type ClassificationSource = 'none' | 'auto' | 'manual';

export interface MeetingOrg {
  id: string;
  title: string;
  /** RFC 3339. Vazio quando a lista veio do modo simples. */
  createdAt: string;
  client: string | null;
  project: string | null;
  category: string | null;
  tags: string[];
  titleSource: TitleSource;
  classificationSource: ClassificationSource;
  pinned: boolean;
  durationSeconds: number | null;
}

export interface FacetCount {
  name: string;
  count: number;
}

export interface MeetingFacets {
  clients: FacetCount[];
  projects: FacetCount[];
  categories: FacetCount[];
  tags: FacetCount[];
}

export interface OrganizationSettings {
  autoOrganize: boolean;
  titleTemplate: string;
  knownClients: string[];
  categories: string[];
}

export interface ClassificationPatch {
  /** `null` mantém o valor atual; `''` limpa. */
  client?: string | null;
  project?: string | null;
  category?: string | null;
  tags?: string[] | null;
  pinned?: boolean | null;
}

export interface OrganizeProgress {
  meetingId: string;
  index: number;
  total: number;
}

export interface OrganizeAllResult {
  processed: number;
  failed: number;
}

export const EMPTY_FACETS: MeetingFacets = { clients: [], projects: [], categories: [], tags: [] };

export const DEFAULT_ORGANIZATION_SETTINGS: OrganizationSettings = {
  autoOrganize: true,
  titleTemplate: '{cliente} · {assunto} · {data}',
  knownClients: [],
  categories: [],
};

export const TITLE_PLACEHOLDERS: Array<{ token: string; description: string }> = [
  { token: '{cliente}', description: 'Cliente da reunião' },
  { token: '{projeto}', description: 'Projeto' },
  { token: '{assunto}', description: 'Assunto principal' },
  { token: '{data}', description: 'Data da reunião' },
  { token: '{hora}', description: 'Hora de início' },
];

function asString(value: unknown): string | null {
  return typeof value === 'string' && value.trim() !== '' ? value : null;
}

/** Aceita a resposta do backend e garante o formato esperado pela interface. */
export function normalizeMeetingOrg(raw: unknown): MeetingOrg | null {
  if (!raw || typeof raw !== 'object') return null;
  const item = raw as Record<string, unknown>;
  if (typeof item.id !== 'string') return null;
  const titleSource = item.titleSource;
  const classificationSource = item.classificationSource;
  return {
    id: item.id,
    title: typeof item.title === 'string' ? item.title : '',
    createdAt: typeof item.createdAt === 'string' ? item.createdAt : '',
    client: asString(item.client),
    project: asString(item.project),
    category: asString(item.category),
    tags: Array.isArray(item.tags) ? item.tags.filter((tag): tag is string => typeof tag === 'string') : [],
    titleSource: titleSource === 'auto' || titleSource === 'manual' ? titleSource : 'default',
    classificationSource: classificationSource === 'auto' || classificationSource === 'manual' ? classificationSource : 'none',
    pinned: item.pinned === true,
    durationSeconds: typeof item.durationSeconds === 'number' ? item.durationSeconds : null,
  };
}

/** Linha do modo simples (só id e título). */
export function plainMeetingOrg(id: string, title: string): MeetingOrg {
  return {
    id,
    title,
    createdAt: '',
    client: null,
    project: null,
    category: null,
    tags: [],
    titleSource: 'default',
    classificationSource: 'none',
    pinned: false,
    durationSeconds: null,
  };
}

/** `null` quando o comando não existe ou falha: use a lista simples. */
export async function listMeetingsOrganized(): Promise<MeetingOrg[] | null> {
  try {
    const result = await invoke<unknown>('list_meetings_organized');
    if (!Array.isArray(result)) return null;
    return result.map(normalizeMeetingOrg).filter((item): item is MeetingOrg => item !== null);
  } catch {
    return null;
  }
}

/** Gera título e classificação com o modelo local. Lança erro para a interface avisar. */
export async function organizeMeeting(meetingId: string, force: boolean): Promise<MeetingOrg> {
  const result = normalizeMeetingOrg(await invoke<unknown>('organize_meeting', { meetingId, force }));
  if (!result) throw new Error('Resposta inesperada ao organizar a reunião.');
  return result;
}

export async function updateMeetingClassification(meetingId: string, patch: ClassificationPatch): Promise<MeetingOrg> {
  const result = normalizeMeetingOrg(await invoke<unknown>('update_meeting_classification', {
    meetingId,
    client: patch.client ?? null,
    project: patch.project ?? null,
    category: patch.category ?? null,
    tags: patch.tags ?? null,
    pinned: patch.pinned ?? null,
  }));
  if (!result) throw new Error('Resposta inesperada ao salvar a classificação.');
  return result;
}

function normalizeFacetList(raw: unknown): FacetCount[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .map((item) => {
      if (!item || typeof item !== 'object') return null;
      const entry = item as Record<string, unknown>;
      if (typeof entry.name !== 'string' || entry.name.trim() === '') return null;
      return { name: entry.name, count: typeof entry.count === 'number' ? entry.count : 0 };
    })
    .filter((item): item is FacetCount => item !== null);
}

export async function listMeetingFacets(): Promise<MeetingFacets> {
  try {
    const raw = await invoke<Record<string, unknown> | null>('list_meeting_facets');
    if (!raw) return EMPTY_FACETS;
    return {
      clients: normalizeFacetList(raw.clients),
      projects: normalizeFacetList(raw.projects),
      categories: normalizeFacetList(raw.categories),
      tags: normalizeFacetList(raw.tags),
    };
  } catch {
    return EMPTY_FACETS;
  }
}

function normalizeStringList(raw: unknown): string[] {
  return Array.isArray(raw) ? raw.filter((item): item is string => typeof item === 'string' && item.trim() !== '') : [];
}

/** `null` quando o recurso não está disponível neste app. */
export async function getOrganizationSettings(): Promise<OrganizationSettings | null> {
  try {
    const raw = await invoke<Record<string, unknown> | null>('get_organization_settings');
    if (!raw) return null;
    return {
      autoOrganize: raw.autoOrganize !== false,
      titleTemplate: typeof raw.titleTemplate === 'string' ? raw.titleTemplate : DEFAULT_ORGANIZATION_SETTINGS.titleTemplate,
      knownClients: normalizeStringList(raw.knownClients),
      categories: normalizeStringList(raw.categories),
    };
  } catch {
    return null;
  }
}

export async function saveOrganizationSettings(settings: OrganizationSettings): Promise<OrganizationSettings> {
  const raw = await invoke<Record<string, unknown> | null>('save_organization_settings', { settings });
  if (!raw) return settings;
  return {
    autoOrganize: raw.autoOrganize !== false,
    titleTemplate: typeof raw.titleTemplate === 'string' ? raw.titleTemplate : settings.titleTemplate,
    knownClients: normalizeStringList(raw.knownClients),
    categories: normalizeStringList(raw.categories),
  };
}

export async function organizeAllMeetings(onlyUnorganized: boolean): Promise<OrganizeAllResult> {
  const raw = await invoke<Partial<OrganizeAllResult> | null>('organize_all_meetings', { onlyUnorganized });
  return { processed: raw?.processed ?? 0, failed: raw?.failed ?? 0 };
}

export async function cancelOrganizeAll(): Promise<void> {
  await invoke('cancel_organize_all');
}

/** Escuta os eventos de organização. Devolve a função de limpeza; nunca lança. */
export async function listenOrganizeEvents(handlers: {
  onProgress?: (progress: OrganizeProgress) => void;
  onOrganized?: (meeting: MeetingOrg) => void;
}): Promise<UnlistenFn> {
  const unlisteners: UnlistenFn[] = [];
  try {
    if (handlers.onProgress) {
      unlisteners.push(await listen<OrganizeProgress>('meeting-organize-progress', (event) => {
        const payload = event.payload;
        if (payload && typeof payload.meetingId === 'string') handlers.onProgress?.(payload);
      }));
    }
    if (handlers.onOrganized) {
      unlisteners.push(await listen<unknown>('meeting-organized', (event) => {
        const meeting = normalizeMeetingOrg(event.payload);
        if (meeting) handlers.onOrganized?.(meeting);
      }));
    }
  } catch {
    // Fora do app nativo não há eventos; a lista segue funcionando.
  }
  return () => unlisteners.forEach((unlisten) => unlisten());
}

/* ---------- Agrupamento, busca e formatação (funções puras) ---------- */

export type GroupBy = 'date' | 'client' | 'category';

export const GROUP_BY_LABELS: Record<GroupBy, string> = {
  date: 'Data',
  client: 'Cliente',
  category: 'Categoria',
};

export interface MeetingGroup {
  key: string;
  label: string;
  meetings: MeetingOrg[];
}

function parseDate(value: string): Date | null {
  if (!value) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function byNewest(a: MeetingOrg, b: MeetingOrg): number {
  const left = parseDate(a.createdAt)?.getTime() ?? 0;
  const right = parseDate(b.createdAt)?.getTime() ?? 0;
  return right - left;
}

const DAY_MS = 24 * 60 * 60 * 1000;

function groupByDate(meetings: MeetingOrg[], now: Date): MeetingGroup[] {
  const today = startOfDay(now);
  const groups = new Map<string, MeetingGroup>();
  const order: string[] = [];
  const push = (key: string, label: string, meeting: MeetingOrg) => {
    let group = groups.get(key);
    if (!group) {
      group = { key, label, meetings: [] };
      groups.set(key, group);
      order.push(key);
    }
    group.meetings.push(meeting);
  };

  const pinned = meetings.filter((meeting) => meeting.pinned);
  const rest = meetings.filter((meeting) => !meeting.pinned);
  const dated = rest.filter((meeting) => parseDate(meeting.createdAt)).sort(byNewest);
  const undated = rest.filter((meeting) => !parseDate(meeting.createdAt));

  pinned.sort(byNewest).forEach((meeting) => push('pinned', 'Fixadas', meeting));
  for (const meeting of dated) {
    const date = parseDate(meeting.createdAt)!;
    const age = Math.floor((today - startOfDay(date)) / DAY_MS);
    if (age <= 0) push('today', 'Hoje', meeting);
    else if (age === 1) push('yesterday', 'Ontem', meeting);
    else if (age <= 7) push('last7', 'Últimos 7 dias', meeting);
    else if (age <= 30) push('last30', 'Últimos 30 dias', meeting);
    else {
      const key = `month-${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}`;
      const label = capitalize(date.toLocaleDateString('pt-BR', {
        month: 'long',
        ...(date.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }),
      }));
      push(key, label, meeting);
    }
  }
  // Modo simples (sem datas): mantém a ordem recebida num único grupo.
  undated.forEach((meeting) => push('all', dated.length > 0 || pinned.length > 0 ? 'Sem data' : 'Reuniões', meeting));

  return order.map((key) => groups.get(key)!);
}

function groupByField(meetings: MeetingOrg[], field: 'client' | 'category', emptyLabel: string): MeetingGroup[] {
  const groups = new Map<string, MeetingGroup>();
  for (const meeting of [...meetings].sort(byNewest)) {
    const value = meeting[field];
    const key = value ? `v:${value.toLocaleLowerCase('pt-BR')}` : 'none';
    let group = groups.get(key);
    if (!group) {
      group = { key, label: value ?? emptyLabel, meetings: [] };
      groups.set(key, group);
    }
    group.meetings.push(meeting);
  }
  return [...groups.values()].sort((a, b) => {
    if (a.key === 'none') return 1;
    if (b.key === 'none') return -1;
    return a.label.localeCompare(b.label, 'pt-BR');
  });
}

export function groupMeetings(meetings: MeetingOrg[], groupBy: GroupBy, now: Date = new Date()): MeetingGroup[] {
  if (groupBy === 'client') return groupByField(meetings, 'client', 'Sem cliente');
  if (groupBy === 'category') return groupByField(meetings, 'category', 'Sem categoria');
  return groupByDate(meetings, now);
}

function fold(text: string): string {
  return text.normalize('NFD').replace(/[̀-ͯ]/g, '').toLocaleLowerCase('pt-BR');
}

/** Busca por título, cliente, projeto, categoria e etiquetas (todas as palavras precisam aparecer). */
export function meetingMatchesQuery(meeting: MeetingOrg, query: string): boolean {
  const terms = fold(query).split(/\s+/).filter(Boolean);
  if (terms.length === 0) return true;
  const haystack = fold([
    meeting.title,
    meeting.client ?? '',
    meeting.project ?? '',
    meeting.category ?? '',
    ...meeting.tags,
  ].join(' \u0001 '));
  return terms.every((term) => haystack.includes(term));
}

/** "14:30" para hoje, "ontem", "12 mai." no ano corrente, "12/05/2024" antes disso. */
export function formatMeetingWhen(createdAt: string, now: Date = new Date()): string {
  const date = parseDate(createdAt);
  if (!date) return '';
  const age = Math.floor((startOfDay(now) - startOfDay(date)) / DAY_MS);
  const time = date.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' });
  if (age <= 0) return time;
  if (age === 1) return `ontem, ${time}`;
  if (date.getFullYear() === now.getFullYear()) {
    return `${date.toLocaleDateString('pt-BR', { day: 'numeric', month: 'short' })}, ${time}`;
  }
  return date.toLocaleDateString('pt-BR');
}

export function formatDuration(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds <= 0) return '';
  const minutes = Math.round(seconds / 60);
  if (minutes < 1) return 'menos de 1 min';
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`;
}

/** Segunda linha da conversa: cliente · categoria · data (omite o que faltar). */
export function meetingSubtitle(meeting: MeetingOrg, now: Date = new Date()): string {
  return [meeting.client, meeting.category, formatMeetingWhen(meeting.createdAt, now)]
    .filter((part): part is string => Boolean(part))
    .join(' · ');
}

/** Prévia do modelo de título com dados de exemplo. */
export function previewTitleTemplate(
  template: string,
  sample: { cliente: string; projeto: string; assunto: string; data: string; hora: string },
): string {
  const filled = template.replace(/\{(cliente|projeto|assunto|data|hora)\}/g, (_, key: keyof typeof sample) => sample[key]);
  return filled.replace(/\s+/g, ' ').replace(/^[\s·\-–—|:,]+|[\s·\-–—|:,]+$/g, '').trim();
}
