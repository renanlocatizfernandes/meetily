'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { Check, ChevronDown, Download, RefreshCw, X } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { useConfig } from '@/contexts/ConfigContext';
import { useLiveAssist } from '@/contexts/LiveAssistContext';
import { listenAll, safeEmit, safeInvoke } from '@/lib/tauriSafe';
import {
  ROLE_LABEL,
  actionFor,
  bytesText,
  downloadKey,
  fitText,
  hardwareText,
  memoryText,
  summariesUseOllama,
  usedInSummaries,
  type AdvisorEngine,
  type AdvisorModel,
  type AdvisorReport,
  type AdvisorRole,
  type AdvisorRoleReport,
} from '@/lib/modelAdvisor';

interface SummaryModelConfig {
  provider: string;
  model: string;
  whisperModel: string;
  apiKey?: string | null;
  ollamaEndpoint?: string | null;
}

interface ModelAdvisorCardProps {
  /** Functions to show. All three when omitted. */
  roles?: AdvisorRole[];
  /** Called after a model was put in use. */
  onChanged?: () => void;
}

const ALL_ROLES: AdvisorRole[] = ['speech', 'voices', 'assistant'];

/**
 * "Modelos recomendados para este Mac". Shows, for each function, the model in
 * use and the one suggested for this machine. Nothing is downloaded or
 * switched without a click on a button of this card.
 */
export function ModelAdvisorCard({ roles = ALL_ROLES, onChanged }: ModelAdvisorCardProps) {
  const { setTranscriptModelConfig } = useConfig();
  const { settings: liveSettings, saveSettings: saveLiveSettings } = useLiveAssist();
  const [report, setReport] = useState<AdvisorReport | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const [loadError, setLoadError] = useState(false);
  /** Download in progress by `engine:id`, 0 to 100. */
  const [downloads, setDownloads] = useState<Record<string, number>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const cancelled = useRef<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    const result = await safeInvoke<AdvisorReport>('model_advisor_get_report');
    if (result.ok && result.value) {
      setReport(result.value);
      setLoadError(false);
    } else if (!result.ok && result.missing) {
      setUnavailable(true);
    } else {
      setLoadError(true);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const setProgress = useCallback((key: string, percent: number | null) => {
    setDownloads((current) => {
      if (percent === null) {
        if (!(key in current)) return current;
        const next = { ...current };
        delete next[key];
        return next;
      }
      // Only downloads started from this card are shown here.
      if (!(key in current)) return current;
      return { ...current, [key]: Math.max(0, Math.min(100, Math.round(percent))) };
    });
  }, []);

  useEffect(() => {
    return listenAll({
      'model-download-progress': (payload: { modelName?: string; progress?: number }) => {
        if (payload?.modelName && typeof payload.progress === 'number') {
          setProgress(downloadKey('whisper', payload.modelName), payload.progress);
        }
      },
      'parakeet-model-download-progress': (payload: { modelName?: string; progress?: number }) => {
        if (payload?.modelName && typeof payload.progress === 'number') {
          setProgress(downloadKey('parakeet', payload.modelName), payload.progress);
        }
      },
      'voice-model-download-progress': (payload: { progress?: number; modelId?: string }) => {
        if (payload?.modelId && typeof payload.progress === 'number') {
          setProgress(downloadKey('voice', payload.modelId), payload.progress * 100);
        }
      },
      'model-advisor-ollama-progress': (payload: { modelName?: string; progress?: number }) => {
        if (payload?.modelName && typeof payload.progress === 'number') {
          setProgress(downloadKey('ollama', payload.modelName), payload.progress);
        }
      },
    });
  }, [setProgress]);

  /** Puts an installed model in use. Returns false (after a toast) when it fails. */
  const putInUse = async (model: AdvisorModel, current: AdvisorReport): Promise<boolean> => {
    if (model.engine === 'whisper' || model.engine === 'parakeet') {
      const provider = model.engine === 'whisper' ? 'localWhisper' : 'parakeet';
      const saved = await safeInvoke('api_save_transcript_config', { provider, model: model.id, apiKey: null });
      if (!saved.ok) {
        toast.error('Não foi possível trocar o modelo de transcrição', { description: saved.missing ? undefined : saved.error });
        return false;
      }
      setTranscriptModelConfig({ provider, model: model.id, apiKey: null });
      return true;
    }
    if (model.engine === 'voice') {
      const selected = await safeInvoke('select_voice_model', { modelId: model.id });
      if (!selected.ok) {
        toast.error('Não foi possível trocar o modelo de vozes', { description: selected.missing ? undefined : selected.error });
        return false;
      }
      if (!current.voiceModelEnabled) await safeInvoke('set_voice_model_enabled', { enabled: true });
      return true;
    }
    try {
      await saveLiveSettings({ ...liveSettings, model: model.id });
    } catch {
      toast.error('Não foi possível trocar o modelo da ajuda ao vivo');
      return false;
    }
    // Summaries that already run on a local Ollama model follow the change.
    if (summariesUseOllama(current) && !usedInSummaries(model, current)) await applyToSummaries(model, false);
    return true;
  };

  const applyToSummaries = async (model: AdvisorModel, announce: boolean): Promise<boolean> => {
    const loaded = await safeInvoke<SummaryModelConfig | null>('api_get_model_config');
    const previous = loaded.ok ? loaded.value : null;
    const config: SummaryModelConfig = {
      provider: 'ollama',
      model: model.id,
      whisperModel: previous?.whisperModel || 'large-v3',
      apiKey: null,
      ollamaEndpoint: previous?.ollamaEndpoint ?? null,
    };
    const saved = await safeInvoke('api_save_model_config', { ...config });
    if (!saved.ok) {
      toast.error('Não foi possível trocar o modelo dos resumos', { description: saved.missing ? undefined : saved.error });
      return false;
    }
    safeEmit('model-config-updated', config);
    if (announce) toast.success(`${model.name} em uso nos resumos`);
    return true;
  };

  const downloadCommand = (engine: AdvisorEngine, id: string): [string, Record<string, unknown>] => {
    if (engine === 'whisper') return ['whisper_download_model', { modelName: id }];
    if (engine === 'parakeet') return ['parakeet_download_model', { modelName: id }];
    if (engine === 'voice') return ['download_voice_model', { modelId: id }];
    return ['model_advisor_pull_ollama_model', { modelName: id }];
  };

  const downloadAndUse = async (model: AdvisorModel) => {
    if (!report) return;
    const key = downloadKey(model.engine, model.id);
    cancelled.current.delete(key);
    setDownloads((current) => ({ ...current, [key]: 0 }));
    if (model.engine === 'whisper') await safeInvoke('whisper_init');
    if (model.engine === 'parakeet') await safeInvoke('parakeet_init');
    const [command, args] = downloadCommand(model.engine, model.id);
    const result = await safeInvoke<{ installed?: boolean; cancelled?: boolean } | null>(command, args);
    setProgress(key, null);
    const wasCancelled = cancelled.current.delete(key) || (result.ok && result.value?.cancelled === true);
    if (wasCancelled) {
      await refresh();
      return;
    }
    if (!result.ok) {
      toast.error(`Não foi possível baixar ${model.name}`, { description: result.missing ? undefined : result.error });
      await refresh();
      return;
    }
    // The voice command resolves with the status: installed is false when the download did not finish.
    if (model.engine === 'voice' && result.value?.installed === false) {
      await refresh();
      return;
    }
    if (await putInUse(model, report)) {
      toast.success(`${model.name} baixado e em uso`);
      onChanged?.();
    }
    await refresh();
  };

  const cancel = async (model: AdvisorModel) => {
    const key = downloadKey(model.engine, model.id);
    cancelled.current.add(key);
    if (model.engine === 'whisper') await safeInvoke('whisper_cancel_download', { modelName: model.id });
    else if (model.engine === 'parakeet') await safeInvoke('parakeet_cancel_download', { modelName: model.id });
    else if (model.engine === 'voice') await safeInvoke('cancel_voice_model_download');
    else await safeInvoke('model_advisor_cancel_ollama_pull', { modelName: model.id });
  };

  const useNow = async (model: AdvisorModel) => {
    if (!report) return;
    setBusy(model.id);
    if (await putInUse(model, report)) {
      toast.success(`${model.name} em uso`);
      onChanged?.();
    }
    await refresh();
    setBusy(null);
  };

  if (unavailable) return null;

  const shown = report ? report.roles.filter((role) => roles.includes(role.role)) : [];

  const renderAction = (model: AdvisorModel, compact: boolean) => {
    if (!report) return null;
    const key = downloadKey(model.engine, model.id);
    const percent = downloads[key];
    if (percent !== undefined) {
      return (
        <div className="w-full space-y-2" role="status" aria-live="polite">
          <Progress value={percent} />
          <div className="flex items-center justify-between gap-3">
            <p className="text-sm text-muted-foreground">Baixando {model.name}… {percent}%</p>
            <Button type="button" variant="outline" size="sm" onClick={() => cancel(model)}>
              <X /> Cancelar
            </Button>
          </div>
        </div>
      );
    }
    if (model.downloading) {
      return <p className="text-sm text-muted-foreground">Download em andamento em outra tela.</p>;
    }
    const action = actionFor(model, report, liveSettings?.model);
    const summaryButton =
      model.role === 'assistant' && model.installed && !usedInSummaries(model, report) ? (
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy !== null}
          onClick={async () => {
            setBusy(model.id);
            if (await applyToSummaries(model, true)) onChanged?.();
            await refresh();
            setBusy(null);
          }}
        >
          Usar nos resumos
        </Button>
      ) : null;
    if (action === 'active') {
      return (
        <div className="flex flex-wrap items-center gap-2">
          <span className="inline-flex items-center gap-1.5 text-sm font-medium text-foreground">
            <Check className="h-4 w-4" aria-hidden /> {model.role === 'assistant' ? 'Em uso na ajuda ao vivo' : 'Em uso'}
          </span>
          {summaryButton}
        </div>
      );
    }
    if (action === 'use') {
      return (
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" variant={compact ? 'outline' : 'default'} size="sm" disabled={busy !== null} onClick={() => useNow(model)}>
            {model.role === 'assistant' ? 'Usar na ajuda ao vivo' : 'Usar'}
          </Button>
          {summaryButton}
        </div>
      );
    }
    if (action === 'download') {
      return (
        <Button type="button" variant={compact ? 'outline' : 'default'} size="sm" disabled={busy !== null} onClick={() => downloadAndUse(model)}>
          <Download /> Baixar e usar
        </Button>
      );
    }
    if (action === 'needsOllama') {
      return <p className="text-sm text-muted-foreground">Abra o Ollama neste computador para baixar este modelo.</p>;
    }
    return null;
  };

  const modelFacts = (model: AdvisorModel) =>
    [
      model.installed ? 'Instalado' : `Download de ${bytesText(model.downloadBytes)}`,
      `cerca de ${memoryText(model.memoryMb)} de memória`,
      model.license,
    ].join(' · ');

  const currentText = (role: AdvisorRoleReport): string => {
    if (role.role === 'assistant') {
      const summary = role.current ? role.current.name : 'não configurado';
      return `Ajuda ao vivo: ${liveSettings?.model || 'não configurado'} · Resumos: ${summary}`;
    }
    if (role.role === 'voices') return role.current ? role.current.name : 'Método simples, sem modelo de vozes';
    return role.current ? role.current.name : 'Não configurado';
  };

  return (
    <section className="mb-6 space-y-4 rounded-xl border border-border bg-card p-6 text-card-foreground">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold text-foreground">Modelos recomendados para este Mac</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            {report
              ? `${hardwareText(report.hardware)}. Os três modelos rodam ao mesmo tempo na reunião, então a sugestão é o melhor conjunto que cabe junto na memória.`
              : loadError
                ? 'Não foi possível ler as informações deste computador.'
                : 'Lendo as informações deste computador…'}
          </p>
        </div>
        <Button type="button" variant="ghost" size="sm" aria-label="Atualizar recomendações" onClick={() => refresh()}>
          <RefreshCw />
        </Button>
      </div>

      {shown.map((role) => {
        const recommended = role.recommended;
        const fitting = role.others.filter((other) => other.fit === 'fits' || other.installed);
        const rest = role.others.filter((other) => other.fit !== 'fits' && !other.installed);
        return (
          <div key={role.role} className="space-y-3 border-t border-border pt-4">
            <div>
              <h3 className="text-sm font-semibold text-foreground">{ROLE_LABEL[role.role]}</h3>
              <p className="mt-0.5 text-xs text-muted-foreground">Em uso agora: {currentText(role)}</p>
            </div>
            {recommended ? (
              <div className="space-y-2">
                <p className="text-sm text-foreground">
                  <span className="font-medium">Recomendado: {recommended.name}</span>
                  <span className="text-muted-foreground"> · {modelFacts(recommended)}</span>
                </p>
                <p className="text-sm text-muted-foreground">{role.reason}</p>
                {recommended.licenseNote && <p className="text-xs text-muted-foreground">{recommended.licenseNote}</p>}
                {renderAction(recommended, false)}
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">{role.reason}</p>
            )}
            {(fitting.length > 0 || rest.length > 0) && (
              <details className="group">
                <summary className="flex cursor-pointer list-none items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
                  <ChevronDown className="h-4 w-4 transition-transform group-open:rotate-180" aria-hidden /> Outras opções
                </summary>
                <ul className="mt-2 space-y-3">
                  {fitting.map((other) => (
                    <li key={other.id} className="space-y-1.5 rounded-lg border border-border p-3">
                      <p className="text-sm text-foreground">
                        <span className="font-medium">{other.name}</span>
                        <span className="text-muted-foreground"> · {modelFacts(other)}</span>
                      </p>
                      <p className="text-xs text-muted-foreground">
                        {other.about}
                        {other.fit !== 'fits' ? ` ${fitText(other.fit)}` : ''}
                      </p>
                      {renderAction(other, true)}
                    </li>
                  ))}
                  {rest.map((other) => (
                    <li key={other.id} className="rounded-lg border border-border p-3 opacity-70">
                      <p className="text-sm text-foreground">
                        <span className="font-medium">{other.name}</span>
                        <span className="text-muted-foreground"> · {modelFacts(other)}</span>
                      </p>
                      <p className="text-xs text-muted-foreground">{fitText(other.fit)}</p>
                    </li>
                  ))}
                </ul>
              </details>
            )}
          </div>
        );
      })}

      {report && shown.some((role) => role.role === 'assistant') && (
        <p className="border-t border-border pt-3 text-xs text-muted-foreground">
          {report.ollama.running
            ? 'Os modelos de organização, ajuda e resumo são baixados pelo Ollama deste computador, sem custo. "Baixar e usar" troca o modelo da ajuda ao vivo; os resumos só mudam junto quando já usam um modelo local.'
            : 'O Ollama não respondeu neste computador. Abra o Ollama para baixar ou trocar os modelos de organização, ajuda e resumo.'}
        </p>
      )}
    </section>
  );
}
