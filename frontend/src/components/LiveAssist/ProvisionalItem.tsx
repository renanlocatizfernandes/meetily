'use client';

/**
 * Suggestion the model is still writing. Same box as a final item (text + action row)
 * so nothing moves when the validated item takes its place. No actions on purpose.
 */
import type { LiveAssistOrigin } from '@/lib/liveAssistContext';
import { OriginTag } from '@/components/LiveAssist/OriginTag';

export function ProvisionalItem({ text, origin }: { text: string; origin?: LiveAssistOrigin }) {
  return (
    <article
      className="rounded-lg border border-dashed border-border bg-muted/50 p-3"
      aria-live="off"
      aria-busy="true"
      title="Sugestão em andamento"
    >
      {origin && <OriginTag origin={origin} />}
      <p className="text-sm italic leading-relaxed text-muted-foreground">
        {text}
        <span
          aria-hidden="true"
          className="ml-0.5 inline-block h-3.5 w-px translate-y-0.5 animate-pulse bg-muted-foreground"
        />
      </p>
      {/* Keeps the height of the Copiar / Fixar / Descartar row of the final item. */}
      <div aria-hidden="true" className="mt-2 flex h-6 items-center">
        <span className="px-2 text-xs text-muted-foreground">Escrevendo…</span>
      </div>
    </article>
  );
}
