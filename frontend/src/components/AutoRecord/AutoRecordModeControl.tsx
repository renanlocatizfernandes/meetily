'use client';

import { useRef } from 'react';
import { cn } from '@/lib/utils';

export type AutoRecordMode = 'off' | 'ask' | 'auto';

export type AutoRecordModeOption = { value: AutoRecordMode; label: string; detail: string; recommended?: boolean };

export const AUTO_RECORD_MODES: AutoRecordModeOption[] = [
  { value: 'off', label: 'Desligada', detail: 'Você começa cada gravação.' },
  { value: 'ask', label: 'Perguntar antes de gravar', detail: 'Um aviso aparece quando uma reunião começa.', recommended: true },
  { value: 'auto', label: 'Gravar sozinho', detail: 'A gravação começa depois de uma contagem curta.' },
];

export function isAutoRecordMode(value: unknown): value is AutoRecordMode {
  return value === 'off' || value === 'ask' || value === 'auto';
}

/** Three-option radio group for the automatic recording mode (or another off/ask/auto choice via `options`). */
export function AutoRecordModeControl({
  value,
  onChange,
  disabled = false,
  labelledBy,
  options = AUTO_RECORD_MODES,
}: {
  value: AutoRecordMode;
  onChange: (mode: AutoRecordMode) => void;
  disabled?: boolean;
  labelledBy?: string;
  options?: AutoRecordModeOption[];
}) {
  const buttons = useRef<Array<HTMLButtonElement | null>>([]);

  const move = (index: number, step: number) => {
    const next = (index + step + options.length) % options.length;
    onChange(options[next].value);
    buttons.current[next]?.focus();
  };

  return (
    <div role="radiogroup" aria-labelledby={labelledBy} className="grid gap-2">
      {options.map((option, index) => {
        const selected = option.value === value;
        return (
          <button
            key={option.value}
            ref={(element) => { buttons.current[index] = element; }}
            type="button"
            role="radio"
            aria-checked={selected}
            tabIndex={selected ? 0 : -1}
            disabled={disabled}
            onClick={() => onChange(option.value)}
            onKeyDown={(event) => {
              if (event.key === 'ArrowDown' || event.key === 'ArrowRight') {
                event.preventDefault();
                move(index, 1);
              } else if (event.key === 'ArrowUp' || event.key === 'ArrowLeft') {
                event.preventDefault();
                move(index, -1);
              }
            }}
            className={cn(
              'flex w-full items-start gap-3 rounded-lg border px-3 py-2 text-left transition-colors',
              'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50',
              selected ? 'border-primary bg-muted' : 'border-border bg-background hover:bg-muted/60',
            )}
          >
            <span
              aria-hidden="true"
              className={cn(
                'mt-1 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border',
                selected ? 'border-primary' : 'border-muted-foreground',
              )}
            >
              {selected && <span className="h-2 w-2 rounded-full bg-primary" />}
            </span>
            <span className="min-w-0">
              <span className="block text-sm font-medium text-foreground">
                {option.label}
                {option.recommended && (
                  <span className="ml-2 rounded-full bg-primary px-2 py-0.5 text-xs font-medium text-primary-foreground">Recomendado</span>
                )}
              </span>
              <span className="block text-xs text-muted-foreground">{option.detail}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}
