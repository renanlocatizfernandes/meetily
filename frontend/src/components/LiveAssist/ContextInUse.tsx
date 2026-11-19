'use client';

import { useState } from 'react';
import { ChevronDown, ChevronRight, Layers } from 'lucide-react';
import {
  LiveAssistContextSummary,
  contextHeadline,
  pastMeetingsStatus,
  profileStatus,
} from '@/lib/liveAssistContext';

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex gap-2">
      <dt className="w-28 shrink-0 text-muted-foreground">{label}</dt>
      <dd className="min-w-0 flex-1 break-words text-foreground">{value}</dd>
    </div>
  );
}

function capitalize(text: string) {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** Compact, collapsible summary of what the suggestions are anchored on. */
export function ContextInUse({ summary }: { summary: LiveAssistContextSummary | null }) {
  const [open, setOpen] = useState(false);
  const Chevron = open ? ChevronDown : ChevronRight;

  return (
    <div className="mt-3 rounded-lg border border-border bg-card text-card-foreground">
      <button
        type="button"
        className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-xs hover:bg-muted"
        onClick={() => setOpen((current) => !current)}
        aria-expanded={open}
        aria-controls="live-assist-context-in-use"
      >
        <Layers className="h-3.5 w-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
        <span className="shrink-0 font-semibold text-foreground">Contexto em uso</span>
        <span className="min-w-0 flex-1 truncate text-muted-foreground">{contextHeadline(summary)}</span>
        <Chevron className="h-3.5 w-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
      </button>
      {open && (
        <div id="live-assist-context-in-use" className="border-t border-border px-3 py-2 text-xs">
          {summary ? (
            <dl className="space-y-1">
              <Row label="Cliente" value={summary.client ?? 'Ainda não identificado'} />
              {summary.project && <Row label="Projeto" value={summary.project} />}
              {summary.category && <Row label="Categoria" value={summary.category} />}
              {summary.tags.length > 0 && <Row label="Etiquetas" value={summary.tags.join(', ')} />}
              {summary.meetingName && <Row label="Assunto" value={summary.meetingName} />}
              {summary.speakers.length > 0 && <Row label="Quem fala" value={summary.speakers.join(', ')} />}
              {summary.localOnlyBlocked ? (
                <p className="pt-1 text-muted-foreground">
                  O modelo atual é um serviço na nuvem. O perfil, o glossário e as reuniões anteriores não são enviados a ele. Termos do glossário e combinados anteriores citados na conversa continuam aparecendo, porque são achados neste computador.
                </p>
              ) : (
                <>
                  <Row label="Reuniões anteriores" value={pastMeetingsStatus(summary)} />
                  <Row
                    label="Perfil"
                    value={`${capitalize(profileStatus(summary))}${summary.profileEnabled && summary.glossaryTerms > 0 ? `, ${summary.glossaryTerms} ${summary.glossaryTerms === 1 ? 'termo' : 'termos'} no glossário` : ''}`}
                  />
                </>
              )}
              {!summary.client && (
                <p className="pt-1 text-muted-foreground">
                  Para ligar a reunião a um cliente, cite o nome dele no contexto da reunião ou no título. Só clientes já usados em reuniões salvas são reconhecidos.
                </p>
              )}
            </dl>
          ) : (
            <p className="text-muted-foreground">O contexto aparece aqui quando a gravação começa.</p>
          )}
        </div>
      )}
    </div>
  );
}
