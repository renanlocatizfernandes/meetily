'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { Check, ChevronDown, ChevronRight, Copy, Loader2, Plane, RefreshCw } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  STATUS_LABELS,
  STEP_LABELS,
  applyAutopilotSuggestion,
  emailToText,
  getAutopilotResult,
  hasContent,
  listToText,
  listenAutopilotEvents,
  providerLabel,
  runAutopilotForMeeting,
  tasksToText,
  type AutopilotResult,
  type AutopilotStepKey,
} from '@/lib/autopilot';

interface Props {
  meetingId: string;
  /** Chamado depois que o piloto muda título ou etiquetas da conversa. */
  onMeetingUpdated?: () => void | Promise<void>;
}

async function copy(text: string, done: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast.success(done);
  } catch {
    toast.error('Não foi possível copiar');
  }
}

function Group({ title, onCopy, children }: { title: string; onCopy?: () => void; children: React.ReactNode }) {
  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">{title}</h3>
        {onCopy && (
          <Button variant="ghost" size="sm" className="h-6 gap-1 px-1.5 text-xs text-muted-foreground" onClick={onCopy}>
            <Copy className="h-3 w-3" aria-hidden />
            Copiar
          </Button>
        )}
      </div>
      {children}
    </div>
  );
}

/** Resultado do piloto automático na página da reunião. Some quando não há nada a mostrar. */
export function AutopilotPanel({ meetingId, onMeetingUpdated }: Props) {
  const [result, setResult] = useState<AutopilotResult | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [applying, setApplying] = useState<'title' | 'tags' | null>(null);
  // Progress events trigger overlapping loads; only the newest one may update the panel.
  const loadSeq = useRef(0);
  // The parent passes a new callback on every render; a ref keeps the listeners stable.
  const onUpdatedRef = useRef(onMeetingUpdated);
  onUpdatedRef.current = onMeetingUpdated;

  const load = useCallback(async () => {
    const id = ++loadSeq.current;
    const value = await getAutopilotResult(meetingId);
    if (id === loadSeq.current) setResult(value);
  }, [meetingId]);

  useEffect(() => {
    setResult(null);
    setOpen(false);
    void load();
  }, [load]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    listenAutopilotEvents({
      onProgress: (event) => {
        if (event.meetingId === meetingId) void load();
      },
      onFinished: (event) => {
        if (event.meetingId !== meetingId) return;
        setBusy(false);
        void load();
        void onUpdatedRef.current?.();
      },
    }).then((fn) => {
      if (active) unlisten = fn;
      else fn();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [meetingId, load]);

  const rerun = async () => {
    setBusy(true);
    try {
      if (!(await runAutopilotForMeeting(meetingId))) {
        toast.info('O piloto automático já está rodando nesta conversa');
        setBusy(false);
      } else {
        void load();
      }
    } catch {
      setBusy(false);
      toast.error('Não foi possível iniciar o piloto automático');
    }
  };

  const apply = async (kind: 'title' | 'tags') => {
    if (applying) return;
    setApplying(kind);
    try {
      await applyAutopilotSuggestion(meetingId, kind);
      toast.success(kind === 'title' ? 'Título aplicado' : 'Etiquetas adicionadas');
      void load();
      void onUpdatedRef.current?.();
    } catch {
      toast.error('Não foi possível aplicar a sugestão');
    } finally {
      setApplying(null);
    }
  };

  if (!result || !hasContent(result)) return null;

  const running = result.status === 'running' || busy;
  const steps = (Object.keys(STEP_LABELS) as AutopilotStepKey[]).filter((key) => result.steps[key] && result.steps[key] !== 'off');
  const text = {
    tasks: tasksToText(result.tasks),
    decisions: listToText('Decisões', result.decisions),
    pending: listToText('Pendências', result.pending),
  };

  return (
    <section className="border-b bg-muted/20 px-4 py-2 text-sm" aria-label="Piloto automático">
      <div className="flex items-center gap-2">
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
        >
          {open ? <ChevronDown className="h-4 w-4 shrink-0" aria-hidden /> : <ChevronRight className="h-4 w-4 shrink-0" aria-hidden />}
          <Plane className="h-4 w-4 shrink-0 text-muted-foreground" aria-hidden />
          <span className="font-medium">Piloto automático</span>
          <span className="truncate text-xs text-muted-foreground">
            {running ? 'trabalhando…' : STATUS_LABELS[result.status]}
            {result.tasks.length > 0 && ` · ${result.tasks.length} tarefa${result.tasks.length > 1 ? 's' : ''}`}
            {result.decisions.length > 0 && ` · ${result.decisions.length} decisã${result.decisions.length > 1 ? 'ões' : 'o'}`}
            {result.pending.length > 0 && ` · ${result.pending.length} pendência${result.pending.length > 1 ? 's' : ''}`}
          </span>
        </button>
        {running ? (
          <span role="status" className="flex items-center">
            <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" aria-hidden />
            <span className="sr-only">Em andamento</span>
          </span>
        ) : (
          <Button variant="ghost" size="sm" className="h-7 gap-1 text-xs text-muted-foreground" onClick={rerun}>
            <RefreshCw className="h-3 w-3" aria-hidden />
            Rodar de novo
          </Button>
        )}
      </div>

      {open && (
        <div className="mt-3 space-y-4 pb-2 pl-6">
          {result.note && <p className="text-xs text-muted-foreground">{result.note}</p>}
          {result.provider && (
            <p className="text-xs text-muted-foreground">
              {providerLabel(result.provider)}
              {result.model ? ` · ${result.model}` : ''}
              {result.leavesMachine ? ' · provedor pago, consumiu créditos' : ' · local, sem custo'}
            </p>
          )}
          {steps.length > 0 && (
            <ul className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
              {steps.map((key) => (
                <li key={key} className="flex items-center gap-1">
                  {result.steps[key] === 'done' ? <Check className="h-3 w-3 text-emerald-600" aria-hidden /> : <span aria-hidden>·</span>}
                  {STEP_LABELS[key]}
                  {result.steps[key] === 'failed' && ' (falhou)'}
                </li>
              ))}
            </ul>
          )}

          {result.tasks.length > 0 && (
            <Group title="Tarefas" onCopy={() => copy(text.tasks, 'Tarefas copiadas')}>
              <ul className="space-y-1">
                {result.tasks.map((task, index) => (
                  <li key={`${index}-${task.text}`} className="flex flex-wrap items-baseline gap-x-2">
                    <span>{task.text}</span>
                    {task.owner && <span className="text-xs text-muted-foreground">· {task.owner}</span>}
                    {task.due && <span className="text-xs text-muted-foreground">· até {task.due}</span>}
                  </li>
                ))}
              </ul>
            </Group>
          )}
          {result.decisions.length > 0 && (
            <Group title="Decisões" onCopy={() => copy(text.decisions, 'Decisões copiadas')}>
              <ul className="list-disc space-y-1 pl-5">
                {result.decisions.map((item, index) => (
                  <li key={`${index}-${item}`}>{item}</li>
                ))}
              </ul>
            </Group>
          )}
          {result.pending.length > 0 && (
            <Group title="Pendências" onCopy={() => copy(text.pending, 'Pendências copiadas')}>
              <ul className="list-disc space-y-1 pl-5">
                {result.pending.map((item, index) => (
                  <li key={`${index}-${item}`}>{item}</li>
                ))}
              </ul>
            </Group>
          )}

          {(result.suggestedTitle || result.suggestedTags.length > 0) && (
            <Group title="Título e etiquetas sugeridos">
              {result.suggestedTitle && (
                <div className="flex flex-wrap items-center gap-2">
                  <span>{result.suggestedTitle}</span>
                  {result.titleApplied ? (
                    <span className="text-xs text-muted-foreground">aplicado</span>
                  ) : (
                    <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={applying !== null} onClick={() => apply('title')}>
                      Usar como título
                    </Button>
                  )}
                </div>
              )}
              {result.suggestedTags.length > 0 && (
                <div className="flex flex-wrap items-center gap-1.5">
                  {result.suggestedTags.map((tag, index) => (
                    <span key={`${index}-${tag}`} className="rounded-full border px-2 py-0.5 text-xs">
                      {tag}
                    </span>
                  ))}
                  {result.tagsApplied ? (
                    <span className="text-xs text-muted-foreground">adicionadas</span>
                  ) : (
                    <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={applying !== null} onClick={() => apply('tags')}>
                      Adicionar etiquetas
                    </Button>
                  )}
                </div>
              )}
            </Group>
          )}

          {result.emailBody && (
            <Group title="Rascunho de e-mail (não enviado)" onCopy={() => copy(emailToText(result), 'E-mail copiado')}>
              {result.emailSubject && <p className="font-medium">{result.emailSubject}</p>}
              <pre className="whitespace-pre-wrap rounded-lg border bg-background p-3 font-sans text-sm">{result.emailBody}</pre>
            </Group>
          )}
        </div>
      )}
    </section>
  );
}
