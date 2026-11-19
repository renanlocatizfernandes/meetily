'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface AutoRecordStatus {
  active: boolean;
  app?: string | null;
}

export function AutoRecordBanner() {
  const [status, setStatus] = useState<AutoRecordStatus>({ active: false });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<AutoRecordStatus>('auto-record-status', (event) => {
      setStatus(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  if (!status.active) return null;

  return (
    <div className="fixed top-4 left-1/2 z-30 flex -translate-x-1/2 items-center gap-3 rounded-full border border-amber-300 bg-amber-50 px-4 py-2 text-sm text-amber-950 shadow">
      <span className="h-2 w-2 rounded-full bg-red-500" />
      <span>
        Gravação automática{status.app ? ` · ${status.app}` : ''}. Avise as outras pessoas.
      </span>
      <button
        type="button"
        className="rounded-full bg-amber-900 px-3 py-1 text-xs font-medium text-white"
        onClick={() => {
          invoke('dismiss_auto_recording').catch((error) => console.error(error));
        }}
      >
        Parar
      </button>
    </div>
  );
}
