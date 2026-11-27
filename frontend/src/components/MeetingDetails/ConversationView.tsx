'use client';

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Loader2, MessagesSquare, Search } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { foldText } from '@/lib/agenda';
import { errorText } from '@/lib/tauriSafe';
import {
  buildTurns,
  formatClock,
  getWorkspaceSettings,
  listParticipants,
  nameColor,
  onWorkspaceEvents,
  renameSpeaker,
  type ConversationSegment,
  type ConversationTurn,
  type Participant,
} from '@/lib/workspace';
import { cn } from '@/lib/utils';

interface RawTranscript {
  id: string;
  text: string;
  audio_start_time?: number | null;
  speaker?: string | null;
  speaker_key?: string | null;
}

interface Page {
  transcripts: RawTranscript[];
  has_more: boolean;
}

const PAGE_SIZE = 500;

async function loadAllSegments(meetingId: string): Promise<ConversationSegment[]> {
  const all: RawTranscript[] = [];
  for (let offset = 0; offset < 200_000; ) {
    const page = await invoke<Page>('api_get_meeting_transcripts', { meetingId, limit: PAGE_SIZE, offset });
    all.push(...page.transcripts);
    offset += page.transcripts.length;
    if (!page.has_more || page.transcripts.length === 0) break;
  }
  return all
    .map((item) => ({
      id: item.id,
      timestamp: item.audio_start_time ?? 0,
      text: item.text,
      speaker: item.speaker,
      speakerKey: item.speaker_key,
    }))
    .sort((a, b) => a.timestamp - b.timestamp);
}

/** Quem é esta voz: escolhe um participante ou digita um nome. */
function SpeakerPicker({ turn, participants, onRename }: {
  turn: ConversationTurn;
  participants: Participant[];
  onRename: (speakerKey: string, name: string) => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');

  const apply = async (value: string) => {
    const trimmed = value.trim();
    if (!trimmed) return;
    setOpen(false);
    setName('');
    await onRename(turn.speakerKey, trimmed);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          title="Dizer quem é esta voz"
          className={cn('text-xs font-semibold underline-offset-2 hover:underline focus-visible:underline focus-visible:outline-none', nameColor(turn.speaker))}
        >
          {turn.speaker}
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-64 p-3">
        <p className="text-xs text-muted-foreground">Quem é esta voz? Vale para a reunião inteira.</p>
        {participants.length > 0 && (
          <ul className="mt-2 max-h-40 space-y-0.5 overflow-y-auto">
            {participants.map((person) => (
              <li key={person.id}>
                <button
                  type="button"
                  onClick={() => apply(person.name)}
                  className="w-full truncate rounded-md px-2 py-1 text-left text-sm hover:bg-accent"
                >
                  {person.name}
                </button>
              </li>
            ))}
          </ul>
        )}
        <form
          className="mt-2 flex gap-1.5"
          onSubmit={(event) => {
            event.preventDefault();
            void apply(name);
          }}
        >
          <Input className="h-8" placeholder="Outro nome" value={name} onChange={(event) => setName(event.target.value)} aria-label="Nome da pessoa" />
          <Button type="submit" size="sm" disabled={!name.trim()}>OK</Button>
        </form>
      </PopoverContent>
    </Popover>
  );
}

function Bubble({ turn, participants, onRename }: {
  turn: ConversationTurn;
  participants: Participant[];
  onRename: (speakerKey: string, name: string) => Promise<void>;
}) {
  return (
    <li className={cn('flex', turn.mine ? 'justify-end' : 'justify-start')}>
      <div
        className={cn(
          'max-w-[85%] rounded-2xl px-3 py-2 shadow-sm',
          turn.mine ? 'rounded-br-md bg-primary/15' : 'rounded-bl-md border bg-card',
        )}
      >
        {turn.mine ? (
          <p className="text-xs font-semibold text-primary">{turn.speaker}</p>
        ) : (
          <SpeakerPicker turn={turn} participants={participants} onRename={onRename} />
        )}
        <div className="mt-0.5 space-y-1 text-sm leading-relaxed">
          {turn.lines.map((line, index) => (
            <p key={index}>{line}</p>
          ))}
        </div>
        <p className="mt-1 text-right text-[10px] tabular-nums text-muted-foreground">{formatClock(turn.start)}</p>
      </div>
    </li>
  );
}

/** A reunião como uma conversa de mensagens: um balão por fala, com o nome de quem falou. */
export function ConversationView({ meetingId, refreshKey }: { meetingId: string; refreshKey?: number }) {
  const [segments, setSegments] = useState<ConversationSegment[] | null>(null);
  const [participants, setParticipants] = useState<Participant[]>([]);
  const [myName, setMyName] = useState('Você');
  const [query, setQuery] = useState('');

  const reload = useCallback(async () => {
    try {
      setSegments(await loadAllSegments(meetingId));
    } catch (error) {
      console.error('Falha ao carregar a conversa', error);
      setSegments([]);
    }
  }, [meetingId]);

  useEffect(() => {
    setSegments(null);
    void reload();
  }, [reload, refreshKey]);

  useEffect(() => {
    const load = () => listParticipants(meetingId).then(setParticipants);
    load();
    getWorkspaceSettings().then((settings) => setMyName(settings.userName.trim() || 'Você'));
    return onWorkspaceEvents(['participants-updated'], load);
  }, [meetingId]);

  const rename = useCallback(async (speakerKey: string, name: string) => {
    try {
      await renameSpeaker(meetingId, speakerKey, name);
      await reload();
    } catch (error) {
      toast.error('Não foi possível renomear', { description: errorText(error) });
    }
  }, [meetingId, reload]);

  const turns = useMemo(() => buildTurns(segments ?? [], myName), [segments, myName]);
  const shown = useMemo(() => {
    const terms = foldText(query).split(/\s+/).filter(Boolean);
    if (terms.length === 0) return turns;
    return turns.filter((turn) => {
      const haystack = foldText(`${turn.speaker} ${turn.lines.join(' ')}`);
      return terms.every((term) => haystack.includes(term));
    });
  }, [turns, query]);

  if (segments === null) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        <Loader2 className="mr-2 h-4 w-4 animate-spin" /> Montando a conversa…
      </div>
    );
  }

  if (turns.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
        <MessagesSquare className="h-8 w-8 text-muted-foreground" />
        <p className="text-sm font-medium">Ainda não há falas nesta reunião</p>
      </div>
    );
  }

  const speakers = new Set(turns.map((turn) => turn.speaker)).size;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b px-3 py-2">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="h-8 pl-8"
            placeholder="Buscar na conversa"
            aria-label="Buscar na conversa"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <span className="shrink-0 text-xs text-muted-foreground">
          {query ? `${shown.length} de ${turns.length}` : `${speakers === 1 ? '1 pessoa' : `${speakers} pessoas`} · ${turns.length} falas`}
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto bg-muted/30 custom-scrollbar">
        {shown.length === 0 ? (
          <p className="px-6 py-10 text-center text-sm text-muted-foreground">Nada encontrado para “{query.trim()}”.</p>
        ) : (
          <ul className="space-y-2 px-3 py-4" style={{ contentVisibility: 'auto' }}>
            {shown.map((turn) => (
              <Bubble key={turn.id} turn={turn} participants={participants} onRename={rename} />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
