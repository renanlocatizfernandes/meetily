'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { SettingsCard, SettingSwitch } from '@/components/settings/SettingsCard';
import { AutoRecordMode, AutoRecordModeControl, isAutoRecordMode } from '@/components/AutoRecord/AutoRecordModeControl';

interface Watch {
  active: boolean;
  app?: string | null;
  phase: string;
  hint: string;
  secondsLeft?: number | null;
}

export function AutoRecordSettings() {
  const [enabled, setEnabled] = useState(true);
  const [loaded, setLoaded] = useState(false);
  // Null when the Rust core still answers with the old on/off setting only.
  const [mode, setMode] = useState<AutoRecordMode | null>(null);
  const [watch, setWatch] = useState<Watch | null>(null);

  useEffect(() => {
    invoke<{ enabled: boolean; mode?: AutoRecordMode }>('get_auto_record_settings')
      .then((settings) => {
        setEnabled(settings.enabled);
        if (isAutoRecordMode(settings.mode)) setMode(settings.mode);
      })
      .catch(() => setEnabled(true))
      .finally(() => setLoaded(true));
    // The privacy dialog may change the mode while this card is on screen.
    const onChanged = (event: Event) => {
      const next = (event as CustomEvent<{ mode?: AutoRecordMode }>).detail?.mode;
      if (isAutoRecordMode(next)) setMode((current) => (current === null ? current : next));
    };
    window.addEventListener('auto-record-settings-changed', onChanged);
    return () => window.removeEventListener('auto-record-settings-changed', onChanged);
  }, []);

  const changeMode = async (next: AutoRecordMode) => {
    const previous = mode;
    setMode(next);
    try {
      const saved = await invoke<{ enabled: boolean; mode?: AutoRecordMode }>('save_auto_record_settings', {
        enabled: next !== 'off',
        mode: next,
      });
      setEnabled(saved.enabled);
      if (isAutoRecordMode(saved.mode)) setMode(saved.mode);
    } catch (error) {
      setMode(previous);
      toast.error('Não foi possível salvar a gravação automática');
      console.error(error);
    }
  };

  const refresh = async () => {
    try {
      setWatch(await invoke<Watch>('auto_record_watch'));
    } catch (error) {
      console.error(error);
    }
  };

  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 2000);
    return () => clearInterval(timer);
  }, []);

  const toggle = async (next: boolean) => {
    setEnabled(next);
    try {
      const saved = await invoke<{ enabled: boolean }>('save_auto_record_settings', { enabled: next });
      setEnabled(saved.enabled);
    } catch (error) {
      setEnabled(!next);
      toast.error('Não foi possível salvar a gravação automática');
      console.error(error);
    }
  };

  return (
    <SettingsCard
      title="Gravação automática"
      description="No macOS, o Meetily Live Plus observa se o Microsoft Teams, o Zoom ou o Google Meet está numa chamada e começa a gravar sozinho, com a ajuda ao vivo. A gravação aparece na tela."
      className="mt-4 first:mt-0"
    >
      <div className="space-y-4">
      {mode !== null ? (
        <div className="space-y-2">
          <span id="auto-record-mode-label" className="text-sm font-medium text-foreground">Quando uma reunião começar</span>
          <AutoRecordModeControl value={mode} onChange={changeMode} disabled={!loaded} labelledBy="auto-record-mode-label" />
        </div>
      ) : (
        <SettingSwitch
          id="auto-record-enabled"
          label="Gravar quando uma reunião começar"
          checked={enabled}
          disabled={!loaded}
          onCheckedChange={toggle}
        />
      )}
      <p className="rounded-md border border-warning/30 bg-warning-soft px-3 py-2 text-[13px] text-warning">
        Avise as outras pessoas que esta reunião está sendo gravada.
      </p>
      <div className="rounded-lg border bg-muted p-3 text-[13px] text-foreground">
        <div className="font-medium">Status agora</div>
        <p className="mt-1 text-muted-foreground">{watch?.hint || 'Lendo o que o Mac está vendo…'}</p>
        {watch?.phase === 'arming' && (
          <p className="mt-1 text-muted-foreground">Contador: {watch.secondsLeft ?? 0}s para começar.</p>
        )}
        <Button type="button" variant="outline" size="sm" className="mt-2" onClick={refresh}>
          Verificar agora
        </Button>
      </div>
      <ol className="list-decimal space-y-1 pl-5 text-[13px] text-muted-foreground">
        <li>{mode !== null ? 'Escolha “Perguntar antes de gravar” ou “Gravar sozinho”.' : 'Deixe “Gravar quando uma reunião começar” ligado.'}</li>
        <li>No Mac: Ajustes → Privacidade → Microfone e Gravação de Tela e Áudio do Sistema, ambos para o Meetily Live Plus. Reabra o app depois.</li>
        <li>Teams: clique em Reunir agora. Pode ser uma reunião só sua. Se o título da janela não mudar, tire o mudo do microfone do Teams.</li>
        <li>Zoom: entre numa sala. Meet: entre numa chamada (o título fica “Meet - …” ).</li>
        <li>{mode === 'ask' ? 'O status acima deve mudar na hora e um aviso pergunta se você quer gravar.' : 'O status acima deve mudar na hora. O contador dura cerca de 12 segundos e então o aviso vermelho aparece com Parar.'}</li>
      </ol>
      </div>
    </SettingsCard>
  );
}
