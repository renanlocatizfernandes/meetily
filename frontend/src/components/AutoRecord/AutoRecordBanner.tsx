'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface AutoRecordStatus {
  active: boolean;
  app?: string | null;
  phase?: string;
  hint?: string;
  secondsLeft?: number | null;
}

export function AutoRecordBanner() {
  const [status, setStatus] = useState<AutoRecordStatus>({ active: false });

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let alive = true;
    invoke<AutoRecordStatus>('auto_record_watch')
      .then((watch) => {
        if (alive) setStatus(watch);
      })
      .catch(() => undefined);
    listen<AutoRecordStatus>('auto-record-status', (event) => {
      setStatus((current) => ({ ...current, ...event.payload }));
    }).then((fn) => {
      if (alive) unlisteners.push(fn);
      else fn();
    });
    listen<AutoRecordStatus>('auto-record-watch', (event) => {
      setStatus(event.payload);
    }).then((fn) => {
      if (alive) unlisteners.push(fn);
      else fn();
    });
    return () => {
      alive = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  if (!status.active && status.phase !== 'arming' && status.phase !== 'starting') return null;

  if (!status.active) {
    return (
      <div className="fixed top-14 left-1/2 z-30 flex -translate-x-1/2 items-center gap-3 rounded-full border bg-card px-4 py-2 text-sm text-foreground shadow-md">
        <span>
          {status.app || 'Reunião'} detectada. Gravando em {status.secondsLeft ?? 0}s. Avise as outras pessoas.
        </span>
      </div>
    );
  }

  return (
    <div className="fixed top-14 left-1/2 z-30 flex -translate-x-1/2 items-center gap-3 rounded-full border bg-card px-4 py-2 text-sm text-foreground shadow-md">
      <span className="h-2 w-2 rounded-full bg-red-500" />
      <span>
        Gravação automática{status.app ? ` · ${status.app}` : ''}. Avise as outras pessoas.
      </span>
      <button
        type="button"
        className="rounded-full bg-primary px-3 py-1 text-xs font-medium text-primary-foreground"
        onClick={() => {
          invoke('dismiss_auto_recording').catch((error) => console.error(error));
        }}
      >
        Parar
      </button>
    </div>
  );
}
