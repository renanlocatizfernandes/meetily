// Pure helpers of the "the meeting seems to be over" warning and its settings.

export type MeetingEndReason = 'signal' | 'silence';
export type MeetingEndStopReason = MeetingEndReason | 'timeout' | 'user';

export interface MeetingEndDetected {
  promptId: number;
  reason: MeetingEndReason;
  timeoutSeconds: number;
  silenceMinutes: number;
}

export const MIN_SILENCE_MINUTES = 1;
export const MAX_SILENCE_MINUTES = 60;
export const DEFAULT_SILENCE_MINUTES = 3;

/** Whole minutes inside the accepted range, or null when the text is not a number. */
export function parseSilenceMinutes(text: string): number | null {
  const value = Number(text.trim().replace(',', '.'));
  if (text.trim() === '' || !Number.isFinite(value)) return null;
  return Math.min(MAX_SILENCE_MINUTES, Math.max(MIN_SILENCE_MINUTES, Math.round(value)));
}

/** Seconds left until `deadlineMs`, never negative. */
export function secondsLeft(deadlineMs: number, nowMs: number): number {
  return Math.max(0, Math.ceil((deadlineMs - nowMs) / 1000));
}

export function minutesLabel(minutes: number): string {
  return minutes === 1 ? '1 minuto' : `${minutes} minutos`;
}

/** What the warning says was noticed. */
export function evidenceText(event: Pick<MeetingEndDetected, 'reason' | 'silenceMinutes'>): string {
  return event.reason === 'signal'
    ? 'A chamada terminou.'
    : `Ninguém fala há ${minutesLabel(event.silenceMinutes)}.`;
}

/** Toast shown after the recording was stopped without a click, or null when no toast is due. */
export function stoppedText(reason: MeetingEndStopReason): string | null {
  switch (reason) {
    case 'signal':
      return 'A chamada terminou.';
    case 'silence':
      return 'Ninguém falava há algum tempo.';
    case 'timeout':
      return 'O aviso de fim de reunião ficou sem resposta.';
    default:
      return null;
  }
}
