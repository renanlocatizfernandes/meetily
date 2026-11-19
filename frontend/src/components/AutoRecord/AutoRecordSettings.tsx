'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Switch } from '@/components/ui/switch';

export function AutoRecordSettings() {
  const [enabled, setEnabled] = useState(true);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    invoke<{ enabled: boolean }>('get_auto_record_settings')
      .then((settings) => setEnabled(settings.enabled))
      .catch(() => setEnabled(true))
      .finally(() => setLoaded(true));
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
    </div>
  );
}
