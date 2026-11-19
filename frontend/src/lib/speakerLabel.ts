export function speakerDisplay(speaker?: string | null): string | null {
  if (!speaker) return null;
  if (speaker === 'mic') return 'Você';
  if (speaker === 'system') return 'Participantes';
  return speaker;
}
