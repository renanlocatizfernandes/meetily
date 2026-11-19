'use client';

import React, { createContext, useCallback, useContext, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface FeatureToggles {
  internetSearch: boolean;
  autoTasks: boolean;
  meetingMemory: boolean;
  calendarPrep: boolean;
  voiceProfiles: boolean;
  commitmentAlerts: boolean;
  followupEmail: boolean;
  meetingSearch: boolean;
  scoreboard: boolean;
  translation: 'auto' | 'on' | 'off';
}

export const DEFAULT_TOGGLES: FeatureToggles = {
  internetSearch: true,
  autoTasks: true,
  meetingMemory: true,
  calendarPrep: true,
  voiceProfiles: true,
  commitmentAlerts: true,
  followupEmail: true,
  meetingSearch: true,
  scoreboard: true,
  translation: 'auto',
};

interface FeatureContextValue {
  toggles: FeatureToggles;
  loaded: boolean;
  setToggle: (patch: Partial<FeatureToggles>) => Promise<void>;
}

const FeatureContext = createContext<FeatureContextValue | undefined>(undefined);

export function FeatureProvider({ children }: { children: React.ReactNode }) {
  const [toggles, setToggles] = useState<FeatureToggles>(DEFAULT_TOGGLES);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    invoke<FeatureToggles>('get_feature_toggles')
      .then((loadedToggles) => {
        if (!cancelled && loadedToggles) setToggles({ ...DEFAULT_TOGGLES, ...loadedToggles });
      })
      .catch((error) => console.error('Falha ao carregar os recursos', error))
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const setToggle = useCallback(async (patch: Partial<FeatureToggles>) => {
    const next = { ...toggles, ...patch };
    setToggles(next);
    try {
      const saved = await invoke<FeatureToggles>('save_feature_toggles', { toggles: next });
      setToggles(saved);
    } catch (error) {
      setToggles(toggles);
      throw error;
    }
  }, [toggles]);

  return (
    <FeatureContext.Provider value={{ toggles, loaded, setToggle }}>
      {children}
    </FeatureContext.Provider>
  );
}

export function useFeatures() {
  const context = useContext(FeatureContext);
  if (!context) throw new Error('useFeatures must be used within FeatureProvider');
  return context;
}
