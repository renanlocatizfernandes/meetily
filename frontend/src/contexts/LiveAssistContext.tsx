'use client';

import React, { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { toast } from 'sonner';

export type LiveAssistSection = 'perguntas' | 'respostas' | 'contexto';

export interface LiveAssistSettings {
  enabled: boolean;
  model: string;
  language: string;
  frequency: string;
  meetingContext: string;
}

export interface LiveAssistItem {
  id: string;
  section: LiveAssistSection;
  text: string;
  pinned: boolean;
  createdAt: number;
}

export interface LiveAssistStatus {
  enabled: boolean;
  recording: boolean;
  busy: boolean;
  note: string | null;
}

const DEFAULT_SETTINGS: LiveAssistSettings = {
  enabled: true,
  model: 'llama3.2:latest',
  language: 'pt-BR',
  frequency: 'media',
  meetingContext: '',
};

interface LiveAssistContextValue {
  settings: LiveAssistSettings;
  settingsLoaded: boolean;
  saveSettings: (next: LiveAssistSettings) => Promise<void>;
  items: LiveAssistItem[];
  pinItem: (id: string, pinned: boolean) => void;
  dismissItem: (id: string) => void;
  panelOpen: boolean;
  setPanelOpen: (open: boolean) => void;
  status: LiveAssistStatus;
  openAlwaysOnTop: () => Promise<void>;
}

const LiveAssistContext = createContext<LiveAssistContextValue | undefined>(undefined);

export function LiveAssistProvider({ children }: { children: React.ReactNode }) {
  const [settings, setSettings] = useState<LiveAssistSettings>(DEFAULT_SETTINGS);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [items, setItems] = useState<LiveAssistItem[]>([]);
  const [panelOpen, setPanelOpenState] = useState(false);
  const panelDismissed = useRef(false);
  const [status, setStatus] = useState<LiveAssistStatus>({
    enabled: true,
    recording: false,
    busy: false,
    note: null,
  });

  useEffect(() => {
    let cancelled = false;
    invoke<LiveAssistSettings>('get_live_assist_settings')
      .then((loaded) => {
        if (!cancelled && loaded) {
          setSettings(loaded);
          setStatus((current) => ({ ...current, enabled: loaded.enabled }));
        }
      })
      .catch((error) => {
        console.error('Falha ao carregar a ajuda ao vivo', error);
      })
      .finally(() => {
        if (!cancelled) setSettingsLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let active = true;

    const attach = async () => {
      const updates = await listen<{ items: Array<{ id: string; section: LiveAssistSection; text: string }> }>(
        'live-assist-update',
        (event) => {
          const incoming = event.payload?.items ?? [];
          if (incoming.length === 0) return;
          setItems((current) => {
            const next = [...current];
            for (const item of [...incoming].reverse()) {
              if (next.some((existing) => existing.id === item.id)) continue;
              next.unshift({ ...item, pinned: false, createdAt: Date.now() });
            }
            return next.slice(0, 40);
          });
          setPanelOpenState((open) => open || !panelDismissed.current);
        },
      );
      const statuses = await listen<LiveAssistStatus>('live-assist-status', (event) => {
        setStatus(event.payload);
      });
      const alerts = await listen<{ text: string; kind: string }>('commitment-alert', (event) => {
        toast.message(`Compromisso (${event.payload.kind})`, { description: event.payload.text });
        setItems((current) => [{
          id: `alert-${Date.now()}`,
          section: 'contexto' as const,
          text: `Compromisso: ${event.payload.text}`,
          pinned: true,
          createdAt: Date.now(),
        }, ...current].slice(0, 40));
        setPanelOpenState((open) => open || !panelDismissed.current);
      });
      const toggles = await listen('live-assist-toggle', async () => {
        const label = getCurrentWindow().label;
        if (label === 'live-assist') {
          const window = getCurrentWindow();
          if (await window.isVisible()) {
            await window.hide();
          } else {
            await window.show();
            await window.setFocus();
          }
          return;
        }
        setPanelOpenState((open) => {
          const next = !open;
          panelDismissed.current = !next;
          return next;
        });
      });
      if (!active) {
        updates();
        statuses();
        alerts();
        toggles();
        return;
      }
      unlisteners.push(updates, statuses, alerts, toggles);
    };

    attach().catch((error) => console.error('Falha ao ouvir a ajuda ao vivo', error));
    return () => {
      active = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  const saveSettings = useCallback(async (next: LiveAssistSettings) => {
    const saved = await invoke<LiveAssistSettings>('save_live_assist_settings', { settings: next });
    setSettings(saved);
    setStatus((current) => ({ ...current, enabled: saved.enabled }));
  }, []);

  const pinItem = useCallback((id: string, pinned: boolean) => {
    setItems((current) => current.map((item) => (item.id === id ? { ...item, pinned } : item)));
  }, []);

  const dismissItem = useCallback((id: string) => {
    setItems((current) => current.filter((item) => item.id !== id));
  }, []);

  const setPanelOpen = useCallback((open: boolean) => {
    panelDismissed.current = !open;
    setPanelOpenState(open);
  }, []);

  const openAlwaysOnTop = useCallback(async () => {
    try {
      await invoke('open_live_assist_window');
      setPanelOpen(false);
    } catch (error) {
      toast.error('Não foi possível abrir a janela');
      console.error(error);
    }
  }, [setPanelOpen]);

  return (
    <LiveAssistContext.Provider
      value={{
        settings,
        settingsLoaded,
        saveSettings,
        items,
        pinItem,
        dismissItem,
        panelOpen,
        setPanelOpen,
        status,
        openAlwaysOnTop,
      }}
    >
      {children}
    </LiveAssistContext.Provider>
  );
}

export function useLiveAssist() {
  const context = useContext(LiveAssistContext);
  if (!context) {
    throw new Error('useLiveAssist must be used within LiveAssistProvider');
  }
  return context;
}
