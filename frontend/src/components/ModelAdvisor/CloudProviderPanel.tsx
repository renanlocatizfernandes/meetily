'use client';

import { useEffect, useState } from 'react';
import { CheckCircle2, ChevronDown, ChevronUp, Loader2, Plus, Trash2, XCircle, Zap } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useConfig } from '@/contexts/ConfigContext';
import { configService, ProviderConnectionResult } from '@/services/configService';
import { REASONING_EFFORTS, ReasoningEffort } from '@/lib/llmProviders';

const FALLBACK_PROVIDERS: { value: string; label: string }[] = [
  { value: 'claude', label: 'Claude (Anthropic)' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'openrouter', label: 'OpenRouter' },
  { value: 'ollama', label: 'Ollama (local)' },
  { value: 'builtin-ai', label: 'IA embutida (local)' },
];

/** "Testar conexão" button for a cloud provider's key. */
export function TestConnectionButton({
  provider,
  apiKey,
}: {
  provider: 'claude' | 'openai' | 'openrouter';
  apiKey: string | null;
}) {
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<ProviderConnectionResult | null>(null);

  // A result is only valid for the provider and key it was tested with.
  useEffect(() => {
    setResult(null);
  }, [provider, apiKey]);

  const run = async () => {
    setTesting(true);
    setResult(null);
    try {
      setResult(await configService.testProviderConnection(provider, apiKey));
    } catch (err) {
      setResult({
        ok: false,
        provider,
        status: null,
        latency_ms: 0,
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="space-y-2">
      <Button type="button" variant="outline" size="sm" onClick={run} disabled={testing || !apiKey?.trim()}>
        {testing ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Zap className="mr-2 h-4 w-4" />}
        Testar conexão
      </Button>
      {result && (
        <p
          role="status"
          className={`flex items-start gap-2 text-sm ${result.ok ? 'text-success' : 'text-danger'}`}
        >
          {result.ok ? (
            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" />
          ) : (
            <XCircle className="mt-0.5 h-4 w-4 shrink-0" />
          )}
          <span>{result.message}</span>
        </p>
      )}
    </div>
  );
}

/**
 * Optional extras for cloud providers: reasoning effort and a fallback chain.
 * Everything here is off by default; nothing paid is enabled automatically.
 */
export function CloudProviderExtras() {
  const config = useConfig();
  const [open, setOpen] = useState(false);
  const [newProvider, setNewProvider] = useState('openrouter');
  const [newModel, setNewModel] = useState('');
  if (!config) return null;
  const { fallbackChain, setFallbackChain, reasoningEffort, setReasoningEffort } = config;

  const addEntry = () => {
    const model = newModel.trim();
    if (!model) return;
    if (fallbackChain.entries.some((entry) => entry.provider === newProvider && entry.model === model)) {
      setNewModel('');
      return;
    }
    setFallbackChain({
      ...fallbackChain,
      entries: [...fallbackChain.entries, { provider: newProvider, model }],
    });
    setNewModel('');
  };

  const removeEntry = (index: number) => {
    const entries = fallbackChain.entries.filter((_, i) => i !== index);
    setFallbackChain({ enabled: fallbackChain.enabled && entries.length > 0, entries });
  };

  return (
    <div className="border-t pt-3">
      <button
        type="button"
        className="flex w-full items-center justify-between py-1 text-left"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
      >
        <Label className="cursor-pointer">Recursos avançados (opcional)</Label>
        {open ? <ChevronUp className="h-4 w-4 text-muted-foreground" /> : <ChevronDown className="h-4 w-4 text-muted-foreground" />}
      </button>

      {open && (
        <div className="mt-3 space-y-5">
          <div className="space-y-1">
            <Label>Esforço de raciocínio</Label>
            <Select value={reasoningEffort} onValueChange={(v) => setReasoningEffort(v as ReasoningEffort)}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {REASONING_EFFORTS.map((o) => (
                  <SelectItem key={o.value} value={o.value}>
                    {o.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              Só é enviado a modelos que aceitam o parâmetro. Mais esforço custa mais e demora mais.
            </p>
          </div>

          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label>Alternativas se o provedor falhar</Label>
              <Switch
                checked={fallbackChain.enabled}
                disabled={fallbackChain.entries.length === 0}
                onCheckedChange={(enabled) => setFallbackChain({ ...fallbackChain, enabled })}
                aria-label="Ativar alternativas"
              />
            </div>
            <p className="text-xs text-muted-foreground">
              Desligado por padrão. Quando ligado, a ordem abaixo é seguida se o provedor principal falhar. Provedores
              pagos sem chave configurada são ignorados. Se o provedor principal for local, as alternativas na nuvem também são
              ignoradas, para o texto não sair da máquina sem aviso. Coloque um modelo local por último para sempre ter resposta.
            </p>
            <ol className="space-y-1">
              {fallbackChain.entries.map((e, i) => (
                <li key={`${e.provider}-${e.model}-${i}`} className="flex items-center gap-2 text-sm">
                  <span className="w-5 text-muted-foreground">{i + 1}.</span>
                  <span className="flex-1 truncate">
                    {FALLBACK_PROVIDERS.find((p) => p.value === e.provider)?.label ?? e.provider} · {e.model}
                  </span>
                  <Button type="button" variant="ghost" size="icon" onClick={() => removeEntry(i)} aria-label={`Remover ${e.model}`}>
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </li>
              ))}
            </ol>
            <div className="flex gap-2">
              <Select value={newProvider} onValueChange={setNewProvider}>
                <SelectTrigger className="w-[170px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {FALLBACK_PROVIDERS.map((p) => (
                    <SelectItem key={p.value} value={p.value}>
                      {p.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Input
                value={newModel}
                onChange={(e) => setNewModel(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
                    e.preventDefault();
                    addEntry();
                  }
                }}
                placeholder="ID do modelo"
                aria-label="ID do modelo alternativo"
              />
              <Button type="button" variant="outline" size="icon" onClick={addEntry} aria-label="Adicionar alternativa">
                <Plus className="h-4 w-4" />
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
