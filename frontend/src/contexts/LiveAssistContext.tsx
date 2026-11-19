'use client';

import React, { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { toast } from 'sonner';
import { safeEmit, windowLabel } from '@/lib/tauriSafe';
import {
  ITEM_ACTION_EVENT,
  STATE_REQUEST_EVENT,
  STATE_SNAPSHOT_EVENT,
  ItemAction,
  StateRequest,
  StateSnapshot,
  applyItemAction,
  itemKey,
  mergeSnapshot,
  rememberDismissed,
} from '@/lib/liveAssistSync';
import { PartialItem, useLiveAssistPartials } from '@/hooks/useLiveAssistPartials';
import { LiveAssistContextSummary, LiveAssistOrigin, parseOrigin } from '@/lib/liveAssistContext';
import { LiveAssistShortcutState, useLiveAssistShortcut } from '@/hooks/useLiveAssistShortcut';

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
  /** Where the suggestion came from, when the app could tell. */
  origin?: LiveAssistOrigin;
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
  clearItems: () => void;
  partials: PartialItem[];
  /** "Contexto em uso" of the recording in progress; null when not recording. */
  contextSummary: LiveAssistContextSummary | null;
  shortcut: LiveAssistShortcutState;
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
  const [contextSummary, setContextSummary] = useState<LiveAssistContextSummary | null>(null);
  const [panelOpen, setPanelOpenState] = useState(false);
  const panelDismissed = useRef(false);
  const itemsRef = useRef<LiveAssistItem[]>([]);
  const dismissedKeys = useRef<Set<string>>(new Set());
  const { partials, settle: settlePartials } = useLiveAssistPartials();
  const shortcut = useLiveAssistShortcut();

  useEffect(() => {
    itemsRef.current = items;
  }, [items]);
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
      const updates = await listen<{ requestId?: number; items: Array<{ id: string; section: LiveAssistSection; text: string; origin?: unknown }> }>(
        'live-assist-update',
        (event) => {
          settlePartials(event.payload?.requestId);
          const incoming = event.payload?.items ?? [];
          if (incoming.length === 0) return;
          setItems((current) => {
            const next = [...current];
            for (const item of [...incoming].reverse()) {
              if (next.some((existing) => existing.id === item.id)) continue;
              const key = itemKey(item);
              if (dismissedKeys.current.has(key) || next.some((existing) => itemKey(existing) === key)) continue;
              next.unshift({
                id: item.id,
                section: item.section,
                text: item.text,
                origin: parseOrigin(item.origin),
                pinned: false,
                createdAt: Date.now(),
              });
            }
            return next.slice(0, 40);
          });
          setPanelOpenState((open) => open || !panelDismissed.current);
        },
      );
      const statuses = await listen<LiveAssistStatus>('live-assist-status', (event) => {
        setStatus(event.payload);
        if (!event.payload?.recording) setContextSummary(null);
      });
      const contexts = await listen<LiveAssistContextSummary>('live-assist-context', (event) => {
        if (event.payload) setContextSummary(event.payload);
      });
      // A window opened in the middle of a recording asks for the current context.
      invoke<LiveAssistContextSummary | null>('get_live_assist_context')
        .then((current) => {
          if (active && current) setContextSummary((existing) => existing ?? current);
        })
        .catch(() => undefined);
      const alerts = await listen<{ text: string; kind: string }>('commitment-alert', (event) => {
        toast.message(`Compromisso (${event.payload.kind})`, { description: event.payload.text });
        setItems((current) => [{
          id: `alert-${Date.now()}`,
          section: 'contexto' as const,
          text: `Compromisso: ${event.payload.text}`,
          origin: { kind: 'conversa', label: 'Conversa' },
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
      // Item state shared with the other webview (main panel <-> always-on-top window).
      const self = windowLabel();
      const actions = await listen<ItemAction>(ITEM_ACTION_EVENT, (event) => {
        const action = event.payload;
        if (!action || action.origin === self) return;
        if (action.action === 'dismiss') rememberDismissed(dismissedKeys.current, action.key);
        setItems((current) => applyItemAction(current, action));
      });
      const requests = await listen<StateRequest>(STATE_REQUEST_EVENT, (event) => {
        if (!event.payload || event.payload.origin === self) return;
        if (itemsRef.current.length === 0 && dismissedKeys.current.size === 0) return;
        const snapshot: StateSnapshot = {
          origin: self,
          target: event.payload.origin,
          items: itemsRef.current,
          dismissed: [...dismissedKeys.current],
        };
        safeEmit(STATE_SNAPSHOT_EVENT, snapshot);
      });
      const snapshots = await listen<StateSnapshot>(STATE_SNAPSHOT_EVENT, (event) => {
        const snapshot = event.payload;
        if (!snapshot || snapshot.origin === self || snapshot.target !== self) return;
        setItems((current) => mergeSnapshot(current, snapshot, dismissedKeys.current));
      });
      if (!active) {
        updates();
        statuses();
        contexts();
        alerts();
        toggles();
        actions();
        requests();
        snapshots();
        return;
      }
      unlisteners.push(updates, statuses, contexts, alerts, toggles, actions, requests, snapshots);
      // A window that just opened asks the other one for what is already on screen.
      safeEmit(STATE_REQUEST_EVENT, { origin: self } satisfies StateRequest);
    };

    attach().catch((error) => console.error('Falha ao ouvir a ajuda ao vivo', error));
    return () => {
      active = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [settlePartials]);

  const saveSettings = useCallback(async (next: LiveAssistSettings) => {
    const saved = await invoke<LiveAssistSettings>('save_live_assist_settings', { settings: next });
    setSettings(saved);
    setStatus((current) => ({ ...current, enabled: saved.enabled }));
  }, []);

  const pinItem = useCallback((id: string, pinned: boolean) => {
    setItems((current) => current.map((item) => (item.id === id ? { ...item, pinned } : item)));
    const item = itemsRef.current.find((candidate) => candidate.id === id);
    if (item) {
      safeEmit(ITEM_ACTION_EVENT, {
        action: pinned ? 'pin' : 'unpin',
        key: itemKey(item),
        origin: windowLabel(),
        item: { ...item, pinned },
      } satisfies ItemAction);
    }
  }, []);

  const dismissItem = useCallback((id: string) => {
    setItems((current) => current.filter((item) => item.id !== id));
    const item = itemsRef.current.find((candidate) => candidate.id === id);
    if (item) {
      const key = itemKey(item);
      rememberDismissed(dismissedKeys.current, key);
      safeEmit(ITEM_ACTION_EVENT, { action: 'dismiss', key, origin: windowLabel() } satisfies ItemAction);
    }
  }, []);

  // Removes every suggestion that is not pinned, in both windows.
  const clearItems = useCallback(() => {
    setItems((current) => current.filter((item) => item.pinned));
    safeEmit(ITEM_ACTION_EVENT, { action: 'clear', origin: windowLabel() } satisfies ItemAction);
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
        clearItems,
        partials,
        contextSummary,
        shortcut,
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
