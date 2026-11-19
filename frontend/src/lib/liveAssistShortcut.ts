// Helpers for the global shortcut of the live assist (Tauri accelerator syntax).

export const DEFAULT_LIVE_ASSIST_SHORTCUT = 'CommandOrControl+Shift+L';

export interface ShortcutStatus {
  shortcut: string;
  registered: boolean;
  error: string | null;
}

export function isMac(): boolean {
  if (typeof navigator === 'undefined') return true;
  return /mac|iphone|ipad/i.test(navigator.platform || navigator.userAgent || '');
}

const CODE_TO_KEY: Record<string, string> = {
  Space: 'Space',
  Enter: 'Enter',
  Tab: 'Tab',
  Backspace: 'Backspace',
  Delete: 'Delete',
  Home: 'Home',
  End: 'End',
  PageUp: 'PageUp',
  PageDown: 'PageDown',
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  Minus: '-',
  Equal: '=',
  Comma: ',',
  Period: '.',
  Slash: '/',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  BracketLeft: '[',
  BracketRight: ']',
  Backquote: '`',
};

function keyFromCode(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return CODE_TO_KEY[code] ?? null;
}

export type CaptureResult =
  | { kind: 'cancel' }
  | { kind: 'pending' }
  | { kind: 'invalid'; reason: string }
  | { kind: 'ok'; accelerator: string };

type KeyLike = Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>;

/** Turns a keydown into a Tauri accelerator such as `CommandOrControl+Shift+L`. */
export function acceleratorFromEvent(event: KeyLike, mac = isMac()): CaptureResult {
  const hasOtherModifier = event.metaKey || event.ctrlKey || event.altKey || event.shiftKey;
  if (event.key === 'Escape' && !hasOtherModifier) return { kind: 'cancel' };
  if (['Meta', 'Control', 'Alt', 'Shift', 'OS', 'AltGraph', 'CapsLock'].includes(event.key)) {
    return { kind: 'pending' };
  }
  const key = keyFromCode(event.code);
  if (!key) return { kind: 'invalid', reason: 'Esta tecla não pode ser usada em um atalho.' };

  const parts: string[] = [];
  if (mac) {
    if (event.metaKey) parts.push('CommandOrControl');
    if (event.ctrlKey) parts.push('Control');
  } else {
    if (event.ctrlKey) parts.push('CommandOrControl');
    if (event.metaKey) parts.push('Super');
  }
  if (event.altKey) parts.push('Alt');
  // Shift alone would steal a plain capital letter from every app.
  if (parts.length === 0) {
    return {
      kind: 'invalid',
      reason: mac ? 'Inclua ⌘, ⌃ ou ⌥ no atalho.' : 'Inclua Ctrl, Alt ou a tecla Windows no atalho.',
    };
  }
  if (event.shiftKey) parts.push('Shift');
  parts.push(key);
  return { kind: 'ok', accelerator: parts.join('+') };
}

const MAC_LABELS: Record<string, string> = {
  commandorcontrol: '⌘',
  cmdorctrl: '⌘',
  cmdorcontrol: '⌘',
  command: '⌘',
  cmd: '⌘',
  super: '⌘',
  meta: '⌘',
  control: '⌃',
  ctrl: '⌃',
  alt: '⌥',
  option: '⌥',
  shift: '⇧',
};

const OTHER_LABELS: Record<string, string> = {
  commandorcontrol: 'Ctrl',
  cmdorctrl: 'Ctrl',
  cmdorcontrol: 'Ctrl',
  command: 'Win',
  cmd: 'Win',
  super: 'Win',
  meta: 'Win',
  control: 'Ctrl',
  ctrl: 'Ctrl',
  alt: 'Alt',
  option: 'Alt',
  shift: 'Shift',
};

const KEY_LABELS: Record<string, string> = {
  space: 'Espaço',
  up: '↑',
  down: '↓',
  left: '←',
  right: '→',
  enter: 'Enter',
};

/** Key caps for display: `CommandOrControl+Shift+L` → ['⌘', '⇧', 'L'] on macOS. */
export function shortcutKeyCaps(accelerator: string, mac = isMac()): string[] {
  const labels = mac ? MAC_LABELS : OTHER_LABELS;
  return accelerator
    .split('+')
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const lower = part.toLowerCase();
      return labels[lower] ?? KEY_LABELS[lower] ?? (part.length === 1 ? part.toUpperCase() : part);
    });
}

/** Short text for running copy: "⌘⇧L" on macOS, "Ctrl+Shift+L" elsewhere. */
export function shortcutText(accelerator: string, mac = isMac()): string {
  return shortcutKeyCaps(accelerator, mac).join(mac ? '' : '+');
}
