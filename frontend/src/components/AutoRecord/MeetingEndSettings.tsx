'use client';

import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Input } from '@/components/ui/input';
import { AutoRecordMode, AutoRecordModeControl, AutoRecordModeOption, isAutoRecordMode } from '@/components/AutoRecord/AutoRecordModeControl';
import { safeInvoke } from '@/lib/tauriSafe';
import { DEFAULT_SILENCE_MINUTES, MAX_SILENCE_MINUTES, MIN_SILENCE_MINUTES, minutesLabel, parseSilenceMinutes } from '@/lib/meetingEnd';

interface Settings {
  mode: AutoRecordMode;
  silenceMinutes: number;
  promptSeconds: number;
}

const END_MODES: AutoRecordModeOption[] = [
  { value: 'off', label: 'Desligado', detail: 'A gravação só para quando você parar.' },
  { value: 'ask', label: 'Perguntar', detail: 'Um aviso aparece e, sem resposta, a gravação encerra sozinha.', recommended: true },
  { value: 'auto', label: 'Parar sozinho', detail: 'A gravação encerra sem perguntar.' },
];

/** Settings → Recording: what to do when the meeting seems to be over. */
export function MeetingEndSettings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [minutesText, setMinutesText] = useState(String(DEFAULT_SILENCE_MINUTES));
  // False when the Rust core does not know these commands (older build).
  const [available, setAvailable] = useState(true);

  useEffect(() => {
    let active = true;
    safeInvoke<Settings>('get_meeting_end_settings').then((result) => {
      if (!active) return;
      if (result.ok && isAutoRecordMode(result.value?.mode)) {
        setSettings(result.value);
        setMinutesText(String(result.value.silenceMinutes));
      } else {
        setAvailable(false);
      }
    });
    return () => {
      active = false;
    };
  }, []);

  const save = async (next: Settings) => {
    const previous = settings;
    setSettings(next);
    const result = await safeInvoke<Settings>('save_meeting_end_settings', {
      mode: next.mode,
      silenceMinutes: next.silenceMinutes,
    });
    if (result.ok && isAutoRecordMode(result.value?.mode)) {
      setSettings(result.value);
      setMinutesText(String(result.value.silenceMinutes));
    } else {
      setSettings(previous);
      if (previous) setMinutesText(String(previous.silenceMinutes));
      toast.error('Não foi possível salvar a detecção de fim de reunião');
    }
  };

  const commitMinutes = () => {
    if (!settings) return;
    const minutes = parseSilenceMinutes(minutesText);
    if (minutes === null) {
      setMinutesText(String(settings.silenceMinutes));
      return;
    }
    setMinutesText(String(minutes));
    if (minutes !== settings.silenceMinutes) save({ ...settings, silenceMinutes: minutes });
  };

  if (!available) return null;

  const off = settings?.mode === 'off';
  const minutes = settings?.silenceMinutes ?? DEFAULT_SILENCE_MINUTES;

  return (
    <div className="mx-auto mt-8 max-w-2xl space-y-4 rounded-xl border border-border bg-card p-6 text-card-foreground">
      <div>
        <h2 className="text-lg font-semibold text-foreground">Fim da reunião</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          O Meetily Live Plus percebe que a reunião acabou quando a chamada do Teams, do Zoom ou do Meet some, ou quando ninguém fala por um tempo. Vale para qualquer gravação, inclusive a que você começou pelo botão.
        </p>
      </div>
      <div className="space-y-2">
        <span id="meeting-end-mode-label" className="text-sm font-medium text-foreground">Quando a reunião terminar</span>
        <AutoRecordModeControl
          value={settings?.mode ?? 'ask'}
          onChange={(mode) => settings && mode !== settings.mode && save({ ...settings, mode })}
          disabled={!settings}
          labelledBy="meeting-end-mode-label"
          options={END_MODES}
        />
      </div>
      <div className="flex items-center justify-between gap-4">
        <label htmlFor="meeting-end-silence" className="min-w-0">
          <span className="block text-sm font-medium text-foreground">Minutos de silêncio</span>
          <span className="block text-xs text-muted-foreground">
            Sem fala no microfone nem no áudio do sistema. De {MIN_SILENCE_MINUTES} a {MAX_SILENCE_MINUTES}.
          </span>
        </label>
        <Input
          id="meeting-end-silence"
          type="number"
          inputMode="numeric"
          min={MIN_SILENCE_MINUTES}
          max={MAX_SILENCE_MINUTES}
          step={1}
          value={minutesText}
          disabled={!settings || off}
          onChange={(event) => setMinutesText(event.target.value)}
          onBlur={commitMinutes}
          onKeyDown={(event) => {
            if (event.key === 'Enter') event.currentTarget.blur();
          }}
          className="w-20 shrink-0 text-right tabular-nums"
        />
      </div>
      {!off && (
        <ul className="list-disc space-y-1 pl-5 text-xs text-muted-foreground">
          <li>Chamada encerrada: o app espera cerca de 20 segundos sem a chamada antes de agir.</li>
          <li>
            Silêncio: {minutesLabel(minutes)} sem fala. Enquanto a chamada ainda aparece, o prazo é de {minutesLabel(minutes * 3)}, porque uma sala de espera também fica em silêncio.
          </li>
          {settings?.mode === 'ask' && (
            <li>
              O aviso espera {settings.promptSeconds} segundos. “Continuar gravando” adia novos avisos por 10 minutos. Se a fala voltar, o aviso some.
            </li>
          )}
          <li>A gravação pausada não conta como silêncio. Depois de encerrar, a reunião é salva como em qualquer parada.</li>
        </ul>
      )}
    </div>
  );
}
