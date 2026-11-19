'use client';

import { useEffect, useState } from 'react';
import { Mic } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { listenAll, safeInvoke } from '@/lib/tauriSafe';

interface RecordPrompt {
  promptId: number;
  app: string;
}

/**
 * "Ask before recording" mode: the Rust core detected a meeting and waits for an answer.
 * Non-blocking: the rest of the app stays usable while the prompt is on screen.
 */
export function AutoRecordPrompt() {
  const [prompt, setPrompt] = useState<RecordPrompt | null>(null);
  const [answering, setAnswering] = useState(false);

  useEffect(() => {
    return listenAll({
      'auto-record-prompt': (payload: RecordPrompt) => {
        if (!payload || typeof payload.promptId !== 'number') return;
        setAnswering(false);
        setPrompt(payload);
      },
      'auto-record-prompt-cancelled': (payload: { promptId: number }) => {
        setPrompt((current) => (current && current.promptId === payload?.promptId ? null : current));
      },
    });
  }, []);

  if (!prompt) return null;

  const answer = async (accept: boolean) => {
    setAnswering(true);
    const result = await safeInvoke('confirm_auto_recording', { promptId: prompt.promptId, accept });
    setAnswering(false);
    if (!result.ok && accept) {
      toast.error('Não foi possível começar a gravação', {
        description: result.missing ? 'Comece a gravação pelo botão de gravar.' : result.error,
      });
    }
    setPrompt((current) => (current && current.promptId === prompt.promptId ? null : current));
  };

  return (
    <div
      role="alertdialog"
      aria-modal="false"
      aria-labelledby="auto-record-prompt-title"
      aria-describedby="auto-record-prompt-detail"
      className="fixed left-1/2 top-4 z-40 flex w-[min(34rem,calc(100vw-2rem))] -translate-x-1/2 items-center gap-3 rounded-xl border border-border bg-popover p-4 text-popover-foreground shadow-lg"
    >
      <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground">
        <Mic className="h-4 w-4" />
      </span>
      <div className="min-w-0 flex-1">
        <p id="auto-record-prompt-title" className="text-sm font-semibold text-foreground">
          Reunião detectada no {prompt.app || 'app de reuniões'}. Gravar?
        </p>
        <p id="auto-record-prompt-detail" className="text-xs text-muted-foreground">
          Se gravar, avise as outras pessoas.
        </p>
      </div>
      <div className="flex shrink-0 gap-2">
        <Button type="button" variant="ghost" size="sm" disabled={answering} onClick={() => answer(false)}>
          Agora não
        </Button>
        <Button type="button" size="sm" disabled={answering} onClick={() => answer(true)} autoFocus>
          Gravar
        </Button>
      </div>
    </div>
  );
}
