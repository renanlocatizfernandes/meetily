'use client';

import { useEffect, useRef, useState } from 'react';
import { AlertTriangle, Info } from 'lucide-react';
import { toast } from 'sonner';
import { Switch } from '@/components/ui/switch';
import { SettingsCard, SettingSwitch } from '@/components/settings/SettingsCard';
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
    <SettingsCard
      title="Piloto automático"
      description="Quando a gravação termina, o app prepara sozinho o que você faria depois da reunião. Fica desligado até você ligar e roda só depois que a gravação acaba."
      className="mt-4 first:mt-0"
      action={
        <Switch
          aria-label="Ligar o piloto automático"
          checked={current.enabled}
          disabled={!settings}
          onCheckedChange={(enabled) => save({ ...current, enabled })}
        />
      }
    >
      <div className="space-y-4">
      {notice && (
        <div
          role="note"
          className={`flex gap-2 rounded-lg border p-3 text-[13px] ${
            notice.tone === 'warning'
              ? 'border-warning/30 bg-warning-soft text-warning'
              : 'border-border bg-muted/40 text-muted-foreground'
          }`}
        >
          {notice.tone === 'warning' ? (
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden />
          ) : (
            <Info className="mt-0.5 h-4 w-4 shrink-0" aria-hidden />
          )}
          <span>{notice.text}</span>
        </div>
      )}

      <div>
        {STEPS.map((step) => (
          <SettingSwitch
            key={step.field}
            id={`autopilot-${step.field}`}
            label={step.label}
            hint={step.detail}
            checked={current[step.field]}
            disabled={!settings}
            onCheckedChange={(value) => save({ ...current, [step.field]: value })}
          />
        ))}
      </div>

      {current.enabled && !anyStep && (
        <p className="text-[13px] text-muted-foreground">Todas as etapas estão desligadas: o piloto não vai fazer nada.</p>
      )}
      <p className="text-[13px] text-muted-foreground">
        Sem chave no provedor pago, o piloto usa um modelo local, se houver; sem nenhum dos dois, não faz nada e avisa na própria reunião.
      </p>
      </div>
    </SettingsCard>
  );
}
