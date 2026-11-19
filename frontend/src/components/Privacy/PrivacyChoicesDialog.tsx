'use client';

import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Switch } from '@/components/ui/switch';
import {
  AutoRecordMode,
  AutoRecordModeControl,
  isAutoRecordMode,
} from '@/components/AutoRecord/AutoRecordModeControl';
import { useFeatures } from '@/contexts/FeatureContext';
import { safeInvoke } from '@/lib/tauriSafe';

interface PrivacyOnboarding {
  done: boolean;
  autoRecordMode: AutoRecordMode;
  internetLookup: boolean;
}

/**
 * Asked once, after the first-run setup: how recordings start and whether cited terms
 * may be looked up on the internet. It cannot be closed without saving.
 */
export function PrivacyChoicesDialog() {
  const { toggles, loaded: togglesLoaded, setToggle } = useFeatures();
  const [open, setOpen] = useState(false);
  // Safe defaults, whatever the stored values are: ask before recording, no internet lookup.
  const [mode, setMode] = useState<AutoRecordMode>('ask');
  const [internetLookup, setInternetLookup] = useState(false);
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    safeInvoke<PrivacyOnboarding>('get_privacy_onboarding').then((result) => {
      // Command missing or failing: nothing to ask in this build.
      if (cancelled || !result.ok || !result.value || result.value.done !== false) return;
      setOpen(true);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const save = async () => {
    setSaving(true);
    setFailure(null);
    const result = await safeInvoke<PrivacyOnboarding>('complete_privacy_onboarding', {
      autoRecordMode: mode,
      internetLookup,
    });
    setSaving(false);
    if (!result.ok && result.missing) {
      // This build cannot store the answer; do not trap the user in the dialog.
      setOpen(false);
      return;
    }
    if (!result.ok) {
      setFailure('Não foi possível salvar. Tente de novo.');
      return;
    }
    const saved = result.value;
    const lookup = typeof saved?.internetLookup === 'boolean' ? saved.internetLookup : internetLookup;
    // Keeps the "Pesquisa" switch of the resources bar in step with this answer.
    if (togglesLoaded && toggles.internetSearch !== lookup) {
      setToggle({ internetSearch: lookup }).catch(() => undefined);
    }
    setOpen(false);
    toast.success('Escolhas de privacidade salvas', {
      description: 'Você pode mudar isso nas configurações.',
    });
    // Lets the settings card of the automatic recording refresh if it is on screen.
    window.dispatchEvent(new CustomEvent('auto-record-settings-changed', {
      detail: { mode: isAutoRecordMode(saved?.autoRecordMode) ? saved.autoRecordMode : mode },
    }));
  };

  if (!open) return null;

  return (
    <Dialog open onOpenChange={() => undefined}>
      <DialogContent
        className="max-h-[90vh] overflow-y-auto [&>button]:hidden"
        onEscapeKeyDown={(event) => event.preventDefault()}
        onPointerDownOutside={(event) => event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>Escolhas de privacidade</DialogTitle>
          <DialogDescription>
            Duas decisões antes de começar. Você pode mudar as duas depois, nas configurações.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-2">
          <h3 id="privacy-auto-record" className="text-sm font-semibold text-foreground">
            Gravação automática
          </h3>
          <p className="text-sm text-muted-foreground">
            O Meetily Live Plus percebe quando uma reunião começa no Teams, no Zoom ou no Meet. O que ele deve fazer?
          </p>
          <AutoRecordModeControl value={mode} onChange={setMode} disabled={saving} labelledBy="privacy-auto-record" />
        </div>

        <div className="space-y-2">
          <label className="flex items-center justify-between gap-4">
            <span id="privacy-internet-lookup" className="text-sm font-semibold text-foreground">
              Pesquisar na internet os termos citados
            </span>
            <Switch
              checked={internetLookup}
              onCheckedChange={setInternetLookup}
              disabled={saving}
              aria-labelledby="privacy-internet-lookup"
            />
          </label>
          <p className="text-sm text-muted-foreground">
            Só o termo sai da máquina, para a Wikipédia e o DuckDuckGo. Áudio e transcrição ficam no computador. Desligada, a ajuda ao vivo explica os termos apenas com o modelo local.
          </p>
        </div>

        {failure && <p className="text-sm text-destructive" role="alert">{failure}</p>}

        <DialogFooter>
          <Button type="button" onClick={save} disabled={saving}>
            {saving ? 'Salvando…' : 'Salvar'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
