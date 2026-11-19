/**
 * Tema do app: segue o sistema por padrão, com escolha manual (Sistema / Claro / Escuro)
 * guardada em localStorage. A classe `dark` em <html> é aplicada antes da primeira pintura
 * pelo script de `themeInitScript` (ver app/layout.tsx).
 */
import { useCallback, useEffect, useState } from 'react';

export type ThemePreference = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

export const THEME_STORAGE_KEY = 'meetily.theme';
const THEME_EVENT = 'meetily-theme-change';

export const THEME_LABELS: Record<ThemePreference, string> = {
  system: 'Sistema',
  light: 'Claro',
  dark: 'Escuro',
};

/** Roda no <head>, antes da pintura. Mantenha sem dependências. */
export const themeInitScript = `(function(){try{var k='${THEME_STORAGE_KEY}';var p=localStorage.getItem(k);if(p!=='light'&&p!=='dark'){p='system'}var d=p==='dark'||(p==='system'&&window.matchMedia('(prefers-color-scheme: dark)').matches);var r=document.documentElement;r.classList.toggle('dark',d);r.setAttribute('data-theme',d?'dark':'light');r.setAttribute('data-theme-preference',p)}catch(e){}})();`;

export function readThemePreference(): ThemePreference {
  if (typeof window === 'undefined') return 'system';
  try {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    return stored === 'light' || stored === 'dark' ? stored : 'system';
  } catch {
    return 'system';
  }
}

function systemPrefersDark(): boolean {
  return typeof window !== 'undefined'
    && typeof window.matchMedia === 'function'
    && window.matchMedia('(prefers-color-scheme: dark)').matches;
}

export function resolveTheme(preference: ThemePreference): ResolvedTheme {
  if (preference === 'system') return systemPrefersDark() ? 'dark' : 'light';
  return preference;
}

export function applyTheme(preference: ThemePreference): ResolvedTheme {
  const resolved = resolveTheme(preference);
  if (typeof document !== 'undefined') {
    const root = document.documentElement;
    root.classList.toggle('dark', resolved === 'dark');
    root.setAttribute('data-theme', resolved);
    root.setAttribute('data-theme-preference', preference);
  }
  return resolved;
}

export function saveThemePreference(preference: ThemePreference): void {
  try {
    if (preference === 'system') localStorage.removeItem(THEME_STORAGE_KEY);
    else localStorage.setItem(THEME_STORAGE_KEY, preference);
  } catch {
    // A preferência é opcional; sem armazenamento o app segue o sistema.
  }
  applyTheme(preference);
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent(THEME_EVENT));
  }
}

/** Preferência e tema efetivo, sincronizados entre componentes, janelas e o sistema. */
export function useTheme() {
  const [preference, setPreferenceState] = useState<ThemePreference>('system');
  const [resolved, setResolved] = useState<ResolvedTheme>('light');

  useEffect(() => {
    const sync = () => {
      const current = readThemePreference();
      setPreferenceState(current);
      setResolved(applyTheme(current));
    };
    sync();

    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const onStorage = (event: StorageEvent) => {
      if (event.key === null || event.key === THEME_STORAGE_KEY) sync();
    };
    media.addEventListener('change', sync);
    window.addEventListener(THEME_EVENT, sync);
    window.addEventListener('storage', onStorage);
    return () => {
      media.removeEventListener('change', sync);
      window.removeEventListener(THEME_EVENT, sync);
      window.removeEventListener('storage', onStorage);
    };
  }, []);

  const setPreference = useCallback((next: ThemePreference) => {
    saveThemePreference(next);
  }, []);

  return { preference, resolved, setPreference };
}
