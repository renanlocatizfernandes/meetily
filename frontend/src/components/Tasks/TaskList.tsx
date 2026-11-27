'use client';

import React, { useState } from 'react';
import Link from 'next/link';
import { ArrowLeftRight, CalendarClock, Check, MoreHorizontal, Pencil, Trash2, User } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { IconButton } from '@/components/ui/icon-button';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { errorText } from '@/lib/tauriSafe';
import {
  addTask,
  deleteTask,
  dueState,
  formatDueDate,
  updateTask,
  type Task,
  type TaskDirection,
  type TaskPatch,
} from '@/lib/workspace';
import { cn } from '@/lib/utils';

export const DIRECTION_LABELS: Record<TaskDirection, string> = {
  mine: 'Com você',
  theirs: 'Aguardando retorno',
  unassigned: 'Sem responsável',
};

const DUE_CLASSES = {
  overdue: 'bg-destructive/10 text-destructive',
  today: 'bg-amber-500/15 text-amber-700 dark:text-amber-400',
  soon: 'bg-muted text-foreground',
  later: 'bg-muted text-muted-foreground',
  none: 'bg-muted text-muted-foreground',
} as const;

async function patchTask(task: Task, patch: TaskPatch, onChanged: () => void) {
  try {
    await updateTask(task.id, patch);
    onChanged();
  } catch (error) {
    toast.error('Não foi possível atualizar a tarefa', { description: errorText(error) });
  }
}

function TaskEditor({ task, onDone, onChanged }: { task: Task; onDone: () => void; onChanged: () => void }) {
  const [text, setText] = useState(task.text);
  const [owner, setOwner] = useState(task.owner ?? '');
  const [dueDate, setDueDate] = useState(task.dueDate ?? '');

  return (
    <form
      className="flex flex-1 flex-col gap-2"
      onSubmit={async (event) => {
        event.preventDefault();
        await patchTask(task, { text, owner, dueDate }, onChanged);
        onDone();
      }}
    >
      <Input autoFocus value={text} onChange={(event) => setText(event.target.value)} aria-label="Texto da tarefa" />
      <div className="flex flex-wrap items-center gap-2">
        <Input className="h-8 w-44" placeholder="Responsável" value={owner} onChange={(event) => setOwner(event.target.value)} aria-label="Responsável" />
        <Input className="h-8 w-40" type="date" value={dueDate} onChange={(event) => setDueDate(event.target.value)} aria-label="Prazo" />
        <Button type="submit" size="sm" disabled={!text.trim()}>Salvar</Button>
        <Button type="button" size="sm" variant="ghost" onClick={onDone}>Cancelar</Button>
      </div>
    </form>
  );
}

function TaskRow({ task, showMeeting, onChanged }: { task: Task; showMeeting: boolean; onChanged: () => void }) {
  const [editing, setEditing] = useState(false);
  const done = task.status === 'done';
  const due = dueState(task);

  const remove = async () => {
    try {
      await deleteTask(task.id);
      onChanged();
    } catch (error) {
      toast.error('Não foi possível excluir a tarefa', { description: errorText(error) });
    }
  };

  return (
    <li className="group flex items-start gap-2.5 rounded-lg px-2 py-2 transition-colors hover:bg-accent/40">
      <button
        type="button"
        role="checkbox"
        aria-checked={done}
        aria-label={done ? 'Reabrir tarefa' : 'Concluir tarefa'}
        onClick={() => patchTask(task, { status: done ? 'open' : 'done' }, onChanged)}
        className={cn(
          'mt-0.5 flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-full border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
          done ? 'border-primary bg-primary text-primary-foreground' : 'border-muted-foreground/50 hover:border-primary',
        )}
      >
        {done && <Check className="h-3 w-3" strokeWidth={3} />}
      </button>

      {editing ? (
        <TaskEditor task={task} onDone={() => setEditing(false)} onChanged={onChanged} />
      ) : (
        <>
          <div className="min-w-0 flex-1">
            <p className={cn('text-sm leading-snug', done && 'text-muted-foreground line-through')} title={task.evidence ?? undefined}>
              {task.text}
            </p>
            <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
              {task.owner && (
                <span className="inline-flex items-center gap-1 rounded-md bg-muted px-1.5 py-0.5">
                  <User className="h-3 w-3" />
                  {task.owner}
                </span>
              )}
              {task.dueDate ? (
                <span className={cn('inline-flex items-center gap-1 rounded-md px-1.5 py-0.5', DUE_CLASSES[due])}>
                  <CalendarClock className="h-3 w-3" />
                  {due === 'overdue' ? 'Atrasada · ' : due === 'today' ? 'Hoje · ' : ''}
                  {formatDueDate(task.dueDate)}
                </span>
              ) : task.due ? (
                <span className="inline-flex items-center gap-1 rounded-md bg-muted px-1.5 py-0.5">
                  <CalendarClock className="h-3 w-3" />
                  {task.due}
                </span>
              ) : null}
              {showMeeting && task.meetingId && (
                <Link href={`/meeting-details?id=${task.meetingId}`} className="truncate underline-offset-2 hover:text-foreground hover:underline">
                  {[task.client, task.meetingTitle].filter(Boolean).join(' · ') || 'Abrir reunião'}
                </Link>
              )}
            </div>
          </div>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <IconButton label="Ações da tarefa" size="sm" className="opacity-0 focus-visible:opacity-100 group-hover:opacity-100 data-[state=open]:opacity-100">
                <MoreHorizontal />
              </IconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-56">
              <DropdownMenuItem onSelect={() => setEditing(true)}>
                <Pencil /> Editar
              </DropdownMenuItem>
              {(Object.keys(DIRECTION_LABELS) as TaskDirection[])
                .filter((direction) => direction !== task.direction)
                .map((direction) => (
                  <DropdownMenuItem key={direction} onSelect={() => patchTask(task, { direction }, onChanged)}>
                    <ArrowLeftRight /> Mover para “{DIRECTION_LABELS[direction]}”
                  </DropdownMenuItem>
                ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={remove} className="text-destructive focus:text-destructive">
                <Trash2 /> Excluir
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </>
      )}
    </li>
  );
}

export function TaskList({ tasks, showMeeting = true, onChanged }: { tasks: Task[]; showMeeting?: boolean; onChanged: () => void }) {
  return (
    <ul className="space-y-0.5">
      {tasks.map((task) => (
        <TaskRow key={task.id} task={task} showMeeting={showMeeting} onChanged={onChanged} />
      ))}
    </ul>
  );
}

/** Campo de uma linha para criar tarefa à mão. */
export function QuickAddTask({ meetingId, direction, onAdded, placeholder }: {
  meetingId?: string;
  direction?: TaskDirection;
  onAdded: () => void;
  placeholder?: string;
}) {
  const [text, setText] = useState('');
  const [saving, setSaving] = useState(false);

  return (
    <form
      className="flex flex-wrap gap-2"
      onSubmit={async (event) => {
        event.preventDefault();
        if (!text.trim() || saving) return;
        setSaving(true);
        try {
          await addTask({ text, meetingId: meetingId ?? null, direction: direction ?? null });
          setText('');
          onAdded();
        } catch (error) {
          toast.error('Não foi possível criar a tarefa', { description: errorText(error) });
        } finally {
          setSaving(false);
        }
      }}
    >
      <Input className="min-w-[9rem] flex-1" value={text} onChange={(event) => setText(event.target.value)} placeholder={placeholder ?? 'Nova tarefa'} aria-label="Nova tarefa" />
      <Button type="submit" variant="outline" disabled={!text.trim() || saving}>Adicionar</Button>
    </form>
  );
}
