'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { CalendarDays, FileUp, Loader2, Sparkles, UserPlus, X } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { QuickAddTask, TaskList, DIRECTION_LABELS } from '@/components/Tasks/TaskList';
import { errorText } from '@/lib/tauriSafe';
import {
  addParticipant,
  generateTasks,
  importParticipantsFile,
  listParticipants,
  listTasks,
  onWorkspaceEvents,
  participantsFromCalendar,
  removeParticipant,
  renameSpeaker,
  type ImportResult,
  type Participant,
  type SpeakerMatch,
  type Task,
  type TaskDirection,
} from '@/lib/workspace';

const SOURCE_LABELS: Record<string, string> = {
  calendar: 'Calendário ou convite',
  manual: 'Adicionado por você',
  teams: 'Arquivo do Teams',
};

const IMPORT_LABELS: Record<string, string> = {
  vtt: 'transcrição do Teams',
  attendance: 'lista de presença',
  ics: 'convite',
};

const ORIGIN_MESSAGES: Record<string, string> = {
  model: 'Tarefas extraídas pelo modelo de resumo.',
  autopilot: 'Tarefas aproveitadas do piloto automático.',
  rules: 'Tarefas tiradas do resumo, sem usar modelo.',
  empty: 'Nenhuma tarefa encontrada nesta reunião.',
};

function Section({ title, action, children }: { title: string; action?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="rounded-xl border bg-card p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="whitespace-nowrap text-sm font-medium">{title}</h2>
        {action}
      </div>
      <div className="mt-3">{children}</div>
    </section>
  );
}

function ParticipantsSection({ meetingId, onSpeakersChanged }: { meetingId: string; onSpeakersChanged?: () => void }) {
  const [people, setPeople] = useState<Participant[]>([]);
  const [name, setName] = useState('');
  const [busy, setBusy] = useState<'calendar' | 'file' | null>(null);
  const [matches, setMatches] = useState<SpeakerMatch[]>([]);

  const reload = useCallback(() => {
    listParticipants(meetingId).then(setPeople);
  }, [meetingId]);

  useEffect(() => {
    setMatches([]);
    reload();
    return onWorkspaceEvents(['participants-updated'], reload);
  }, [reload]);

  const fromCalendar = async () => {
    setBusy('calendar');
    try {
      const before = people.length;
      const next = await participantsFromCalendar(meetingId);
      setPeople(next);
      if (next.length > before) toast.success(`${next.length - before} participante(s) do Calendário`);
      else toast.info('Nenhum evento com convidados nesse horário', { description: 'O app procura no Calendário do macOS um evento que cubra o horário da gravação.' });
    } catch (error) {
      toast.error('Não foi possível ler o Calendário', { description: errorText(error) });
    } finally {
      setBusy(null);
    }
  };

  const fromFile = async () => {
    setBusy('file');
    try {
      const result: ImportResult | null = await importParticipantsFile(meetingId);
      if (!result) return;
      setPeople(result.participants);
      setMatches(result.matches);
      toast.success(`Lido: ${IMPORT_LABELS[result.kind] ?? 'arquivo'}`, {
        description: result.matches.length > 0
          ? 'Confira abaixo as vozes reconhecidas e aplique os nomes.'
          : result.added > 0 ? `${result.added} participante(s) novo(s).` : 'Ninguém novo na lista.',
      });
    } catch (error) {
      toast.error('Não foi possível ler o arquivo', { description: errorText(error) });
    } finally {
      setBusy(null);
    }
  };

  const applyMatches = async () => {
    try {
      for (const match of matches) await renameSpeaker(meetingId, match.speakerKey, match.name);
      toast.success('Nomes aplicados às vozes');
      setMatches([]);
      onSpeakersChanged?.();
    } catch (error) {
      toast.error('Não foi possível aplicar os nomes', { description: errorText(error) });
    }
  };

  return (
    <Section title={people.length > 0 ? `Participantes (${people.length})` : 'Participantes'}>
      {people.length === 0 ? (
        <p className="text-[13px] text-muted-foreground">
          Ninguém listado. Busque no Calendário, importe a transcrição (.vtt) ou a lista de presença (.csv) que o Teams gera, ou adicione à mão.
        </p>
      ) : (
        <ul className="flex flex-wrap gap-1.5">
          {people.map((person) => (
            <li
              key={person.id}
              title={[person.email, person.organization, SOURCE_LABELS[person.source]].filter(Boolean).join(' · ')}
              className="inline-flex items-center gap-1 rounded-md bg-muted py-0.5 pl-2 pr-1 text-[13px]"
            >
              {person.name}
              {person.organization && <span className="text-xs text-muted-foreground">· {person.organization}</span>}
              <button
                type="button"
                aria-label={`Remover ${person.name}`}
                onClick={async () => {
                  await removeParticipant(person.id).catch(() => undefined);
                  reload();
                }}
                className="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
              >
                <X className="h-3 w-3" />
              </button>
            </li>
          ))}
        </ul>
      )}

      {matches.length > 0 && (
        <div className="mt-3 rounded-lg border border-primary/30 bg-primary/5 p-3">
          <p className="text-[13px] font-medium">Vozes reconhecidas pela transcrição do Teams</p>
          <ul className="mt-1.5 space-y-0.5 text-[13px] text-muted-foreground">
            {matches.map((match) => (
              <li key={match.speakerKey}>
                <span className="text-foreground">{match.name}</span> · {match.segments} falas, {Math.round(match.share * 100)}% de coincidência
              </li>
            ))}
          </ul>
          <div className="mt-2 flex gap-2">
            <Button size="sm" onClick={applyMatches}>Aplicar nomes</Button>
            <Button size="sm" variant="ghost" onClick={() => setMatches([])}>Ignorar</Button>
          </div>
        </div>
      )}

      <div className="mt-3 flex flex-wrap gap-2">
        <Button size="sm" variant="outline" onClick={fromCalendar} disabled={busy !== null}>
          {busy === 'calendar' ? <Loader2 className="animate-spin" /> : <CalendarDays />} Buscar no Calendário
        </Button>
        <Button size="sm" variant="outline" onClick={fromFile} disabled={busy !== null} title="Transcrição .vtt, lista de presença .csv ou convite .ics">
          {busy === 'file' ? <Loader2 className="animate-spin" /> : <FileUp />} Importar do Teams
        </Button>
      </div>
      <form
        className="mt-2 flex flex-wrap gap-2"
        onSubmit={async (event) => {
          event.preventDefault();
          if (!name.trim()) return;
          try {
            setPeople(await addParticipant(meetingId, name));
            setName('');
          } catch (error) {
            toast.error('Não foi possível adicionar', { description: errorText(error) });
          }
        }}
      >
        <Input className="h-8 min-w-[9rem] flex-1" placeholder="Nome ou e-mail" aria-label="Novo participante" value={name} onChange={(event) => setName(event.target.value)} />
        <Button type="submit" size="sm" variant="outline" disabled={!name.trim()}>
          <UserPlus /> Adicionar
        </Button>
      </form>
    </Section>
  );
}

function TasksSection({ meetingId }: { meetingId: string }) {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [generating, setGenerating] = useState(false);

  const reload = useCallback(() => {
    listTasks(meetingId).then(setTasks);
  }, [meetingId]);

  useEffect(() => {
    reload();
    return onWorkspaceEvents(['tasks-updated'], reload);
  }, [reload]);

  const generate = async () => {
    setGenerating(true);
    try {
      // Na primeira vez aproveita o que o piloto automático já extraiu; "Gerar de novo" relê a reunião.
      const result = await generateTasks(meetingId, tasks.length > 0);
      setTasks(result.tasks);
      toast.success(result.added === 1 ? '1 tarefa' : `${result.added} tarefas`, {
        description: `${ORIGIN_MESSAGES[result.origin] ?? ''}${result.leavesMachine ? ' O texto foi enviado ao provedor configurado.' : ''}`.trim(),
      });
    } catch (error) {
      toast.error('Não foi possível gerar as tarefas', { description: errorText(error) });
    } finally {
      setGenerating(false);
    }
  };

  const groups = (['mine', 'theirs', 'unassigned'] as TaskDirection[])
    .map((direction) => ({ direction, items: tasks.filter((task) => task.direction === direction) }))
    .filter((group) => group.items.length > 0);

  return (
    <Section
      title={tasks.length > 0 ? `Tarefas (${tasks.filter((task) => task.status !== 'done').length} abertas)` : 'Tarefas'}
      action={
        <Button size="sm" variant="outline" onClick={generate} disabled={generating} title="Lê a reunião de novo; o que você editou, concluiu ou criou é mantido">
          {generating ? <Loader2 className="animate-spin" /> : <Sparkles />}
          {tasks.length > 0 ? 'Gerar de novo' : 'Gerar tarefas'}
        </Button>
      }
    >
      {groups.length === 0 ? (
        <p className="text-[13px] text-muted-foreground">
          Nenhuma tarefa ainda. Elas são extraídas quando o resumo termina, ou clique em “Gerar tarefas”.
        </p>
      ) : (
        <div className="space-y-3">
          {groups.map((group) => (
            <div key={group.direction}>
              <h3 className="px-2 text-xs font-medium text-muted-foreground">{DIRECTION_LABELS[group.direction]}</h3>
              <TaskList tasks={group.items} showMeeting={false} onChanged={reload} />
            </div>
          ))}
        </div>
      )}
      <div className="mt-3">
        <QuickAddTask meetingId={meetingId} onAdded={reload} />
      </div>
    </Section>
  );
}

/** Aba "Pessoas e tarefas" da reunião. */
export function PeopleAndTasks({ meetingId, onSpeakersChanged }: { meetingId: string; onSpeakersChanged?: () => void }) {
  return (
    <div className="h-full overflow-y-auto custom-scrollbar">
      <div className="space-y-3 p-3">
        <ParticipantsSection meetingId={meetingId} onSpeakersChanged={onSpeakersChanged} />
        <TasksSection meetingId={meetingId} />
      </div>
    </div>
  );
}
