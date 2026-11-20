import type { Transcript } from '@/types';

export const AUTO_TEMPLATE_KEY = 'autoSummaryTemplate';
/** The default template is picked when detection finds nothing clear. */
export const DEFAULT_TEMPLATE_ID = 'standard_meeting';
/** Enough text for the keyword score; a long meeting does not need to cross the bridge whole. */
export const DETECTION_TEXT_LIMIT = 40000;
/** Below this many characters there is too little to tell meeting types apart. */
export const DETECTION_MIN_CHARS = 200;

export interface TemplateSuggestion {
  template_id: string;
  template_name: string;
  confidence: number;
  detected: boolean;
}

/** Auto selection is on unless the person turned it off. */
export function readAutoTemplate(): boolean {
  if (typeof window === 'undefined') return true;
  try {
    return window.localStorage.getItem(AUTO_TEMPLATE_KEY) !== 'off';
  } catch {
    return true;
  }
}

export function writeAutoTemplate(enabled: boolean): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(AUTO_TEMPLATE_KEY, enabled ? 'on' : 'off');
  } catch {
    // Storage can be blocked; the choice then lasts for this session only.
  }
}

/** Plain text for the detector, cut at the limit. */
export function buildDetectionText(transcripts: Pick<Transcript, 'text'>[]): string {
  let text = '';
  for (const segment of transcripts) {
    const piece = segment.text?.trim();
    if (!piece) continue;
    text += (text ? '\n' : '') + piece;
    if (text.length >= DETECTION_TEXT_LIMIT) return text.slice(0, DETECTION_TEXT_LIMIT);
  }
  return text;
}

/** Whether a suggestion should replace the current template. */
export function shouldApplySuggestion(
  suggestion: TemplateSuggestion,
  availableIds: string[],
  currentId: string,
): boolean {
  return (
    suggestion.detected &&
    suggestion.template_id !== currentId &&
    availableIds.includes(suggestion.template_id)
  );
}
