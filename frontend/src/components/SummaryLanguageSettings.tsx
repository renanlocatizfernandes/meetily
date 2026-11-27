'use client';

import { useState } from 'react';
import { Globe, Pin } from 'lucide-react';
import { Popover, PopoverTrigger, PopoverContent } from '@/components/ui/popover';
import { LanguagePickerPopover } from '@/components/LanguagePickerPopover';
import { useRecentLanguages } from '@/hooks/useRecentLanguages';
import { labelForCode } from '@/lib/summary-languages';

export function SummaryLanguageSettings() {
  const { recents, pinned, addRecent, removeRecent, setPinned } = useRecentLanguages();
  const [pickerOpen, setPickerOpen] = useState(false);

  const togglePin = (code: string) => {
    setPinned(pinned === code ? null : code);
  };

  return (
    <div className="bg-card rounded-xl border border-border p-5 relative">
      <div className="flex items-center gap-2">
        <Globe size={16} className="text-muted-foreground" />
        <h2 className="text-sm font-medium">Idioma do resumo</h2>
      </div>
      <p className="mt-0.5 mb-4 text-[13px] text-muted-foreground">
        Fixe um idioma como padrão para as novas reuniões. Os demais continuam como
        opções de troca rápida ao gerar o resumo. O Automático usa o idioma predominante da transcrição.
      </p>

      <div className="flex flex-wrap items-center gap-2">
        {recents.map((code) => {
          const isPinned = pinned === code;
          return (
            <span
              key={code}
              className={`inline-flex items-center rounded-full border text-sm overflow-hidden ${
                isPinned
                  ? 'bg-brand-soft border-primary/30 text-brand'
                  : 'bg-muted border-border text-foreground'
              }`}
            >
              <button
                type="button"
                aria-label={isPinned ? `Remover ${labelForCode(code)} como padrão` : `Definir ${labelForCode(code)} como padrão`}
                aria-pressed={isPinned}
                title={isPinned ? 'Clique para remover como padrão' : 'Clique para definir como padrão'}
                onClick={() => togglePin(code)}
                className={`flex items-center gap-1.5 pl-3 pr-2 py-1 hover:brightness-95 active:brightness-90 ${
                  isPinned ? 'text-brand' : 'text-foreground'
                }`}
              >
                <Pin
                  size={14}
                  className={isPinned ? 'text-brand' : 'text-muted-foreground/80'}
                  fill={isPinned ? 'currentColor' : 'none'}
                />
                {labelForCode(code)}
              </button>
              <button
                type="button"
                aria-label={`Remover ${labelForCode(code)}`}
                onClick={() => removeRecent(code)}
                className={`pr-2.5 pl-0.5 py-1 leading-none ${isPinned ? 'text-brand hover:text-brand' : 'text-muted-foreground/80 hover:text-foreground'}`}
              >
                ×
              </button>
            </span>
          );
        })}

        <Popover open={pickerOpen} onOpenChange={setPickerOpen}>
          <PopoverTrigger asChild>
            <button
              type="button"
              disabled={recents.length >= 5}
              className="inline-flex items-center gap-1 rounded-full border border-dashed border-border px-3 py-1 text-sm text-muted-foreground hover:border-input hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
            >
              ＋ Adicionar idioma
            </button>
          </PopoverTrigger>
          <PopoverContent align="start" className="w-auto p-0 border-0 shadow-none bg-transparent">
            <LanguagePickerPopover
              mode="settings"
              value={null}
              onChange={(code) => {
                if (code) addRecent(code);
                setPickerOpen(false);
              }}
              onClose={() => setPickerOpen(false)}
            />
          </PopoverContent>
        </Popover>
      </div>

      <p className="text-xs text-muted-foreground/80 mt-3">
        {pinned
          ? `Padrão: ${labelForCode(pinned)}. Clique de novo para remover. Máximo de 5 opções de troca rápida.`
          : 'Clique em um idioma para defini-lo como padrão. Máximo de 5 opções de troca rápida.'}
      </p>
    </div>
  );
}
