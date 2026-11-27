'use client';

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { ListChecks } from 'lucide-react';
import { TopBar } from '@/components/Shell/TopBar';
import { Input } from '@/components/ui/input';
import { Skeleton } from '@/components/ui/skeleton';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { foldText } from '@/lib/agenda';
import { dueState, listTasks, onWorkspaceEvents, type Task, type TaskDirection } from '@/lib/workspace';
import { cn } from '@/lib/utils';
import { DIRECTION_LABELS, QuickAddTask, TaskList } from './TaskList';

type TabValue = TaskDirection | 'done';

const TABS: Array<{ value: TabValue; label: string; empty: string }> = [
  { value: 'mine', label: 'Com você', empty: 'Nada pendente com você.' },
  { value: 'theirs', label: 'Aguardando retorno', empty: 'Você não está esperando nada de ninguém.' },
  { value: 'unassigned', label: 'Sem responsável', empty: 'Toda tarefa tem dono.' },
  { value: 'done', label: 'Concluídas', empty: 'Nenhuma tarefa concluída ainda.' },
];

const ALL_CLIENTS = '__todos__';
const DUE_ORDER = { overdue: 0, today: 1, soon: 2, later: 3, none: 4 } as const;

function sortOpen(a: Task, b: Task): number {
  const byState = DUE_ORDER[dueState(a)] - DUE_ORDER[dueState(b)];
  if (byState !== 0) return byState;
  if (a.dueDate && b.dueDate && a.dueDate !== b.dueDate) return a.dueDate < b.dueDate ? -1 : 1;
  return a.createdAt < b.createdAt ? 1 : -1;
}

export function TasksView() {
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [tab, setTab] = useState<TabValue>('mine');
  const [client, setClient] = useState(ALL_CLIENTS);
  const [query, setQuery] = useState('');

  const reload = useCallback(() => {
    listTasks().then(setTasks);
  }, []);

  useEffect(() => {
    reload();
    return onWorkspaceEvents(['tasks-updated', 'meeting-organized'], reload);
  }, [reload]);

  const all = tasks ?? [];
  const clients = useMemo(
    () => [...new Set(all.map((task) => task.client).filter((name): name is string => !!name))].sort((a, b) => a.localeCompare(b, 'pt-BR')),
    [all],
  );

  const filtered = useMemo(() => {
    const terms = foldText(query).split(/\s+/).filter(Boolean);
    return all.filter((task) => {
      if (client !== ALL_CLIENTS && task.client !== client) return false;
      if (terms.length === 0) return true;
      const haystack = foldText([task.text, task.owner ?? '', task.meetingTitle ?? '', task.client ?? ''].join(' '));
      return terms.every((term) => haystack.includes(term));
    });
  }, [all, client, query]);

  const groups = useMemo(() => {
    const open = filtered.filter((task) => task.status !== 'done');
    return {
      mine: open.filter((task) => task.direction === 'mine').sort(sortOpen),
      theirs: open.filter((task) => task.direction === 'theirs').sort(sortOpen),
      unassigned: open.filter((task) => task.direction === 'unassigned').sort(sortOpen),
      done: filtered
        .filter((task) => task.status === 'done')
        .sort((a, b) => ((a.doneAt ?? '') < (b.doneAt ?? '') ? 1 : -1)),
    } satisfies Record<TabValue, Task[]>;
  }, [filtered]);

  const current = TABS.find((item) => item.value === tab) ?? TABS[0];
  const list = groups[tab];
  const overdue = [...groups.mine, ...groups.theirs, ...groups.unassigned].filter((task) => dueState(task) === 'overdue').length;

  return (
    <div className="flex h-full min-w-0 flex-col bg-background">
      <TopBar
        title={<span className="block truncate px-2">Tarefas</span>}
        chips={overdue > 0 ? (
          <span className="rounded-md bg-destructive/10 px-1.5 py-0.5 text-xs text-destructive">
            {overdue === 1 ? '1 atrasada' : `${overdue} atrasadas`}
          </span>
        ) : undefined}
      />

      <div className="min-h-0 flex-1 overflow-y-auto custom-scrollbar">
        <div className="mx-auto w-full max-w-[820px] px-6 py-5">
          <div role="tablist" aria-label="Grupos de tarefas" className="flex flex-wrap gap-1 border-b">
            {TABS.map((item) => (
              <button
                key={item.value}
                type="button"
                role="tab"
                aria-selected={tab === item.value}
                onClick={() => setTab(item.value)}
                className={cn(
                  '-mb-px flex items-center gap-1.5 border-b-2 px-3 py-2 text-sm transition-colors',
                  tab === item.value ? 'border-primary font-medium text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground',
                )}
              >
                {item.label}
                <span className="rounded-full bg-muted px-1.5 text-xs tabular-nums text-muted-foreground">{groups[item.value].length}</span>
              </button>
            ))}
          </div>

          <div className="mt-4 flex flex-wrap gap-2">
            <Input
              className="min-w-[200px] flex-1"
              placeholder="Filtrar tarefas"
              aria-label="Filtrar tarefas"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            {clients.length > 0 && (
              <Select value={client} onValueChange={setClient}>
                <SelectTrigger className="w-52" aria-label="Filtrar por cliente">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={ALL_CLIENTS}>Todos os clientes</SelectItem>
                  {clients.map((name) => (
                    <SelectItem key={name} value={name}>{name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>

          {tab !== 'done' && (
            <div className="mt-3">
              <QuickAddTask
                direction={tab}
                onAdded={reload}
                placeholder={tab === 'theirs' ? 'O que você está esperando, e de quem?' : `Nova tarefa em “${DIRECTION_LABELS[tab]}”`}
              />
            </div>
          )}

          <div className="mt-4">
            {tasks === null ? (
              <div className="space-y-2" aria-busy="true" aria-label="Carregando tarefas">
                <Skeleton className="h-10 w-full" />
                <Skeleton className="h-10 w-full" />
                <Skeleton className="h-10 w-full" />
              </div>
            ) : list.length === 0 ? (
              <div className="flex flex-col items-center gap-2 py-14 text-center">
                <ListChecks className="h-8 w-8 text-muted-foreground" />
                <p className="text-sm font-medium">{query || client !== ALL_CLIENTS ? 'Nada com esse filtro.' : current.empty}</p>
                {all.length === 0 && (
                  <p className="max-w-sm text-[13px] text-muted-foreground">
                    Quando o resumo de uma reunião termina, o que ficou combinado aparece aqui, separado entre o que é seu e o que você espera dos outros.
                  </p>
                )}
              </div>
            ) : (
              <TaskList tasks={list} onChanged={reload} />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
