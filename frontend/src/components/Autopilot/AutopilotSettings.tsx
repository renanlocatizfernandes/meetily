'use client';

import { useEffect, useRef, useState } from 'react';
import { AlertTriangle, Info } from 'lucide-react';
import { toast } from 'sonner';
import { Switch } from '@/components/ui/switch';
import {
  DEFAULT_AUTOPILOT_SETTINGS,
  costNotice,
  getAutopilotSettings,
  getAutopilotStatus,
  saveAutopilotSettings,
  type AutopilotSettings as Settings,
  type AutopilotStatus,
} from '@/lib/autopilot';

type StepField = 'stepSummary' | 'stepTasks' | 'stepTitleTags' | 'stepEmail';

const STEPS: { field: StepField; label: string; detail: string }[] = [
  { field: 'stepSummary', label: 'Resumo', detail: 'Gera o resumo com o provedor e o modelo de resumo escolhidos.' },
  { field: 'stepTasks', label: 'Tarefas, decisões e pendências', detail: 'Lista o que ficou combinado, com responsável e prazo quando foram ditos.' },
  { field: 'stepTitleTags', label: 'Título e etiquetas', detail: 'Sugere um título e etiquetas. Um título que você digitou nunca é trocado.' },
  { field: 'stepEmail', label: 'Rascunho de e-mail', detail: 'Prepara um texto de follow-up para você revisar. Nada é enviado.' },
];

/** Configurações → Resumo: piloto automático pós-reunião (desligado por padrão). */
export function AutopilotSettings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<AutopilotStatus | null>(null);

  useEffect(() => {
    let active = true;
    getAutopilotSettings().then((value) => active && setSettings(value));
    getAutopilotStatus().then((value) => active && setStatus(value));
    return () => {
      active = false;
    };
  }, []);

  // Quick toggles send overlapping saves; only the newest may update the screen.
  const saveSeq = useRef(0);

  const save = async (next: Settings) => {
    const previous = settings;
    const id = ++saveSeq.current;
    setSettings(next);
    try {
      const saved = await saveAutopilotSettings(next);
      if (id !== saveSeq.current) return;
      setSettings(saved);
      // O aviso de custo depende do provedor; confere de novo ao ligar.
      if (next.enabled && !previous?.enabled) getAutopilotStatus().then(setStatus);
    } catch {
      if (id === saveSeq.current) setSettings(previous);
      toast.error('Não foi possível salvar o piloto automático');
    }
  };

  const current = settings ?? DEFAULT_AUTOPILOT_SETTINGS;
  const notice = costNotice(status);
  const anyStep = current.stepSummary || current.stepTasks || current.stepTitleTags || current.stepEmail;

  return (
    <section className="mx-auto mt-8 max-w-2xl space-y-4 rounded-xl border border-border bg-card p-6 text-card-foreground">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold text-foreground">Piloto automático</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            Quando a gravação termina, o app prepara sozinho o que você faria depois da reunião. Fica desligado até você ligar e roda só depois que a gravação acaba.
          </p>
        </div>
        <Switch
          aria-label="Ligar o piloto automático"
          checked={current.enabled}
          disabled={!settings}
          onCheckedChange={(enabled) => save({ ...current, enabled })}
        />
      </div>

      {notice && (
        <div
          role="note"
          className={`flex gap-2 rounded-lg border p-3 text-sm ${
            notice.tone === 'warning'
              ? 'border-amber-500/40 bg-amber-500/10 text-foreground'
              : 'border-border bg-muted/40 text-muted-foreground'
          }`}
        >
          {notice.tone === 'warning' ? (
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600" aria-hidden />
          ) : (
            <Info className="mt-0.5 h-4 w-4 shrink-0" aria-hidden />
          )}
          <span>{notice.text}</span>
        </div>
      )}

      <div className="space-y-3">
        {STEPS.map((step) => (
          <div key={step.field} className="flex items-center justify-between gap-4">
            <div className="min-w-0">
              <span id={`autopilot-${step.field}`} className="block text-sm font-medium text-foreground">
                {step.label}
              </span>
              <span className="block text-xs text-muted-foreground">{step.detail}</span>
            </div>
            <Switch
              aria-labelledby={`autopilot-${step.field}`}
              checked={current[step.field]}
              disabled={!settings}
              onCheckedChange={(value) => save({ ...current, [step.field]: value })}
            />
          </div>
        ))}
      </div>

      {current.enabled && !anyStep && (
        <p className="text-xs text-muted-foreground">Todas as etapas estão desligadas: o piloto não vai fazer nada.</p>
      )}
      <p className="text-xs text-muted-foreground">
        Sem chave no provedor pago, o piloto usa um modelo local, se houver; sem nenhum dos dois, não faz nada e avisa na própria reunião.
      </p>
    </section>
  );
}
