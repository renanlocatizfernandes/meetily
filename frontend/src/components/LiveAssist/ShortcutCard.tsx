'use client';

import { useEffect, useState } from 'react';
import { AlertTriangle } from 'lucide-react';
import { toast } from 'sonner';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { SettingsCard } from '@/components/LiveAssist/SettingsCard';
import { useLiveAssist } from '@/contexts/LiveAssistContext';
import {
  DEFAULT_LIVE_ASSIST_SHORTCUT,
  acceleratorFromEvent,
  shortcutKeyCaps,
} from '@/lib/liveAssistShortcut';

function KeyCaps({ accelerator }: { accelerator: string }) {
  return (
    <span className="inline-flex items-center gap-1" aria-label={`Atalho ${shortcutKeyCaps(accelerator).join(' ')}`}>
      {shortcutKeyCaps(accelerator).map((cap, index) => (
        <kbd
          key={`${cap}-${index}`}
          className="inline-flex h-7 min-w-7 items-center justify-center rounded-md border border-border bg-muted px-2 font-sans text-sm font-medium text-foreground shadow-sm"
        >
          {cap}
        </kbd>
      ))}
    </span>
  );
}

export function ShortcutCard() {
  const { shortcut } = useLiveAssist();
  const [capturing, setCapturing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [hint, setHint] = useState<string | null>(null);

  const apply = async (accelerator: string) => {
    setSaving(true);
    try {
      const saved = await shortcut.update(accelerator);
      if (saved.registered) toast.success('Atalho salvo');
    } catch (error) {
      toast.error('Não foi possível salvar o atalho');
      console.error(error);
    } finally {
      setSaving(false);
    }
  };

  useEffect(() => {
    if (!capturing) return;
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      const result = acceleratorFromEvent(event);
      if (result.kind === 'cancel') {
        setCapturing(false);
        setHint(null);
      } else if (result.kind === 'invalid') {
        setHint(result.reason);
      } else if (result.kind === 'ok') {
        setCapturing(false);
        setHint(null);
        apply(result.accelerator);
      }
    };
    window.addEventListener('keydown', onKeyDown, true);
    return () => window.removeEventListener('keydown', onKeyDown, true);
    // `apply` only uses stable values while capturing.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [capturing]);

  if (!shortcut.available || !shortcut.status) {
    return (
      <SettingsCard
        title="Atalho global"
        description="Abre e fecha a ajuda ao vivo de qualquer app."
      >
        <div className="flex items-center gap-3">
          <KeyCaps accelerator={shortcut.accelerator} />
          <span className="text-sm text-muted-foreground">A troca de atalho não está disponível nesta versão.</span>
        </div>
      </SettingsCard>
    );
  }

  const { status } = shortcut;
  const isDefault = status.shortcut.toLowerCase() === DEFAULT_LIVE_ASSIST_SHORTCUT.toLowerCase();

  return (
    <SettingsCard
      title="Atalho global"
      description="Abre e fecha a ajuda ao vivo de qualquer app, mesmo com o Meetily Live Plus em segundo plano."
    >
      <div className="flex min-h-9 flex-wrap items-center gap-3">
        {capturing ? (
          <span
            className="inline-flex h-9 items-center rounded-md border border-dashed border-ring bg-muted px-3 text-sm text-foreground"
            role="status"
          >
            Pressione o novo atalho… (Esc cancela)
          </span>
        ) : (
          <KeyCaps accelerator={status.shortcut} />
        )}
      </div>

      {capturing && hint && <p className="text-sm text-destructive">{hint}</p>}

      {!capturing && !status.registered && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>O atalho não está ativo</AlertTitle>
          <AlertDescription>
            {status.error || 'Outro app já usa este atalho.'} Grave outro atalho ou feche o app que usa esta combinação.
          </AlertDescription>
        </Alert>
      )}

      <div className="flex flex-wrap gap-2">
        {capturing ? (
          <Button type="button" variant="outline" size="sm" onClick={() => { setCapturing(false); setHint(null); }}>
            Cancelar
          </Button>
        ) : (
          <Button type="button" variant="outline" size="sm" disabled={saving} onClick={() => { setHint(null); setCapturing(true); }}>
            Gravar novo atalho
          </Button>
        )}
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={saving || capturing || (isDefault && status.registered)}
          onClick={() => apply(DEFAULT_LIVE_ASSIST_SHORTCUT)}
        >
          Restaurar padrão
        </Button>
      </div>
    </SettingsCard>
  );
}
