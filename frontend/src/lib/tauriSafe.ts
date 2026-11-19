import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

export type SafeResult<T> =
  | { ok: true; value: T }
  | { ok: false; missing: boolean; error: string };

export function errorText(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

/** True when the Rust core does not have (or does not expose) this command yet. */
export function isMissingCommand(error: unknown, command: string): boolean {
  const text = errorText(error).toLowerCase();
  if (!text.includes(command.toLowerCase()) && !text.includes('command')) return false;
  return text.includes('not found') || text.includes('not allowed') || text.includes('unknown command');
}

/** `invoke` that never throws. Commands that do not exist yet come back with `missing: true`. */
export async function safeInvoke<T>(command: string, args?: Record<string, unknown>): Promise<SafeResult<T>> {
  try {
    return { ok: true, value: await invoke<T>(command, args) };
  } catch (error) {
    const missing = isMissingCommand(error, command) || typeof window === 'undefined' || !hasTauri();
    if (!missing) console.error(`Falha em ${command}`, error);
    return { ok: false, missing, error: errorText(error) };
  }
}

function hasTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** Label of this webview ("main", "live-assist"…). Falls back to "main" outside Tauri. */
export function windowLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    return 'main';
  }
}

/** `emit` that never throws (other windows may not exist). */
export function safeEmit(event: string, payload: unknown): void {
  try {
    emit(event, payload).catch(() => undefined);
  } catch {
    // outside Tauri
  }
}

/**
 * Subscribes to several events and returns one cleanup function.
 * Safe to call from an effect: listeners attached after cleanup are removed at once.
 */
export function listenAll(handlers: Record<string, (payload: any) => void>): () => void {
  let active = true;
  const unlisteners: Array<() => void> = [];
  for (const [event, handler] of Object.entries(handlers)) {
    let pending: Promise<() => void>;
    try {
      pending = listen<any>(event, (received) => handler(received.payload));
    } catch {
      continue;
    }
    pending
      .then((unlisten) => {
        if (active) unlisteners.push(unlisten);
        else unlisten();
      })
      .catch(() => undefined);
  }
  return () => {
    active = false;
    unlisteners.forEach((unlisten) => unlisten());
  };
}
