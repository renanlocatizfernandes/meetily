'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import { SettingsCard, SettingSwitch } from '@/components/settings/SettingsCard';
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
    <SettingsCard
      title="Sugestões durante a gravação"
      description="O padrão é o Ollama nesta máquina. Nada de áudio ou texto sai do computador, a menos que o provedor de resumo seja um serviço na nuvem."
      className="mt-4 first:mt-0"
    >
      <div className="space-y-4">
        <SettingSwitch
          id="live-assist-enabled"
          label="Ativar ajuda ao vivo"
          checked={draft.enabled}
          onCheckedChange={(enabled) => setDraft({ ...draft, enabled })}
        />

        <div className="space-y-1.5">
          <Label htmlFor="live-assist-model" className="text-sm font-medium">Modelo</Label>
          <Input
            id="live-assist-model"
            list="live-assist-models"
            value={draft.model}
            onChange={(event) => setDraft({ ...draft, model: event.target.value })}
            placeholder="llama3.2:latest"
          />
          <datalist id="live-assist-models">
            {models.map((model) => <option key={model} value={model} />)}
          </datalist>
          <p className="text-[13px] text-muted-foreground">Padrão local: llama3.2:latest. Rode `ollama pull llama3.2` se o modelo ainda não estiver instalado.</p>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="live-assist-language" className="text-sm font-medium">Idioma das sugestões</Label>
          <Select value={draft.language} onValueChange={(language) => setDraft({ ...draft, language })}>
            <SelectTrigger id="live-assist-language">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="pt-BR">Português (Brasil)</SelectItem>
              <SelectItem value="en">Inglês</SelectItem>
              <SelectItem value="es">Espanhol</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="live-assist-frequency" className="text-sm font-medium">Frequência</Label>
          <Select value={draft.frequency} onValueChange={(frequency) => setDraft({ ...draft, frequency })}>
            <SelectTrigger id="live-assist-frequency">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="baixa">Baixa</SelectItem>
              <SelectItem value="media">Média</SelectItem>
              <SelectItem value="alta">Alta</SelectItem>
            </SelectContent>
          </Select>
          <p className="text-[13px] text-muted-foreground">Intervalo mínimo entre sugestões: 8 s, 4 s ou 2 s. Uma pausa de 1,5 s também pode disparar uma sugestão.</p>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="live-assist-context" className="text-sm font-medium">Contexto da reunião</Label>
          <Textarea
            id="live-assist-context"
            className="min-h-28"
            value={draft.meetingContext}
            onChange={(event) => setDraft({ ...draft, meetingContext: event.target.value })}
            placeholder="Pauta, nome do cliente, objetivo da conversa"
            maxLength={2000}
          />
        </div>

        {cloud && (
          <p className="rounded-md border border-warning/30 bg-warning-soft px-3 py-2 text-[13px] text-warning">
            O provedor de resumo atual é {providerLabel(summaryProvider.toLowerCase())}. O texto da transcrição será enviado a esse serviço. Para manter tudo local, escolha Ollama nas configurações de resumo. Se o campo de modelo continuar como llama3.2:latest, a ajuda ao vivo usa o modelo já salvo no resumo.
          </p>
        )}

        <Button type="button" size="sm" onClick={save} disabled={saving || !settingsLoaded}>
          {saving ? 'Salvando…' : 'Salvar'}
        </Button>
      </div>
    </SettingsCard>
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
