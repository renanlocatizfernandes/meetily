'use client';

import { useState } from 'react';
import { AlertTriangle, CheckCircle2, HelpCircle, ListTodo, Loader2, Sparkles, X } from 'lucide-react';
import { toast } from 'sonner';
import { safeInvoke } from '@/lib/tauriSafe';
import { SmartAlert, SmartAlertKind, alertTitle, recapCostNote } from '@/lib/liveAssistSmart';

const ICONS: Record<SmartAlertKind, typeof HelpCircle> = {
  pergunta: HelpCircle,
  decisao: CheckCircle2,
  tarefa: ListTodo,
  divergencia: AlertTriangle,
};

const TONES: Record<SmartAlertKind, string> = {
  pergunta: 'border-blue-200 bg-blue-50 text-blue-900',
  decisao: 'border-emerald-200 bg-emerald-50 text-emerald-900',
  tarefa: 'border-violet-200 bg-violet-50 text-violet-900',
  divergencia: 'border-amber-300 bg-amber-50 text-amber-900',
};

function AlertCard({ alert, onDismiss, canAnswer }: { alert: SmartAlert; onDismiss: () => void; canAnswer: boolean }) {
  const [busy, setBusy] = useState(false);
  const Icon = ICONS[alert.kind];

  const suggest = async () => {
    setBusy(true);
    const result = await safeInvoke<{ text: string; inputTokens: number; outputTokens: number; paid: boolean }>(
      'live_assist_suggest_answer',
      { question: alert.text },
    );
    setBusy(false);
    if (result.ok) {
      toast.success('Resposta sugerida', { description: recapCostNote(result.value) });
      onDismiss();
    } else {
      toast.error(result.error || 'Não foi possível sugerir uma resposta');
    }
  };

  return (
    <article className={`rounded-lg border p-2.5 text-sm ${TONES[alert.kind]}`}>
      <div className="flex items-start gap-2">
        <Icon className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <p className="text-[11px] font-semibold uppercase tracking-wide">
            {alertTitle(alert.kind)}
            {alert.speaker ? ` · ${alert.speaker}` : ''}
          </p>
          <p className="mt-0.5 break-words leading-snug">{alert.text}</p>
          {alert.detail && <p className="mt-1 break-words text-xs opacity-80">{alert.detail}</p>}
          {alert.kind === 'pergunta' && canAnswer && (
            <button
              type="button"
              disabled={busy}
              onClick={suggest}
              className="mt-1.5 inline-flex items-center gap-1 rounded bg-white/70 px-2 py-1 text-xs font-medium hover:bg-white disabled:opacity-60"
              title="Chama o modelo uma vez. Com provedor na nuvem, gasta tokens."
            >
              {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Sparkles className="h-3.5 w-3.5" />}
              Sugerir resposta
            </button>
          )}
        </div>
        <button type="button" className="rounded p-1 hover:bg-white/60" onClick={onDismiss} title="Dispensar">
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
    </article>
  );
}

/** Alerts raised without the model: a question to the owner, a decision, a task, a possible divergence. */
export function SmartAlerts({
  alerts,
  onDismiss,
  onClear,
  canAnswer,
}: {
  alerts: SmartAlert[];
  onDismiss: (id: string) => void;
  onClear: () => void;
  canAnswer: boolean;
}) {
  if (alerts.length === 0) return null;
  return (
    <section aria-label="Alertas da reunião" className="space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">Alertas</h3>
        {alerts.length > 1 && (
          <button type="button" className="text-xs text-muted-foreground hover:underline" onClick={onClear}>
            Dispensar todos
          </button>
        )}
      </div>
      {alerts.map((alert) => (
        <AlertCard key={alert.id} alert={alert} canAnswer={canAnswer} onDismiss={() => onDismiss(alert.id)} />
      ))}
    </section>
  );
}
