/**
 * Regras puras da Agenda: em que dia cada reunião cai, como blocos que se
 * sobrepõem dividem a coluna e como a busca filtra.
 */
import type { MeetingOrg } from '@/lib/organize';
import { dayKeyOf } from '@/lib/workspace';

export interface AgendaItem {
  meeting: MeetingOrg;
  start: Date;
  dayKey: string;
  /** Horas desde a meia-noite local. */
  startHour: number;
  /** Duração em horas (mínimo visual aplicado na tela). */
  hours: number;
}

export interface PlacedItem extends AgendaItem {
  lane: number;
  lanes: number;
}

const DEFAULT_HOURS = 0.5;

export function toAgendaItems(meetings: MeetingOrg[]): AgendaItem[] {
  const items: AgendaItem[] = [];
  for (const meeting of meetings) {
    const dayKey = dayKeyOf(meeting.createdAt);
    if (!dayKey) continue;
    const start = new Date(meeting.createdAt);
    const startHour = start.getHours() + start.getMinutes() / 60;
    const hours = meeting.durationSeconds && meeting.durationSeconds > 0 ? meeting.durationSeconds / 3600 : DEFAULT_HOURS;
    // Um bloco não atravessa a meia-noite na grade.
    items.push({ meeting, start, dayKey, startHour, hours: Math.min(hours, 24 - startHour) });
  }
  return items.sort((a, b) => a.start.getTime() - b.start.getTime());
}

export function groupByDay(items: AgendaItem[]): Map<string, AgendaItem[]> {
  const days = new Map<string, AgendaItem[]>();
  for (const item of items) {
    const list = days.get(item.dayKey);
    if (list) list.push(item);
    else days.set(item.dayKey, [item]);
  }
  return days;
}

/** Distribui reuniões sobrepostas do mesmo dia em faixas lado a lado. */
export function placeInLanes(items: AgendaItem[], minHours = 0.5): PlacedItem[] {
  const sorted = [...items].sort((a, b) => a.startHour - b.startHour);
  const placed: PlacedItem[] = [];
  let cluster: PlacedItem[] = [];
  let clusterEnd = -1;
  let laneEnds: number[] = [];

  const closeCluster = () => {
    cluster.forEach((item) => {
      item.lanes = laneEnds.length;
    });
    placed.push(...cluster);
    cluster = [];
    laneEnds = [];
  };

  for (const item of sorted) {
    const end = item.startHour + Math.max(item.hours, minHours);
    if (cluster.length > 0 && item.startHour >= clusterEnd) closeCluster();
    let lane = laneEnds.findIndex((laneEnd) => laneEnd <= item.startHour);
    if (lane === -1) {
      lane = laneEnds.length;
      laneEnds.push(end);
    } else {
      laneEnds[lane] = end;
    }
    cluster.push({ ...item, lane, lanes: 1 });
    clusterEnd = Math.max(clusterEnd, end);
  }
  if (cluster.length > 0) closeCluster();
  return placed;
}

/** Faixa de horas a mostrar: 8h–19h, esticada para caber tudo. */
export function hourRange(items: AgendaItem[]): { from: number; to: number } {
  let from = 8;
  let to = 19;
  for (const item of items) {
    from = Math.min(from, Math.floor(item.startHour));
    to = Math.max(to, Math.ceil(item.startHour + item.hours));
  }
  return { from: Math.max(0, from), to: Math.min(24, to) };
}

export function foldText(text: string): string {
  return text
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLocaleLowerCase('pt-BR');
}

/** Todas as palavras da busca precisam aparecer em algum campo da reunião. */
export function matchesAgendaQuery(meeting: MeetingOrg, people: string[], query: string): boolean {
  const terms = foldText(query).split(/\s+/).filter(Boolean);
  if (terms.length === 0) return true;
  const haystack = foldText(
    [meeting.title, meeting.client ?? '', meeting.project ?? '', meeting.category ?? '', ...meeting.tags, ...people].join(' \u0001 '),
  );
  return terms.every((term) => haystack.includes(term));
}
