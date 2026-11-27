'use client';

import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { SettingsCard } from '@/components/settings/SettingsCard';
import { safeInvoke } from '@/lib/tauriSafe';

/** Mirrors `jev::settings::PublicSettings` (the key itself never comes back). */
export interface JevPublicSettings {
  enabled: boolean;
  baseUrl: string;
  model: string;
  hasKey: boolean;
  defaultBaseUrl: string;
  defaultModel: string;
}

/** Mirrors `jev::TestResult`. */
export interface JevTestResult {
  ok: boolean;
  message: string;
  models: string[];
  modelListed: boolean | null;
  keyVerified: boolean;
  inputTokens: number | null;
  latencyMs: number;
}

interface Draft {
  enabled: boolean;
  baseUrl: string;
  model: string;
  apiKey: string;
}

const draftFrom = (s: JevPublicSettings): Draft => ({ enabled: s.enabled, baseUrl: s.baseUrl, model: s.model, apiKey: '' });

/**
 * Settings → Beta: optional connection to Jev (TypeSafe AI's System One
 * decision model) or a compatible server. Off by default; paid per input token.
 */
export function JevSettings() {
  const [saved, setSaved] = useState<JevPublicSettings | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [available, setAvailable] = useState(true);
  const [busy, setBusy] = useState<'save' | 'test' | 'verify' | null>(null);
  const [result, setResult] = useState<JevTestResult | null>(null);

  useEffect(() => {
    let active = true;
    safeInvoke<JevPublicSettings>('jev_get_settings').then((res) => {
      if (!active) return;
      if (res.ok && res.value) {
        setSaved(res.value);
        setDraft(draftFrom(res.value));
      } else {
        setAvailable(false);
      }
    });
    return () => {
      active = false;
    };
  }, []);

  if (!available) return null;

  const dirty =
    !!saved &&
    !!draft &&
    (draft.enabled !== saved.enabled || draft.baseUrl !== saved.baseUrl || draft.model !== saved.model || draft.apiKey.trim() !== '');

  const persist = async (next: Draft, clearKey = false): Promise<boolean> => {
    const res = await safeInvoke<JevPublicSettings>('jev_save_settings', {
      enabled: next.enabled,
      baseUrl: next.baseUrl,
      model: next.model,
      apiKey: next.apiKey.trim() ? next.apiKey.trim() : null,
      clearKey,
    });
    if (res.ok && res.value) {
      setSaved(res.value);
      setDraft(draftFrom(res.value));
      return true;
    }
    toast.error(res.ok ? 'Não foi possível salvar a conexão com o Jev' : res.error || 'Não foi possível salvar a conexão com o Jev');
    return false;
  };

  const save = async () => {
    if (!draft) return;
    setBusy('save');
    if (await persist(draft)) toast.success('Conexão com o Jev salva');
    setBusy(null);
  };

  const toggle = async (enabled: boolean) => {
    if (!draft) return;
    setBusy('save');
    await persist({ ...draft, enabled });
    setBusy(null);
  };

  const clearKey = async () => {
    if (!draft) return;
    setBusy('save');
    if (await persist({ ...draft, apiKey: '' }, true)) toast.success('Chave removida');
    setBusy(null);
  };

  // Saves pending edits first, so the test always uses what is on screen.
  const test = async (verifyKey: boolean) => {
    if (!draft) return;
    setBusy(verifyKey ? 'verify' : 'test');
    setResult(null);
    if (!dirty || (await persist(draft))) {
      const res = await safeInvoke<JevTestResult>('jev_test_connection', { verifyKey });
      if (res.ok && res.value) setResult(res.value);
      else toast.error('Não foi possível testar a conexão');
    }
    setBusy(null);
  };

  const disabled = !draft || busy !== null;

  return (
    <SettingsCard
      title="Jev (TypeSafe) — decisões rápidas"
      description="Conexão opcional com o Jev, o modelo de decisão “System One” da TypeSafe AI. Em vez de texto, ele devolve respostas tipadas com probabilidade (sim/não, escolha, nota), o que deixa decisões como “vale sugerir algo agora?” mais baratas que um LLM. Serviço pago por token de entrada; também aceita um servidor compatível neste computador."
      className="mt-4 first:mt-0"
      action={
        <Switch
          checked={draft?.enabled ?? false}
          onCheckedChange={toggle}
          disabled={disabled}
          aria-label="Usar o Jev"
        />
      }
    >
      <div className="space-y-4">
      <div className="space-y-3">
        <label className="block space-y-1">
          <span className="block text-sm font-medium text-foreground">Endereço</span>
          <Input
            value={draft?.baseUrl ?? ''}
            placeholder={saved?.defaultBaseUrl ?? 'https://api.typesafe.ai'}
            onChange={(e) => draft && setDraft({ ...draft, baseUrl: e.target.value })}
            disabled={disabled}
            spellCheck={false}
            autoComplete="off"
          />
          <span className="block text-[13px] text-muted-foreground">
            Padrão: {saved?.defaultBaseUrl ?? 'https://api.typesafe.ai'}. Endereço http:// só é aceito para localhost.
          </span>
        </label>

        <label className="block space-y-1">
          <span className="block text-sm font-medium text-foreground">Modelo</span>
          <Input
            value={draft?.model ?? ''}
            placeholder={saved?.defaultModel ?? 'jev-latest'}
            onChange={(e) => draft && setDraft({ ...draft, model: e.target.value })}
            disabled={disabled}
            spellCheck={false}
            autoComplete="off"
          />
        </label>

        <label className="block space-y-1">
          <span className="block text-sm font-medium text-foreground">Chave da API</span>
          <Input
            type="password"
            value={draft?.apiKey ?? ''}
            placeholder={saved?.hasKey ? 'Chave salva (digite para trocar)' : 'Cole a chave da TypeSafe'}
            onChange={(e) => draft && setDraft({ ...draft, apiKey: e.target.value })}
            disabled={disabled}
            spellCheck={false}
            autoComplete="off"
          />
          <span className="block text-[13px] text-muted-foreground">
            Fica só neste computador, no arquivo de configurações do app, e nunca volta para a tela.
          </span>
        </label>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" onClick={save} disabled={disabled || !dirty}>
          {busy === 'save' ? 'Salvando…' : 'Salvar'}
        </Button>
        <Button size="sm" variant="outline" onClick={() => test(false)} disabled={disabled}>
          {busy === 'test' ? 'Testando…' : 'Testar conexão'}
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => test(true)}
          disabled={disabled || (!saved?.hasKey && !draft?.apiKey.trim())}
          title="Faz uma chamada mínima (poucos tokens de entrada)"
        >
          {busy === 'verify' ? 'Validando…' : 'Validar chave'}
        </Button>
        {saved?.hasKey && (
          <Button size="sm" variant="ghost" onClick={clearKey} disabled={disabled}>
            Remover chave
          </Button>
        )}
      </div>

      {result && (
        <div
          role="status"
          className={`rounded-lg border p-3 text-sm ${result.ok ? 'border-green-600/40 text-foreground' : 'border-destructive/50 text-destructive'}`}
        >
          <p>{result.message}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {result.latencyMs} ms
            {result.inputTokens != null && ` · ${result.inputTokens} tokens de entrada`}
            {result.models.length > 0 && ` · modelos: ${result.models.slice(0, 6).join(', ')}${result.models.length > 6 ? '…' : ''}`}
          </p>
        </div>
      )}

      <ul className="list-disc space-y-1 pl-5 text-[13px] text-muted-foreground">
        <li>“Testar conexão” só lista os modelos e não gasta créditos. “Validar chave” envia uma frase de teste.</li>
        <li>Desligado, o app nunca chama o Jev. Ligado, recursos que usam o Jev enviam a ele trechos da transcrição: é um serviço de terceiros.</li>
        <li>Sem o Jev, tudo continua funcionando com os modelos locais.</li>
      </ul>
      </div>
    </SettingsCard>
  );
}
