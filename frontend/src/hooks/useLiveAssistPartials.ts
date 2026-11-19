'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { listenAll } from '@/lib/tauriSafe';
import type { SyncSection } from '@/lib/liveAssistSync';
import { LiveAssistOrigin, parseOrigin } from '@/lib/liveAssistContext';

export interface PartialItem {
  section: SyncSection;
  text: string;
  origin?: LiveAssistOrigin;
}

interface PartialPayload {
  requestId: number;
  items?: Array<{ kind: string; text: string; origin?: unknown }>;
  done?: boolean;
}

const KIND_TO_SECTION: Record<string, SyncSection> = {
  pergunta: 'perguntas',
  perguntas: 'perguntas',
  resposta: 'respostas',
  respostas: 'respostas',
  contexto: 'contexto',
};

// After `done`, the validated items arrive in `live-assist-update`. Waiting a moment
// lets them replace the provisional rows in place instead of leaving a gap.
const DONE_GRACE_MS = 600;
// A stream that stops without `done` (recording ended, model died) must not stay on screen.
const STALE_MS = 15000;
const RESTART_MS = 5000;

/**
 * Items the model is still writing (`live-assist-partial`).
 * `settle(requestId)` must be called when the final update of a request arrives.
 */
export function useLiveAssistPartials() {
  const [partials, setPartials] = useState<PartialItem[]>([]);
  const requestId = useRef<number | null>(null);
  const settledId = useRef<number | null>(null);
  const lastSeenAt = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clear = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    setPartials((current) => (current.length === 0 ? current : []));
  }, []);

  const clearLater = useCallback((delay: number) => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      timer.current = null;
      setPartials([]);
    }, delay);
  }, []);

  /** The final, validated items of `finalId` arrived (or of any request when undefined). */
  const settle = useCallback((finalId?: number | null) => {
    if (typeof finalId === 'number' && requestId.current !== null && finalId < requestId.current) return;
    settledId.current = typeof finalId === 'number' ? finalId : requestId.current;
    clear();
  }, [clear]);

  useEffect(() => {
    const stop = listenAll({
      'live-assist-partial': (payload: PartialPayload) => {
        if (!payload || typeof payload.requestId !== 'number') return;
        const now = Date.now();
        // Ids going back after a quiet period mean the counter restarted (new recording).
        if (requestId.current !== null && payload.requestId < requestId.current && now - lastSeenAt.current > RESTART_MS) {
          requestId.current = null;
          settledId.current = null;
        }
        // Late chunk of a request that was already replaced or already has its final items.
        if (requestId.current !== null && payload.requestId < requestId.current) return;
        if (settledId.current !== null && payload.requestId <= settledId.current) return;
        requestId.current = payload.requestId;
        lastSeenAt.current = now;
        const items: PartialItem[] = (payload.items ?? [])
          .map((item) => ({ section: KIND_TO_SECTION[item?.kind], text: (item?.text ?? '').trim(), origin: parseOrigin(item?.origin) }))
          .filter((item) => Boolean(item.section) && item.text.length > 0);
        if (payload.done) {
          if (items.length > 0) setPartials(items);
          clearLater(DONE_GRACE_MS);
          return;
        }
        setPartials(items);
        clearLater(STALE_MS);
      },
    });
    return () => {
      stop();
      if (timer.current) clearTimeout(timer.current);
    };
  }, [clearLater]);

  return { partials, settle };
}
