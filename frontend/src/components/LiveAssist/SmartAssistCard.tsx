'use client';

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Switch } from '@/components/ui/switch';
import { SettingsCard } from '@/components/LiveAssist/SettingsCard';
import { DEFAULT_SMART_SETTINGS, SmartSettings } from '@/lib/liveAssistSmart';

const CLOUD_PROVIDERS = new Set(['openai', 'claude', 'groq', 'openrouter', 'custom-openai']);

function Row({ title, hint, checked, onChange, disabled }: { title: string; hint: string; checked: boolean; onChange: (value: boolean) => void; disabled?: boolean }) {
  return (
    <label className="flex items-start justify-between gap-4">
      <span>
        <span className="block text-sm font-medium text-foreground">{title}</span>
        <span className="block text-xs text-muted-foreground">{hint}</span>
      </span>
      <Switch checked={checked} onCheckedChange={onChange} disabled={disabled} />
    </label>
  );
}

/** Switches of the proactive assistant. Every change is saved at once. */
export function SmartAssistCard() {
  const [settings, setSettings] = useState<SmartSettings>(DEFAULT_SMART_SETTINGS);
  const [loaded, setLoaded] = useState(false);
  const [cloud, setCloud] = useState(false);

  useEffect(() => {
    invoke<SmartSettings>('get_live_assist_smart_settings')
      .then((value) => setSettings(value))
      .catch(() => undefined)
      .finally(() => setLoaded(true));
    invoke<{ provider?: string } | null>('api_get_model_config')
      .then((config) => setCloud(CLOUD_PROVIDERS.has((config?.provider || 'ollama').toLowerCase())))
      .catch(() => setCloud(false));
  }, []);

  const saveSeq = useRef(0);

  const update = async (patch: Partial<SmartSettings>) => {
    // Until the saved values arrive, a save would overwrite them with the defaults.
    if (!loaded) return;
    const previous = settings;
    const next = { ...settings, ...patch };
    const id = ++saveSeq.current;
    setSettings(next);
    try {
      const saved = await invoke<SmartSettings>('save_live_assist_smart_settings', { settings: next });
      if (id === saveSeq.current) setSettings(saved);
    } catch (error) {
      if (id === saveSeq.current) setSettings(previous);
      toast.error('Não foi possível salvar');
      console.error(error);
    }
  };

  const economyOn = settings.economy === 'on' || (settings.economy === 'auto' && cloud);

  return (
    <SettingsCard
      title="Ajuda proativa"
      description="Avisos durante a reunião. Os alertas são calculados nesta máquina, sem modelo e sem custo. Todos são opcionais."
    >
      <div className={`space-y-4 ${loaded ? '' : 'opacity-60'}`}>
        <Row
          title="Pergunta para você"
          hint="Avisa quando alguém faz uma pergunta direta e oferece sugerir uma resposta."
          checked={settings.questionAlerts}
          disabled={!loaded}
          onChange={(value) => update({ questionAlerts: value })}
        />
        <Row
          title="Decisões"
          hint="Avisa quando a conversa fecha uma decisão (&quot;ficou decidido&quot;, &quot;combinamos&quot;)."
          checked={settings.decisionAlerts}
          disabled={!loaded}
          onChange={(value) => update({ decisionAlerts: value })}
        />
        <Row
          title="Tarefas"
          hint="Avisa quando alguém pede algo a você ou define um próximo passo."
          checked={settings.taskAlerts}
          disabled={!loaded}
          onChange={(value) => update({ taskAlerts: value })}
        />
        <Row
          title="Possíveis divergências"
          hint="Compara números e dias ditos agora com as anotações de reuniões anteriores do mesmo cliente. Só aponta, nunca afirma."
          checked={settings.contradictionAlerts}
          disabled={!loaded}
          onChange={(value) => update({ contradictionAlerts: value })}
        />
        <Row
          title="Responder perguntas sozinho"
          hint="Escreve uma resposta assim que uma pergunta é detectada. Chama o modelo; com provedor na nuvem, gasta tokens. Desligado por padrão."
          checked={settings.autoAnswer}
          disabled={!loaded}
          onChange={(value) => update({ autoAnswer: value })}
        />
        <label className="block space-y-1">
          <span className="text-sm font-medium text-foreground">Modo economia</span>
          <select
            className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            value={settings.economy}
            disabled={!loaded}
            onChange={(event) => update({ economy: event.target.value as SmartSettings['economy'] })}
          >
            <option value="auto">Automático (liga só com provedor na nuvem)</option>
            <option value="on">Sempre ligado</option>
            <option value="off">Desligado</option>
          </select>
          <span className="block text-xs text-muted-foreground">
            Agrupa as chamadas: só uma pergunta chama o modelo na hora (no mínimo a cada 20 s); o resto vira uma chamada a cada 45 s, com menos texto. Resposta um pouco mais lenta, bem menos tokens.
            {economyOn ? ' Está ligado agora.' : ''}
          </span>
        </label>
        {cloud && (
          <p className="rounded-md bg-amber-50 px-3 py-2 text-xs text-amber-800">
            O provedor de resumo é um serviço na nuvem. O painel da ajuda ao vivo mostra uma estimativa dos tokens usados na gravação.
          </p>
        )}
      </div>
    </SettingsCard>
  );
}
