'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Switch } from '@/components/ui/switch';

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
  const [watch, setWatch] = useState<Watch | null>(null);

  useEffect(() => {
    invoke<{ enabled: boolean }>('get_auto_record_settings')
      .then((settings) => setEnabled(settings.enabled))
      .catch(() => setEnabled(true))
      .finally(() => setLoaded(true));
  }, []);

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
    <div className="mx-auto mt-8 max-w-2xl space-y-4 rounded-xl border border-gray-200 bg-white p-6">
      <div>
        <h2 className="text-lg font-semibold text-gray-900">Gravação automática</h2>
        <p className="mt-1 text-sm text-gray-500">
          No macOS, o Meetily Live observa se o Microsoft Teams, o Zoom ou o Google Meet está numa chamada e começa a gravar sozinho, com a ajuda ao vivo. A gravação aparece na tela. Avise as outras pessoas que a reunião está sendo gravada.
        </p>
      </div>
      <label className="flex items-center justify-between gap-4">
        <span className="text-sm font-medium text-gray-800">Gravar quando uma reunião começar</span>
        <Switch checked={enabled} disabled={!loaded} onCheckedChange={toggle} />
      </label>
      <p className="text-sm text-amber-800">
        Avise as outras pessoas que esta reunião está sendo gravada.
      </p>
      <div className="rounded-lg border border-gray-200 bg-gray-50 p-3 text-sm text-gray-800">
        <div className="font-medium">Status agora</div>
        <p className="mt-1">{watch?.hint || 'Lendo o que o Mac está vendo…'}</p>
        {watch?.phase === 'arming' && (
          <p className="mt-1">Contador: {watch.secondsLeft ?? 0}s para começar.</p>
        )}
        <button type="button" className="mt-2 rounded-md border border-gray-300 bg-white px-3 py-1 text-xs" onClick={refresh}>
          Verificar agora
        </button>
      </div>
      <ol className="list-decimal space-y-1 pl-5 text-sm text-gray-700">
        <li>Deixe “Gravar quando uma reunião começar” ligado.</li>
        <li>No Mac: Ajustes → Privacidade → Microfone e Gravação de Tela e Áudio do Sistema, ambos para o Meetily Live. Reabra o app depois.</li>
        <li>Teams: clique em Reunir agora. Pode ser uma reunião só sua. Se o título da janela não mudar, tire o mudo do microfone do Teams.</li>
        <li>Zoom: entre numa sala. Meet: entre numa chamada (o título fica “Meet - …” ).</li>
        <li>O status acima deve mudar na hora. O contador dura cerca de 12 segundos e então o aviso vermelho aparece com Parar.</li>
      </ol>
    </div>
  );
}
