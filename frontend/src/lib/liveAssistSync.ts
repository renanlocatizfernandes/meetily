// Shared item state between the main panel and the always-on-top window.
// They are two webviews, so every user action is broadcast with Tauri's JS `emit`.

export const ITEM_ACTION_EVENT = 'live-assist-item-action';
export const STATE_REQUEST_EVENT = 'live-assist-state-request';
export const STATE_SNAPSHOT_EVENT = 'live-assist-state-snapshot';

import type { LiveAssistOrigin } from './liveAssistContext';

export type SyncSection = 'perguntas' | 'respostas' | 'contexto';

export interface SyncItem {
  id: string;
  section: SyncSection;
  text: string;
  pinned: boolean;
  createdAt: number;
  /** Where the suggestion came from. Not part of `itemKey`. */
  origin?: LiveAssistOrigin;
}

export type ItemAction =
  | { action: 'pin' | 'unpin'; key: string; origin: string; item: SyncItem }
  | { action: 'dismiss'; key: string; origin: string }
  | { action: 'clear'; origin: string };

export interface StateRequest {
  origin: string;
}

export interface StateSnapshot {
  origin: string;
  target: string;
  items: SyncItem[];
  dismissed: string[];
}

export const MAX_ITEMS = 40;
const MAX_DISMISSED = 200;

function normalize(text: string): string {
  return text
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .trim();
}

/** Stable id that is the same in every webview: section + FNV-1a of the normalized text. */
export function itemKey(item: { section: string; text: string }): string {
  const text = normalize(item.text);
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return `${item.section}:${(hash >>> 0).toString(16)}:${text.length}`;
}

export function rememberDismissed(dismissed: Set<string>, key: string): void {
  dismissed.add(key);
  if (dismissed.size > MAX_DISMISSED) {
    const oldest = dismissed.values().next().value;
    if (oldest !== undefined) dismissed.delete(oldest);
  }
}

/** Applies an action that came from this or from another window. */
export function applyItemAction<T extends SyncItem>(items: T[], action: ItemAction): T[] {
  switch (action.action) {
    case 'pin':
    case 'unpin': {
      const pinned = action.action === 'pin';
      if (items.some((item) => itemKey(item) === action.key)) {
        return items.map((item) => (itemKey(item) === action.key ? { ...item, pinned } : item));
      }
      // The other window pinned something this one never received.
      return pinned ? ([{ ...action.item, pinned: true } as T, ...items].slice(0, MAX_ITEMS)) : items;
    }
    case 'dismiss':
      return items.filter((item) => itemKey(item) !== action.key);
    case 'clear':
      return items.filter((item) => item.pinned);
    default:
      return items;
  }
}

/** Merges the state of the other window into this one (used when a window opens). */
export function mergeSnapshot<T extends SyncItem>(items: T[], snapshot: StateSnapshot, dismissed: Set<string>): T[] {
  for (const key of snapshot.dismissed ?? []) rememberDismissed(dismissed, key);
  const byKey = new Map<string, T>();
  for (const item of items) byKey.set(itemKey(item), item);
  for (const remote of snapshot.items ?? []) {
    const key = itemKey(remote);
    const local = byKey.get(key);
    byKey.set(key, local ? { ...local, pinned: remote.pinned } : (remote as T));
  }
  return [...byKey.entries()]
    .filter(([key]) => !dismissed.has(key))
    .map(([, item]) => item)
    .sort((a, b) => b.createdAt - a.createdAt)
    .slice(0, MAX_ITEMS);
}
