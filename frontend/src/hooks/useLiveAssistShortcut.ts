'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { DEFAULT_LIVE_ASSIST_SHORTCUT, ShortcutStatus } from '@/lib/liveAssistShortcut';
import { listenAll, safeEmit, safeInvoke, windowLabel } from '@/lib/tauriSafe';

// Sent by the Rust core when registration fails at startup.
const STATUS_EVENT = 'live-assist-shortcut-status';
// Sent between webviews so every window shows the same shortcut after a change.
const CHANGED_EVENT = 'live-assist-shortcut-changed';

export interface LiveAssistShortcutState {
  /** Null while loading or when this build has no shortcut commands. */
  status: ShortcutStatus | null;
  /** False when the Rust core does not expose the shortcut commands. */
  available: boolean;
  /** Accelerator to show in the UI (falls back to the default one). */
  accelerator: string;
  update: (shortcut: string) => Promise<ShortcutStatus>;
}

export function useLiveAssistShortcut(): LiveAssistShortcutState {
  const router = useRouter();
  const [status, setStatus] = useState<ShortcutStatus | null>(null);
  const [available, setAvailable] = useState(false);
  const warned = useRef(false);

  const warnOnce = useCallback((failed: ShortcutStatus) => {
    if (warned.current || failed.registered || windowLabel() !== 'main') return;
    warned.current = true;
    toast.warning('O atalho da ajuda ao vivo não foi ativado', {
      description: failed.error || 'Outro app já usa este atalho. Escolha outro nas configurações.',
      duration: 12000,
      action: {
        label: 'Abrir configurações',
        onClick: () => router.push('/settings?tab=liveAssist'),
      },
    });
  }, [router]);

  useEffect(() => {
    let cancelled = false;
    safeInvoke<ShortcutStatus>('get_live_assist_shortcut').then((result) => {
      if (cancelled || !result.ok || !result.value) return;
      setAvailable(true);
      setStatus(result.value);
      warnOnce(result.value);
    });
    const stop = listenAll({
      [STATUS_EVENT]: (payload: ShortcutStatus) => {
        if (!payload || typeof payload.shortcut !== 'string') return;
        setAvailable(true);
        setStatus(payload);
        warnOnce(payload);
      },
      [CHANGED_EVENT]: (payload: { origin: string; status: ShortcutStatus }) => {
        if (!payload?.status || payload.origin === windowLabel()) return;
        setStatus(payload.status);
      },
    });
    return () => {
      cancelled = true;
      stop();
    };
  }, [warnOnce]);

  const update = useCallback(async (shortcut: string) => {
    const result = await safeInvoke<ShortcutStatus>('set_live_assist_shortcut', { shortcut });
    if (!result.ok) throw new Error(result.error);
    setStatus(result.value);
    safeEmit(CHANGED_EVENT, { origin: windowLabel(), status: result.value });
    return result.value;
  }, []);

  return {
    status,
    available,
    accelerator: status?.shortcut || DEFAULT_LIVE_ASSIST_SHORTCUT,
    update,
  };
}
