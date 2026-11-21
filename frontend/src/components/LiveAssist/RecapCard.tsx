'use client';

import { useState } from 'react';
import { History, Loader2 } from 'lucide-react';
import { safeInvoke } from '@/lib/tauriSafe';
import { RECAP_MINUTES, RecapResult, recapCostNote } from '@/lib/liveAssistSmart';

/** "O que eu perdi?": summary of the last minutes, only when the owner asks. */
export function RecapCard({ disabled, paid }: { disabled: boolean; paid: boolean }) {
  const [minutes, setMinutes] = useState<number>(5);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<RecapResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const ask = async () => {
    setBusy(true);
    setError(null);
    const response = await safeInvoke<RecapResult>('live_assist_recap', { minutes });
    setBusy(false);
    if (response.ok) setResult(response.value);
    else setError(response.error || 'Não foi possível resumir agora.');
  };

  return (
    <section className="rounded-lg border border-border bg-card p-3 text-card-foreground" aria-label="O que eu perdi?">
      <div className="flex flex-wrap items-center gap-2">
        <History className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
        <h3 className="text-sm font-semibold text-foreground">O que eu perdi?</h3>
        <select
          className="ml-auto rounded border border-border bg-background px-1.5 py-1 text-xs"
          value={minutes}
          onChange={(event) => setMinutes(Number(event.target.value))}
          aria-label="Período do resumo"
        >
          {RECAP_MINUTES.map((value) => (
            <option key={value} value={value}>
              Últimos {value} min
            </option>
          ))}
        </select>
        <button
          type="button"
          onClick={ask}
          disabled={disabled || busy}
          className="inline-flex items-center gap-1 rounded bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground disabled:opacity-50"
          title={paid ? 'Chama o modelo na nuvem uma vez e gasta tokens.' : 'Chama o modelo local uma vez.'}
        >
          {busy && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
          Resumir
        </button>
      </div>
      {paid && !result && <p className="mt-1 text-[11px] text-muted-foreground">Usa o provedor na nuvem: gasta tokens a cada pedido.</p>}
      {error && <p className="mt-2 text-xs text-amber-700">{error}</p>}
      {result && (
        <div className="mt-2">
          {result.bullets.length === 0 ? (
            <p className="text-sm text-muted-foreground">Nada de novo nesse período.</p>
          ) : (
            <ul className="list-disc space-y-1 pl-5 text-sm text-foreground">
              {result.bullets.map((bullet, index) => (
                <li key={index}>{bullet}</li>
              ))}
            </ul>
          )}
          <p className="mt-1 text-[11px] text-muted-foreground">
            Últimos {result.minutes} min · {recapCostNote(result)}
          </p>
        </div>
      )}
    </section>
  );
}
