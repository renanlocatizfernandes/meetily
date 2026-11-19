'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import { Check, Gauge, X } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { SettingsCard } from '@/components/LiveAssist/SettingsCard';
import { listenAll, safeInvoke } from '@/lib/tauriSafe';

interface BenchmarkResult {
  model: string;
  firstTokenMs: number | null;
  totalMs: number | null;
  ok: boolean;
  error: string | null;
}

interface BenchmarkProgress {
  model: string;
  index: number;
  total: number;
}

// A first answer slower than this is too late to help during a conversation.
const GOOD_FIRST_TOKEN_MS = 3000;

function seconds(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms)) return '—';
  return `${(ms / 1000).toLocaleString('pt-BR', { minimumFractionDigits: 1, maximumFractionDigits: 1 })} s`;
}

function byFirstToken(a: BenchmarkResult, b: BenchmarkResult): number {
  const rank = (result: BenchmarkResult) => (result.ok && result.firstTokenMs !== null ? result.firstTokenMs : Number.POSITIVE_INFINITY);
  return rank(a) - rank(b) || (a.totalMs ?? Number.POSITIVE_INFINITY) - (b.totalMs ?? Number.POSITIVE_INFINITY);
}

export function LatencyBenchmarkCard({
  currentModel,
  recording,
  onUseModel,
}: {
  currentModel: string;
  recording: boolean;
  /** Saves the model through the settings form of the live assist. */
  onUseModel: (model: string) => Promise<void>;
}) {
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<BenchmarkProgress | null>(null);
  const [results, setResults] = useState<BenchmarkResult[] | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [applying, setApplying] = useState<string | null>(null);
  const cancelled = useRef(false);

  useEffect(() => {
    return listenAll({
      'live-assist-benchmark-progress': (payload: BenchmarkProgress) => {
        if (payload && typeof payload.model === 'string') setProgress(payload);
      },
    });
  }, []);

  const sorted = useMemo(() => (results ? [...results].sort(byFirstToken) : []), [results]);
  const fastest = sorted.find((result) => result.ok && result.firstTokenMs !== null) ?? null;
  const fastEnough = fastest !== null && (fastest.firstTokenMs ?? Infinity) <= GOOD_FIRST_TOKEN_MS;

  const run = async () => {
    cancelled.current = false;
    setRunning(true);
    setProgress(null);
    setMessage(null);
    const result = await safeInvoke<BenchmarkResult[]>('benchmark_live_assist_models', { models: null });
    setRunning(false);
    setProgress(null);
    if (result.ok) {
      const list = Array.isArray(result.value) ? result.value : [];
      setResults(list);
      if (cancelled.current) setMessage(list.length > 0 ? 'Teste cancelado. Resultados parciais abaixo.' : 'Teste cancelado.');
      else if (list.length === 0) setMessage('Nenhum modelo instalado para testar.');
      return;
    }
    if (result.missing) {
      setUnavailable(true);
      return;
    }
    setMessage(cancelled.current ? 'Teste cancelado.' : `Não foi possível concluir o teste: ${result.error}`);
  };

  const cancel = async () => {
    cancelled.current = true;
    await safeInvoke('cancel_live_assist_benchmark');
  };

  const applyModel = async (model: string) => {
    setApplying(model);
    try {
      await onUseModel(model);
      toast.success(`Ajuda ao vivo agora usa ${model}`);
    } catch (error) {
      toast.error('Não foi possível salvar o modelo');
      console.error(error);
    } finally {
      setApplying(null);
    }
  };

  const percent = progress && progress.total > 0
    ? Math.min(100, Math.round((Math.min(progress.index, progress.total) / progress.total) * 100))
    : 0;

  return (
    <SettingsCard
      title="Testar velocidade dos modelos"
      description="Mede quanto tempo cada modelo instalado leva para começar a responder nesta máquina. Quanto menor a primeira resposta, mais cedo a sugestão aparece."
    >
      {unavailable ? (
        <p className="text-sm text-muted-foreground">O teste de velocidade não está disponível nesta versão.</p>
      ) : (
        <>
          <div className="flex min-h-9 flex-wrap items-center gap-3">
            {running ? (
              <Button type="button" variant="outline" size="sm" onClick={cancel}>
                <X /> Cancelar
              </Button>
            ) : (
              <Button type="button" size="sm" onClick={run} disabled={recording}>
                <Gauge /> {results ? 'Testar de novo' : 'Testar agora'}
              </Button>
            )}
            {recording && !running && (
              <span className="text-sm text-muted-foreground">
                Indisponível durante a gravação: o teste disputaria o modelo com as sugestões.
              </span>
            )}
          </div>

          {running && (
            <div className="space-y-2" role="status" aria-live="polite">
              <Progress value={percent} />
              <p className="text-sm text-muted-foreground">
                {progress
                  ? `Testando ${progress.model} (${Math.min(progress.index + 1, progress.total)} de ${progress.total})…`
                  : 'Preparando o teste…'}
              </p>
            </div>
          )}

          {message && <p className="text-sm text-muted-foreground">{message}</p>}

          {!running && sorted.length > 0 && (
            <>
              <div className="overflow-x-auto rounded-lg border border-border">
                <table className="w-full text-left text-sm">
                  <thead className="bg-muted text-xs text-muted-foreground">
                    <tr>
                      <th scope="col" className="px-3 py-2 font-medium">Modelo</th>
                      <th scope="col" className="px-3 py-2 text-right font-medium">Primeira resposta</th>
                      <th scope="col" className="px-3 py-2 text-right font-medium">Total</th>
                      <th scope="col" className="px-3 py-2 font-medium">Status</th>
                      <th scope="col" className="px-3 py-2"><span className="sr-only">Ação</span></th>
                    </tr>
                  </thead>
                  <tbody>
                    {sorted.map((result) => {
                      const inUse = result.model === currentModel;
                      return (
                        <tr key={result.model} className="border-t border-border">
                          <td className="px-3 py-2 font-medium text-foreground">
                            {result.model}
                            {result === fastest && (
                              <span className="ml-2 rounded-full bg-primary px-2 py-0.5 text-xs font-medium text-primary-foreground">Mais rápido</span>
                            )}
                          </td>
                          <td className="px-3 py-2 text-right tabular-nums">{seconds(result.firstTokenMs)}</td>
                          <td className="px-3 py-2 text-right tabular-nums">{seconds(result.totalMs)}</td>
                          <td className={result.ok ? 'px-3 py-2 text-muted-foreground' : 'px-3 py-2 text-destructive'}>
                            {result.ok ? 'OK' : result.error || 'Falhou'}
                          </td>
                          <td className="px-3 py-2 text-right">
                            {inUse ? (
                              <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                                <Check className="h-3.5 w-3.5" /> Em uso
                              </span>
                            ) : result.ok ? (
                              <Button
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={applying !== null}
                                onClick={() => applyModel(result.model)}
                              >
                                {applying === result.model ? 'Salvando…' : 'Usar este modelo'}
                              </Button>
                            ) : null}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>

              <p className="text-sm text-foreground">
                {fastest === null
                  ? 'Nenhum modelo respondeu. Confira se o Ollama está aberto e tente de novo.'
                  : fastEnough
                    ? `Recomendado: ${fastest.model}, com a primeira resposta em ${seconds(fastest.firstTokenMs)}.`
                    : `Recomendado: ${fastest.model}, o mais rápido aqui (${seconds(fastest.firstTokenMs)}). Ainda passa de 3 s: as sugestões podem chegar atrasadas. Um modelo menor responde mais cedo.`}
              </p>
            </>
          )}
        </>
      )}
    </SettingsCard>
  );
}
