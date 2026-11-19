'use client';

import React from 'react';
import { Monitor, Moon, Sun } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { IconButton } from '@/components/ui/icon-button';
import { THEME_LABELS, useTheme, type ThemePreference } from '@/lib/theme';

const OPTIONS: Array<{ value: ThemePreference; icon: typeof Sun }> = [
  { value: 'system', icon: Monitor },
  { value: 'light', icon: Sun },
  { value: 'dark', icon: Moon },
];

/** Sistema / Claro / Escuro. `compact` mostra um único botão que alterna entre os três. */
export function ThemeSwitch({ compact = false }: { compact?: boolean }) {
  const { preference, setPreference } = useTheme();

  if (compact) {
    const index = OPTIONS.findIndex((option) => option.value === preference);
    const current = OPTIONS[index] ?? OPTIONS[0];
    const next = OPTIONS[(index + 1) % OPTIONS.length];
    const Icon = current.icon;
    return (
      <IconButton
        label={`Tema: ${THEME_LABELS[current.value]}. Mudar para ${THEME_LABELS[next.value]}`}
        tooltipSide="right"
        onClick={() => setPreference(next.value)}
      >
        <Icon />
      </IconButton>
    );
  }

  return (
    <div role="radiogroup" aria-label="Tema" className="inline-flex items-center rounded-lg border border-sidebar-border p-0.5">
      {OPTIONS.map(({ value, icon: Icon }) => {
        const selected = preference === value;
        return (
          <Tooltip key={value}>
            <TooltipTrigger asChild>
              <button
                type="button"
                role="radio"
                aria-checked={selected}
                aria-label={`Tema ${THEME_LABELS[value].toLowerCase()}`}
                onClick={() => setPreference(value)}
                className={`inline-flex h-7 w-7 items-center justify-center rounded-md transition-colors duration-150 ${
                  selected
                    ? 'bg-sidebar-accent text-foreground'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                <Icon className="h-3.5 w-3.5" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="top">{THEME_LABELS[value]}</TooltipContent>
          </Tooltip>
        );
      })}
    </div>
  );
}
