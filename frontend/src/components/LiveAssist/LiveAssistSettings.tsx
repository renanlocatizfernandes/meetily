'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Switch } from '@/components/ui/switch';
import { useLiveAssist, LiveAssistSettings as Settings } from '@/contexts/LiveAssistContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { ShortcutCard } from '@/components/LiveAssist/ShortcutCard';
import { LatencyBenchmarkCard } from '@/components/LiveAssist/LatencyBenchmarkCard';
import { VoiceModelCard } from '@/components/VoiceModel/VoiceModelCard';
import { ContextProfileCard } from '@/components/LiveAssist/ContextProfileCard';
import { SmartAssistCard } from '@/components/LiveAssist/SmartAssistCard';
import { providerLabel } from '@/lib/ask';

const CLOUD_PROVIDERS = new Set(['openai', 'claude', 'groq', 'openrouter', 'custom-openai']);

export function LiveAssistSettings() {
  const { settings, settingsLoaded, saveSettings, status } = useLiveAssist();
  const { isRecording } = useRecordingState();
  const [draft, setDraft] = useState<Settings>(settings);
  const [saving, setSaving] = useState(false);
  const [models, setModels] = useState<string[]>([]);
  const [summaryProvider, setSummaryProvider] = useState<string>('ollama');

  useEffect(() => {
    if (settingsLoaded) setDraft(settings);
  }, [settings, settingsLoaded]);

  useEffect(() => {
    invoke<Array<{ name: string }>>('get_ollama_models', { endpoint: null })
      .then((list) => setModels(list.map((model) => model.name)))
      .catch(() => setModels([]));
    invoke<{ provider?: string } | null>('api_get_model_config')
      .then((config) => setSummaryProvider(config?.provider || 'ollama'))
      .catch(() => setSummaryProvider('ollama'));
  }, []);

  const cloud = CLOUD_PROVIDERS.has(summaryProvider.toLowerCase());

  const save = async () => {
    setSaving(true);
    try {
      await saveSettings(draft);
      toast.success('Ajuda ao vivo salva');
    } catch (error) {
      toast.error('Não foi possível salvar');
      console.error(error);
    } finally {
      setSaving(false);
    }
  };

  // "Usar este modelo" of the speed test: same save path as the Salvar button.
  const applyBenchmarkedModel = async (model: string) => {
    const next = { ...draft, model };
    setDraft(next);
    await saveSettings(next);
  };

  return (
    <>
    <div className="mx-auto mt-8 max-w-2xl space-y-6 rounded-xl border border-gray-200 bg-white p-6">
      <div>
        <h2 className="text-lg font-semibold text-gray-900">Ajuda ao vivo</h2>
        <p className="mt-1 text-sm text-gray-500">
          Sugestões durante a gravação. O padrão é o Ollama nesta máquina. Nada de áudio ou texto sai do computador, a menos que o provedor de resumo seja um serviço na nuvem.
        </p>
      </div>

      <label className="flex items-center justify-between gap-4">
        <span className="text-sm font-medium text-gray-800">Ativar ajuda ao vivo</span>
        <Switch checked={draft.enabled} onCheckedChange={(enabled) => setDraft({ ...draft, enabled })} />
      </label>

      <label className="block space-y-1">
        <span className="text-sm font-medium text-gray-800">Modelo</span>
        <input
          list="live-assist-models"
          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
          value={draft.model}
          onChange={(event) => setDraft({ ...draft, model: event.target.value })}
          placeholder="llama3.2:latest"
        />
        <datalist id="live-assist-models">
          {models.map((model) => <option key={model} value={model} />)}
        </datalist>
        <span className="text-xs text-gray-500">Padrão local: llama3.2:latest. Rode `ollama pull llama3.2` se o modelo ainda não estiver instalado.</span>
      </label>

      <label className="block space-y-1">
        <span className="text-sm font-medium text-gray-800">Idioma das sugestões</span>
        <select
          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
          value={draft.language}
          onChange={(event) => setDraft({ ...draft, language: event.target.value })}
        >
          <option value="pt-BR">Português (Brasil)</option>
          <option value="en">Inglês</option>
          <option value="es">Espanhol</option>
        </select>
      </label>

      <label className="block space-y-1">
        <span className="text-sm font-medium text-gray-800">Frequência</span>
        <select
          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
          value={draft.frequency}
          onChange={(event) => setDraft({ ...draft, frequency: event.target.value })}
        >
          <option value="baixa">Baixa</option>
          <option value="media">Média</option>
          <option value="alta">Alta</option>
        </select>
        <span className="text-xs text-gray-500">Intervalo mínimo entre sugestões: 8 s, 4 s ou 2 s. Uma pausa de 1,5 s também pode disparar uma sugestão.</span>
      </label>

      <label className="block space-y-1">
        <span className="text-sm font-medium text-gray-800">Contexto da reunião</span>
        <textarea
          className="min-h-28 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
          value={draft.meetingContext}
          onChange={(event) => setDraft({ ...draft, meetingContext: event.target.value })}
          placeholder="Pauta, nome do cliente, objetivo da conversa"
          maxLength={2000}
        />
      </label>

      {cloud && (
        <p className="rounded-md bg-amber-50 px-3 py-2 text-sm text-amber-800">
          O provedor de resumo atual é {providerLabel(summaryProvider.toLowerCase())}. O texto da transcrição será enviado a esse serviço. Para manter tudo local, escolha Ollama nas configurações de resumo. Se o campo de modelo continuar como llama3.2:latest, a ajuda ao vivo usa o modelo já salvo no resumo.
        </p>
      )}

      <button
        type="button"
        onClick={save}
        disabled={saving || !settingsLoaded}
        className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white disabled:opacity-60"
      >
        {saving ? 'Salvando…' : 'Salvar'}
      </button>
    </div>
    <ContextProfileCard />
    <SmartAssistCard />
    <ShortcutCard />
    <LatencyBenchmarkCard
      currentModel={settings.model}
      recording={isRecording || status.recording}
      onUseModel={applyBenchmarkedModel}
    />
    <VoiceModelCard />
    </>
  );
}
