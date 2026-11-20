import { useCallback, useEffect, useState } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';

export interface SpeakerNameSuggestion {
  speaker_key: string;
  label: string;
  name: string;
  confidence: number;
  reason: string;
}

/** Drops suggestions the person already dismissed in this session. */
export function visibleSuggestions(
  suggestions: SpeakerNameSuggestion[],
  dismissed: ReadonlySet<string>,
): SpeakerNameSuggestion[] {
  return suggestions.filter((item) => !dismissed.has(`${item.speaker_key}:${item.name}`));
}

/**
 * Name suggestions for the speakers of a saved meeting. They are only
 * suggestions: `confirm` renames through the caller's own rename function.
 */
export function useSpeakerSuggestions(
  meetingId: string | undefined,
  transcriptCount: number,
  rename: (speakerKey: string, displayName: string) => Promise<void>,
) {
  const [suggestions, setSuggestions] = useState<SpeakerNameSuggestion[]>([]);
  const [dismissed, setDismissed] = useState<Set<string>>(new Set());

  useEffect(() => {
    setDismissed(new Set());
    if (!meetingId || transcriptCount === 0) {
      setSuggestions([]);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const found = await invokeTauri('api_suggest_speaker_names', { meetingId }) as SpeakerNameSuggestion[];
        if (!cancelled) setSuggestions(found);
      } catch (error) {
        // A convenience only: no suggestions is the same as none found.
        console.error('Speaker name suggestions failed:', error);
        if (!cancelled) setSuggestions([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [meetingId, transcriptCount > 0]); // eslint-disable-line react-hooks/exhaustive-deps

  const dismiss = useCallback((item: SpeakerNameSuggestion) => {
    setDismissed((previous) => new Set(previous).add(`${item.speaker_key}:${item.name}`));
  }, []);

  const dismissAll = useCallback(() => {
    setDismissed(new Set(suggestions.map((item) => `${item.speaker_key}:${item.name}`)));
  }, [suggestions]);

  const confirm = useCallback(
    async (item: SpeakerNameSuggestion) => {
      await rename(item.speaker_key, item.name);
      setSuggestions((previous) => previous.filter((other) => other.speaker_key !== item.speaker_key));
    },
    [rename],
  );

  return {
    suggestions: visibleSuggestions(suggestions, dismissed),
    confirm,
    dismiss,
    dismissAll,
  };
}
