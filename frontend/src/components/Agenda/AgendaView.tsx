'use client';

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { CalendarDays, ChevronLeft, ChevronRight, Loader2, Search, X } from 'lucide-react';
import { TopBar } from '@/components/Shell/TopBar';
import { Button } from '@/components/ui/button';
import { IconButton } from '@/components/ui/icon-button';
import { Input } from '@/components/ui/input';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import type { MeetingOrg } from '@/lib/organize';
import {
  groupByDay,
  hourRange,
  matchesAgendaQuery,
  placeInLanes,
  toAgendaItems,
  type AgendaItem,
} from '@/lib/agenda';
import {
  addDays,
  formatDuration,
  fromDayKey,
  onWorkspaceEvents,
  participantsIndex,
  startOfWeek,
  toDayKey,
  weekDays,
} from '@/lib/workspace';
import { cn } from '@/lib/utils';

type AgendaMode = 'day' | 'week' | 'timeline';

const MODES: Array<{ value: AgendaMode; label: string }> = [
  { value: 'day', label: 'Dia' },
  { value: 'week', label: 'Semana' },
  { value: 'timeline', label: 'Linha do tempo' },
];

const MODE_KEY = 'meetily.agenda.mode';
const HOUR_HEIGHT = 52;

interface TranscriptHit {
  id: string;
  matchContext?: string;
}

function timeOf(date: Date): string {
  return date.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' });
}

function longDay(date: Date): string {
  const text = date.toLocaleDateString('pt-BR', { weekday: 'long', day: 'numeric', month: 'long' });
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function rangeLabel(mode: AgendaMode, cursor: Date): string {
  if (mode === 'day') return longDay(cursor);
  const days = weekDays(cursor);
  const first = days[0];
  const last = days[6];
  const sameMonth = first.getMonth() === last.getMonth();
  const start = first.toLocaleDateString('pt-BR', sameMonth ? { day: 'numeric' } : { day: 'numeric', month: 'short' });
  const end = last.toLocaleDateString('pt-BR', { day: 'numeric', month: 'short', year: 'numeric' });
  return `${start} – ${end}`;
}

/* ---------- Grade de horas (dia e semana) ---------- */

function MeetingBlock({ item, onOpen, short }: { item: AgendaItem; onOpen: (meeting: MeetingOrg) => void; short: boolean }) {
  const { meeting } = item;
  const detail = [timeOf(item.start), formatDuration(meeting.durationSeconds), meeting.client].filter(Boolean).join(' · ');
  return (
    <button
      type="button"
      onClick={() => onOpen(meeting)}
      title={`${meeting.title}\n${detail}`}
      className={cn(
        'flex h-full w-full overflow-hidden rounded-md border border-primary/25 bg-primary/10 px-1.5 text-left transition-colors hover:bg-primary/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
        short ? 'items-center gap-1.5' : 'flex-col py-1',
      )}
    >
      {short ? (
        <>
          <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground">{timeOf(item.start)}</span>
          <span className="truncate text-[11px] font-medium">{meeting.title}</span>
        </>
      ) : (
        <>
          <span className="truncate text-xs font-medium leading-tight">{meeting.title}</span>
          <span className="truncate text-[11px] leading-tight text-muted-foreground">{detail}</span>
        </>
      )}
    </button>
  );
}

function HourGrid({ days, byDay, onOpen, onPickDay }: {
  days: Date[];
  byDay: Map<string, AgendaItem[]>;
  onOpen: (meeting: MeetingOrg) => void;
  onPickDay?: (day: Date) => void;
}) {
  const todayKey = toDayKey(new Date());
  const visible = days.flatMap((day) => byDay.get(toDayKey(day)) ?? []);
  const { from, to } = hourRange(visible);
  const hours = Array.from({ length: to - from }, (_, index) => from + index);
  const nowRef = useRef<HTMLDivElement>(null);
  const now = new Date();
  const nowHour = now.getHours() + now.getMinutes() / 60;

  useEffect(() => {
    nowRef.current?.scrollIntoView({ block: 'center' });
  }, [days.length]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {days.length > 1 && (
        <div className="flex shrink-0 border-b pl-12">
          {days.map((day) => {
            const key = toDayKey(day);
            const count = byDay.get(key)?.length ?? 0;
            return (
              <button
                key={key}
                type="button"
                onClick={() => onPickDay?.(day)}
                className="flex min-w-0 flex-1 flex-col items-center border-l py-1.5 text-xs transition-colors hover:bg-accent/60"
              >
                <span className="text-muted-foreground">{day.toLocaleDateString('pt-BR', { weekday: 'short' }).replace('.', '')}</span>
                <span
                  className={cn(
                    'mt-0.5 flex h-6 min-w-6 items-center justify-center rounded-full px-1 text-sm font-medium',
                    key === todayKey && 'bg-primary text-primary-foreground',
                  )}
                >
                  {day.getDate()}
                </span>
                <span className="sr-only">{count === 1 ? '1 reunião' : `${count} reuniões`}</span>
              </button>
            );
          })}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto custom-scrollbar">
        <div className="relative flex" style={{ height: hours.length * HOUR_HEIGHT }}>
          <div className="w-12 shrink-0">
            {hours.map((hour, index) => (
              <div key={hour} style={{ height: HOUR_HEIGHT }} className="pr-2 text-right text-[11px] text-muted-foreground">
                <span className="relative -top-2 first:top-0">{index === 0 ? '' : `${String(hour).padStart(2, '0')}:00`}</span>
              </div>
            ))}
          </div>
          {days.map((day) => {
            const key = toDayKey(day);
            const placed = placeInLanes(byDay.get(key) ?? [], 26 / HOUR_HEIGHT);
            const showNow = key === todayKey && nowHour >= from && nowHour <= to;
            return (
              <div key={key} className="relative min-w-0 flex-1 border-l">
                {hours.map((hour) => (
                  <div key={hour} style={{ height: HOUR_HEIGHT }} className="border-b border-border/60" />
                ))}
                {showNow && (
                  <div
                    ref={nowRef}
                    aria-hidden
                    className="pointer-events-none absolute inset-x-0 z-10 border-t-2 border-recording"
                    style={{ top: (nowHour - from) * HOUR_HEIGHT }}
                  />
                )}
                {placed.map((item) => (
                  <div
                    key={item.meeting.id}
                    className="absolute p-px"
                    style={{
                      top: (item.startHour - from) * HOUR_HEIGHT,
                      height: Math.max(26, item.hours * HOUR_HEIGHT),
                      left: `${(item.lane / item.lanes) * 100}%`,
                      width: `${100 / item.lanes}%`,
                    }}
                  >
                    <MeetingBlock item={item} onOpen={onOpen} short={item.hours * HOUR_HEIGHT < 40} />
                  </div>
                ))}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

/* ---------- Lista (linha do tempo e resultados da busca) ---------- */

function MeetingLine({ item, people, snippet, onOpen }: {
  item: AgendaItem;
  people: string[];
  snippet?: string;
  onOpen: (meeting: MeetingOrg) => void;
}) {
  const { meeting } = item;
  const meta = [formatDuration(meeting.durationSeconds), meeting.client, meeting.project, meeting.category].filter(Boolean);
  return (
    <li className="relative pl-6">
      <span aria-hidden className="absolute left-[5px] top-3.5 h-2 w-2 rounded-full bg-primary ring-4 ring-background" />
      <button
        type="button"
        onClick={() => onOpen(meeting)}
        className="w-full rounded-lg px-3 py-2 text-left transition-colors hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <div className="flex items-baseline gap-2">
          <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{timeOf(item.start)}</span>
          <span className="min-w-0 truncate text-sm font-medium">{meeting.title}</span>
        </div>
        {meta.length > 0 && <p className="mt-0.5 truncate text-xs text-muted-foreground">{meta.join(' · ')}</p>}
        {people.length > 0 && (
          <p className="mt-0.5 truncate text-xs text-muted-foreground">Com {people.slice(0, 6).join(', ')}{people.length > 6 ? ` e mais ${people.length - 6}` : ''}</p>
        )}
        {snippet && <p className="mt-1 line-clamp-2 text-xs italic text-muted-foreground">“{snippet.trim()}”</p>}
      </button>
    </li>
  );
}

function DayList({ items, people, snippets, onOpen, empty }: {
  items: AgendaItem[];
  people: Record<string, string[]>;
  snippets?: Map<string, string | undefined>;
  onOpen: (meeting: MeetingOrg) => void;
  empty: string;
}) {
  const days = useMemo(() => {
    const grouped = groupByDay(items);
    return [...grouped.entries()].sort((a, b) => (a[0] < b[0] ? 1 : -1));
  }, [items]);

  if (days.length === 0) {
    return <p className="px-6 py-12 text-center text-sm text-muted-foreground">{empty}</p>;
  }

  const todayKey = toDayKey(new Date());
  return (
    <div className="min-h-0 flex-1 overflow-y-auto custom-scrollbar">
      <div className="mx-auto w-full max-w-[760px] px-6 py-5">
        {days.map(([key, list]) => (
          <section key={key} className="mb-6 last:mb-0">
            <h2 className="mb-1 flex items-baseline gap-2 text-sm font-medium">
              {key === todayKey ? 'Hoje' : longDay(fromDayKey(key))}
              <span className="text-xs font-normal text-muted-foreground">
                {fromDayKey(key).getFullYear() !== new Date().getFullYear() ? `${fromDayKey(key).getFullYear()} · ` : ''}
                {list.length === 1 ? '1 reunião' : `${list.length} reuniões`}
              </span>
            </h2>
            <ul className="relative space-y-0.5 before:absolute before:bottom-3 before:left-[8px] before:top-3 before:w-px before:bg-border">
              {[...list].reverse().map((item) => (
                <MeetingLine
                  key={item.meeting.id}
                  item={item}
                  people={people[item.meeting.id] ?? []}
                  snippet={snippets?.get(item.meeting.id)}
                  onOpen={onOpen}
                />
              ))}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}

/* ---------- Tela ---------- */

export function AgendaView() {
  const router = useRouter();
  const { organizedMeetings, setCurrentMeeting, isLoadingMeetings } = useSidebar();
  const [mode, setMode] = useState<AgendaMode>('week');
  const [cursor, setCursor] = useState(() => new Date());
  const [query, setQuery] = useState('');
  const [people, setPeople] = useState<Record<string, string[]>>({});
  const [hits, setHits] = useState<TranscriptHit[]>([]);
  const [searching, setSearching] = useState(false);
  const searchSeq = useRef(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    try {
      const stored = localStorage.getItem(MODE_KEY);
      if (stored && MODES.some((item) => item.value === stored)) setMode(stored as AgendaMode);
    } catch {
      // Preferência opcional.
    }
  }, []);

  const changeMode = (next: AgendaMode) => {
    setMode(next);
    try {
      localStorage.setItem(MODE_KEY, next);
    } catch {
      // Preferência opcional.
    }
  };

  useEffect(() => {
    const load = () => participantsIndex().then(setPeople);
    load();
    return onWorkspaceEvents(['participants-updated'], load);
  }, []);

  // Busca dentro das transcrições, com atraso para não disparar a cada tecla.
  const trimmed = query.trim();
  useEffect(() => {
    const id = ++searchSeq.current;
    if (trimmed.length < 2) {
      setHits([]);
      setSearching(false);
      return;
    }
    setSearching(true);
    const timer = setTimeout(async () => {
      try {
        const results = await invoke<TranscriptHit[]>('api_search_transcripts', { query: trimmed });
        if (id === searchSeq.current) setHits(results ?? []);
      } catch {
        if (id === searchSeq.current) setHits([]);
      } finally {
        if (id === searchSeq.current) setSearching(false);
      }
    }, 250);
    return () => clearTimeout(timer);
  }, [trimmed]);

  // "/" leva o foco para a busca.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable);
      if (event.key === '/' && !typing) {
        event.preventDefault();
        inputRef.current?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const items = useMemo(() => toAgendaItems(organizedMeetings), [organizedMeetings]);
  const byDay = useMemo(() => groupByDay(items), [items]);

  const snippets = useMemo(() => new Map(hits.map((hit) => [hit.id, hit.matchContext])), [hits]);
  const results = useMemo(() => {
    if (!trimmed) return [];
    return items.filter((item) => snippets.has(item.meeting.id) || matchesAgendaQuery(item.meeting, people[item.meeting.id] ?? [], trimmed));
  }, [items, people, snippets, trimmed]);

  const openMeeting = useCallback((meeting: MeetingOrg) => {
    setCurrentMeeting({ id: meeting.id, title: meeting.title });
    router.push(`/meeting-details?id=${meeting.id}`);
  }, [router, setCurrentMeeting]);

  const step = mode === 'day' ? 1 : 7;
  const days = mode === 'day' ? [cursor] : weekDays(cursor);
  const isCurrent = mode === 'day'
    ? toDayKey(cursor) === toDayKey(new Date())
    : toDayKey(startOfWeek(cursor)) === toDayKey(startOfWeek(new Date()));
  const periodCount = days.reduce((total, day) => total + (byDay.get(toDayKey(day))?.length ?? 0), 0);

  return (
    <div className="flex h-full min-w-0 flex-col bg-background">
      <TopBar
        title={<span className="block truncate px-2">Agenda</span>}
        actions={
          <div role="tablist" aria-label="Modo de visualização" className="flex rounded-lg bg-muted p-0.5">
            {MODES.map((item) => (
              <button
                key={item.value}
                type="button"
                role="tab"
                aria-selected={mode === item.value}
                onClick={() => changeMode(item.value)}
                className={cn(
                  'h-7 rounded-md px-2.5 text-xs transition-colors',
                  mode === item.value ? 'bg-background font-medium text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground',
                )}
              >
                {item.label}
              </button>
            ))}
          </div>
        }
      />

      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b px-4 py-2">
        <div className="relative min-w-[220px] flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => event.key === 'Escape' && setQuery('')}
            placeholder="Buscar por título, cliente, pessoa ou algo que foi dito"
            aria-label="Buscar reuniões"
            className="pl-8 pr-8"
          />
          {searching ? (
            <Loader2 className="absolute right-2.5 top-1/2 h-4 w-4 -translate-y-1/2 animate-spin text-muted-foreground" />
          ) : query ? (
            <button
              type="button"
              aria-label="Limpar busca"
              onClick={() => setQuery('')}
              className="absolute right-1.5 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          ) : null}
        </div>
        {!trimmed && mode !== 'timeline' && (
          <div className="flex items-center gap-1">
            <Button variant="outline" size="sm" disabled={isCurrent} onClick={() => setCursor(new Date())}>Hoje</Button>
            <IconButton label={mode === 'day' ? 'Dia anterior' : 'Semana anterior'} onClick={() => setCursor(addDays(cursor, -step))}>
              <ChevronLeft />
            </IconButton>
            <IconButton label={mode === 'day' ? 'Próximo dia' : 'Próxima semana'} onClick={() => setCursor(addDays(cursor, step))}>
              <ChevronRight />
            </IconButton>
            <span className="ml-1 text-sm font-medium" aria-live="polite">{rangeLabel(mode, cursor)}</span>
            <span className="ml-2 text-xs text-muted-foreground">
              {periodCount === 0 ? 'sem reuniões' : periodCount === 1 ? '1 reunião' : `${periodCount} reuniões`}
            </span>
          </div>
        )}
        {trimmed && (
          <span className="text-xs text-muted-foreground" aria-live="polite">
            {results.length === 1 ? '1 reunião encontrada' : `${results.length} reuniões encontradas`}
          </span>
        )}
      </div>

      {isLoadingMeetings && items.length === 0 ? (
        <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          <Loader2 className="mr-2 h-4 w-4 animate-spin" /> Carregando reuniões…
        </div>
      ) : items.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
          <CalendarDays className="h-8 w-8 text-muted-foreground" />
          <p className="text-sm font-medium">Nenhuma reunião gravada ainda</p>
          <p className="max-w-sm text-[13px] text-muted-foreground">As reuniões aparecem aqui, no dia e na hora em que foram gravadas.</p>
        </div>
      ) : trimmed ? (
        <DayList
          items={results}
          people={people}
          snippets={snippets}
          onOpen={openMeeting}
          empty={searching ? 'Buscando…' : `Nada encontrado para “${trimmed}”.`}
        />
      ) : mode === 'timeline' ? (
        <DayList items={items} people={people} onOpen={openMeeting} empty="Nenhuma reunião." />
      ) : (
        <HourGrid
          days={days}
          byDay={byDay}
          onOpen={openMeeting}
          onPickDay={(day) => {
            setCursor(day);
            changeMode('day');
          }}
        />
      )}
    </div>
  );
}
