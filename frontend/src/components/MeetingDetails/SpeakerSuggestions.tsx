"use client";

import { useState } from 'react';
import { Check, Users, X } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import type { SpeakerNameSuggestion } from '@/hooks/meeting-details/useSpeakerSuggestions';

interface SpeakerSuggestionsProps {
  suggestions: SpeakerNameSuggestion[];
  onConfirm: (item: SpeakerNameSuggestion) => Promise<void>;
  onDismiss: (item: SpeakerNameSuggestion) => void;
  onDismissAll: () => void;
}

/** Suggested names for unnamed speakers. Nothing is renamed until the person confirms. */
export function SpeakerSuggestions({ suggestions, onConfirm, onDismiss, onDismissAll }: SpeakerSuggestionsProps) {
  const [busyKey, setBusyKey] = useState<string | null>(null);
  if (suggestions.length === 0) return null;

  const confirm = async (item: SpeakerNameSuggestion) => {
    setBusyKey(item.speaker_key);
    try {
      await onConfirm(item);
    } catch {
      toast.error('Não foi possível salvar o nome.');
    } finally {
      setBusyKey(null);
    }
  };

  return (
    <div
      className="flex flex-col gap-2 border-b bg-muted/40 px-4 py-2 text-sm"
      role="region"
      aria-label="Sugestões de nomes para as pessoas da conversa"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-2 font-medium">
          <Users className="h-4 w-4" aria-hidden="true" />
          Quem é quem? Confirme os nomes sugeridos
        </span>
        <Button variant="ghost" size="sm" onClick={onDismissAll}>
          Dispensar todas
        </Button>
      </div>
      <ul className="flex flex-col gap-1">
        {suggestions.map((item) => (
          <li key={`${item.speaker_key}:${item.name}`} className="flex flex-wrap items-center gap-2">
            <span>
              <strong>{item.label || 'Pessoa'}</strong> pode ser <strong>{item.name}</strong>
            </span>
            <span className="text-muted-foreground">({item.reason})</span>
            <span className="ml-auto flex gap-1">
              <Button
                size="sm"
                variant="outline"
                disabled={busyKey === item.speaker_key}
                onClick={() => confirm(item)}
                title={`Chamar ${item.label || 'esta pessoa'} de ${item.name}`}
              >
                <Check />
                Confirmar
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onDismiss(item)}
                title="Não é essa pessoa"
                aria-label={`Dispensar a sugestão ${item.name}`}
              >
                <X />
              </Button>
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
