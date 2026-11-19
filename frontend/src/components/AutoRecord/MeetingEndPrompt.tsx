'use client';

import { useEffect, useState } from 'react';
import { CircleStop } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { listenAll, safeInvoke } from '@/lib/tauriSafe';
import { MeetingEndDetected, MeetingEndStopReason, evidenceText, secondsLeft, stoppedText } from '@/lib/meetingEnd';

interface OpenWarning extends MeetingEndDetected {
  deadlineMs: number;
}

/**
 * The Rust core thinks the meeting is over (call gone or long silence) and
 * will stop the recording when the countdown ends. Non-blocking, and placed
 * below the top bar and the recording banner.
 */
export function MeetingEndPrompt() {
  const [warning, setWarning] = useState<OpenWarning | null>(null);
  const [answering, setAnswering] = useState(false);
  const [left, setLeft] = useState(0);

  useEffect(() => {
    return listenAll({
      'meeting-end-detected': (payload: MeetingEndDetected) => {
        if (!payload || typeof payload.promptId !== 'number') return;
        const timeoutSeconds = Number(payload.timeoutSeconds) > 0 ? Number(payload.timeoutSeconds) : 60;
        setAnswering(false);
        setLeft(timeoutSeconds);
        setWarning({ ...payload, timeoutSeconds, deadlineMs: Date.now() + timeoutSeconds * 1000 });
      },
      'meeting-end-cancelled': (payload: { promptId: number }) => {
        setWarning((current) => (current && current.promptId === payload?.promptId ? null : current));
      },
      'meeting-end-stopped': (payload: { reason: MeetingEndStopReason }) => {
        setWarning(null);
        const detail = stoppedText(payload?.reason);
        if (detail) toast.info('Gravação encerrada', { description: detail });
      },
    });
  }, []);

  useEffect(() => {
    if (!warning) return;
    const tick = () => setLeft(secondsLeft(warning.deadlineMs, Date.now()));
    tick();
    const timer = setInterval(tick, 500);
    return () => clearInterval(timer);
  }, [warning]);

  if (!warning) return null;

  const respond = async (action: 'stop' | 'continue') => {
    setAnswering(true);
    const result = await safeInvoke('respond_meeting_end', { promptId: warning.promptId, action });
    setAnswering(false);
    if (!result.ok && action === 'stop') {
      toast.error('Não foi possível encerrar por aqui', {
        description: result.missing ? 'Use o botão de parar a gravação.' : result.error,
      });
    }
    setWarning((current) => (current && current.promptId === warning.promptId ? null : current));
  };

  const progress = Math.min(100, Math.max(0, (left / warning.timeoutSeconds) * 100));

  return (
    <div
      role="alertdialog"
      aria-modal="false"
      aria-labelledby="meeting-end-title"
      aria-describedby="meeting-end-detail"
      className="fixed left-1/2 top-28 z-40 w-[min(34rem,calc(100vw-2rem))] -translate-x-1/2 overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-lg"
    >
      <div className="flex items-center gap-3 p-4">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground">
          <CircleStop className="h-4 w-4" aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <p id="meeting-end-title" className="text-sm font-semibold text-foreground">
            A reunião parece ter terminado
          </p>
          <p id="meeting-end-detail" className="text-xs text-muted-foreground">
            {evidenceText(warning)} A gravação encerra em{' '}
            <span className="font-medium tabular-nums text-foreground" aria-live="off">{left} s</span>.
          </p>
        </div>
        <div className="flex shrink-0 gap-2">
          <Button type="button" variant="ghost" size="sm" disabled={answering} onClick={() => respond('continue')}>
            Continuar gravando
          </Button>
          <Button type="button" size="sm" disabled={answering} onClick={() => respond('stop')}>
            Encerrar agora
          </Button>
        </div>
      </div>
      <div className="h-1 w-full bg-muted" aria-hidden="true">
        <div className="h-full bg-primary transition-[width] duration-500 ease-linear" style={{ width: `${progress}%` }} />
      </div>
    </div>
  );
}
