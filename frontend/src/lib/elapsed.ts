/** Recording clock: "mm:ss", or "h:mm:ss" from one hour on. */
export function formatElapsed(seconds: number | null | undefined): string {
  const total = Math.max(0, Math.floor(Number.isFinite(seconds) ? (seconds as number) : 0));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = String(total % 60).padStart(2, '0');
  if (hours > 0) return `${hours}:${String(minutes).padStart(2, '0')}:${secs}`;
  return `${String(minutes).padStart(2, '0')}:${secs}`;
}
