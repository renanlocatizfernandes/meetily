'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { Download, Trash2, X } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { Switch } from '@/components/ui/switch';
import { SettingsCard } from '@/components/LiveAssist/SettingsCard';
import { listenAll, safeInvoke } from '@/lib/tauriSafe';

interface VoiceModelStatus {
  installed: boolean;
  enabled: boolean;
  downloading: boolean;
  /** 0..1 */
  progress: number;
  sizeMb: number | null;
  modelName: string | null;
  error: string | null;
}

function sizeText(sizeMb: number | null | undefined): string {
  if (!sizeMb || !Number.isFinite(sizeMb)) return '';
  return sizeMb >= 1000
    ? `${(sizeMb / 1024).toLocaleString('pt-BR', { maximumFractionDigits: 1 })} GB`
    : `${Math.round(sizeMb)} MB`;
}

export function VoiceModelCard() {
  const [status, setStatus] = useState<VoiceModelStatus | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  // A download that ends because the user cancelled is not an error worth a toast.
  const cancelRequested = useRef(false);

  const refresh = useCallback(async () => {
    const result = await safeInvoke<VoiceModelStatus>('get_voice_model_status');
    if (result.ok && result.value) setStatus(result.value);
    else if (!result.ok && result.missing) setUnavailable(true);
    return result.ok;
  }, []);

  useEffect(() => {
    refresh();
    return listenAll({
      'voice-model-status': (payload: VoiceModelStatus) => {
        if (payload && typeof payload.installed === 'boolean') setStatus(payload);
      },
      'voice-model-download-progress': (payload: { progress: number }) => {
        if (!payload || typeof payload.progress !== 'number') return;
        setStatus((current) => (current ? { ...current, downloading: true, error: null, progress: payload.progress } : current));
      },
    });
  }, [refresh]);

  /** Runs a command; uses the returned status when there is one, otherwise reads it again. */
  const run = async (command: string, args: Record<string, unknown> | undefined, failure: string) => {
    setBusy(true);
    const result = await safeInvoke<VoiceModelStatus | null>(command, args);
    if (result.ok && result.value && typeof result.value.installed === 'boolean') {
      setStatus(result.value);
    } else {
      const silenced = command === 'download_voice_model' && cancelRequested.current;
      if (!result.ok && !silenced) toast.error(failure, { description: result.missing ? undefined : result.error });
      await refresh();
    }
    setBusy(false);
  };

  const download = () => {
    cancelRequested.current = false;
    setStatus((current) => (current ? { ...current, downloading: true, progress: 0, error: null } : current));
    run('download_voice_model', undefined, 'Não foi possível baixar o modelo de vozes');
  };

  if (unavailable) return null;

  const size = sizeText(status?.sizeMb);
  const percent = Math.max(0, Math.min(100, Math.round((status?.progress ?? 0) * 100)));

  return (
    <SettingsCard
      title="Separação de vozes"
      description={
        <>
          Um modelo opcional{size ? ` (cerca de ${size})` : ''} que roda nesta máquina e melhora a separação de quem está falando. O áudio não sai do computador.
        </>
      }
    >
      {!status ? (
        <p className="text-sm text-muted-foreground">Lendo o estado do modelo…</p>
      ) : status.downloading ? (
        <div className="space-y-2" role="status" aria-live="polite">
          <Progress value={percent} />
          <div className="flex items-center justify-between gap-3">
            <p className="text-sm text-muted-foreground">
              Baixando{status.modelName ? ` ${status.modelName}` : ''}… {percent}%
            </p>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                cancelRequested.current = true;
                run('cancel_voice_model_download', undefined, 'Não foi possível cancelar o download');
              }}
            >
              <X /> Cancelar
            </Button>
          </div>
        </div>
      ) : status.installed ? (
        <>
          <label className="flex items-center justify-between gap-4">
            <span>
              <span className="block text-sm font-medium text-foreground">Usar o modelo de vozes</span>
              <span className="block text-xs text-muted-foreground">
                {status.modelName ? `${status.modelName} instalado` : 'Modelo instalado'}{size ? ` · ${size}` : ''}
              </span>
            </span>
            <Switch
              checked={status.enabled}
              disabled={busy}
              onCheckedChange={(enabled) => {
                setStatus({ ...status, enabled });
                run('set_voice_model_enabled', { enabled }, 'Não foi possível salvar');
              }}
            />
          </label>
          {!status.enabled && (
            <p className="text-sm text-muted-foreground">
              Desligado: a separação de vozes usa o método simples, que erra mais quando as vozes são parecidas.
            </p>
          )}
          <div className="flex flex-wrap items-center gap-2">
            {confirmRemove ? (
              <>
                <span className="text-sm text-foreground">Remover o modelo desta máquina?</span>
                <Button
                  type="button"
                  variant="destructive"
                  size="sm"
                  disabled={busy}
                  onClick={() => {
                    setConfirmRemove(false);
                    run('delete_voice_model', undefined, 'Não foi possível remover o modelo');
                  }}
                >
                  Remover
                </Button>
                <Button type="button" variant="ghost" size="sm" onClick={() => setConfirmRemove(false)}>
                  Manter
                </Button>
              </>
            ) : (
              <Button type="button" variant="ghost" size="sm" disabled={busy} onClick={() => setConfirmRemove(true)}>
                <Trash2 /> Remover
              </Button>
            )}
          </div>
        </>
      ) : (
        <>
          <p className="text-sm text-muted-foreground">
            Sem o modelo, a separação de vozes continua funcionando pelo método simples, que erra mais quando as vozes são parecidas.
          </p>
          <div>
            <Button type="button" size="sm" disabled={busy} onClick={download}>
              <Download /> Baixar{size ? ` (${size})` : ''}
            </Button>
          </div>
        </>
      )}

      {status?.error && !status.downloading && (
        <p className="text-sm text-destructive">{status.error}</p>
      )}
    </SettingsCard>
  );
}
