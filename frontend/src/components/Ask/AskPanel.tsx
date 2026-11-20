'use client';

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { ArrowUp, FileText, Loader2, MessageCircleQuestion, RefreshCw, TriangleAlert } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { TopBar } from '@/components/Shell/TopBar';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import {
  askMeetings,
  citationLocation,
  consumptionNotice,
  EXAMPLE_QUESTIONS,
  errorMessage,
  formatTokens,
  loadAskStatus,
  meetingHref,
  providerLabel,
  rebuildAskIndex,
  splitAnswer,
  toHistory,
  type AskCitation,
  type AskResponse,
  type AskStatus,
} from '@/lib/ask';

interface Turn {
  id: number;
  question: string;
  response: AskResponse | null;
  error: string | null;
  pending: boolean;
}

function formatDate(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? '' : date.toLocaleDateString('pt-BR');
}

interface AskPanelProps {
  /** Restringe as perguntas a uma reunião (opcional). */
  meetingId?: string | null;
}

/** Chat "Pergunte às suas reuniões": busca local primeiro; o modelo configurado só redige a resposta. */
export function AskPanel({ meetingId = null }: AskPanelProps) {
  const router = useRouter();
  const { setCurrentMeeting } = useSidebar();
  const [status, setStatus] = useState<AskStatus | null>(null);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [input, setInput] = useState('');
  const [useLlm, setUseLlm] = useState(true);
  const [rebuilding, setRebuilding] = useState(false);
  const nextId = useRef(1);
  const bottomRef = useRef<HTMLDivElement>(null);
  const busy = turns.some((turn) => turn.pending);

  const refreshStatus = useCallback(async () => {
    const next = await loadAskStatus();
    setStatus(next);
    return next;
  }, []);

  useEffect(() => {
    void refreshStatus().then((next) => {
      // Sem modelo utilizável, a opção começa desligada: só trechos.
      if (next && !next.llmConfigured) setUseLlm(false);
    });
  }, [refreshStatus]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  }, [turns]);

  const notice = useMemo(() => consumptionNotice(status), [status]);

  const openCitation = useCallback(
    (citation: AskCitation) => {
      setCurrentMeeting({ id: citation.meetingId, title: citation.meetingTitle });
      router.push(meetingHref(citation.meetingId));
    },
    [router, setCurrentMeeting],
  );

  const send = useCallback(
    async (raw: string) => {
      const question = raw.trim();
      if (!question || busy) return;
      const id = nextId.current++;
      const history = toHistory(turns);
      setInput('');
      setTurns((current) => [...current, { id, question, response: null, error: null, pending: true }]);
      try {
        const response = await askMeetings(question, useLlm, history, meetingId);
        setTurns((current) => current.map((turn) => (turn.id === id ? { ...turn, response, pending: false } : turn)));
        void refreshStatus();
      } catch (error) {
        setTurns((current) =>
          current.map((turn) => (turn.id === id ? { ...turn, error: errorMessage(error), pending: false } : turn)),
        );
      }
    },
    [busy, meetingId, refreshStatus, turns, useLlm],
  );

  const rebuild = useCallback(async () => {
    setRebuilding(true);
    try {
      const count = await rebuildAskIndex();
      toast.success(`Índice refeito: ${count} ${count === 1 ? 'reunião' : 'reuniões'}.`);
      await refreshStatus();
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setRebuilding(false);
    }
  }, [refreshStatus]);

  const empty = turns.length === 0;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <TopBar
        title="Perguntar às reuniões"
        actions={
          <Button variant="ghost" size="sm" onClick={() => void rebuild()} disabled={rebuilding} title="Refazer o índice de busca">
            {rebuilding ? <Loader2 className="animate-spin" /> : <RefreshCw />}
            Refazer índice
          </Button>
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto custom-scrollbar">
        <div className="mx-auto w-full max-w-3xl space-y-6 px-4 py-6">
          {empty && (
            <section className="space-y-4 py-8 text-center">
              <MessageCircleQuestion className="mx-auto h-8 w-8 text-muted-foreground" aria-hidden="true" />
              <div>
                <h2 className="text-lg font-semibold">Pergunte sobre qualquer reunião gravada</h2>
                <p className="mx-auto mt-1 max-w-md text-sm text-muted-foreground">
                  A busca acontece nesta máquina, no texto das transcrições e dos resumos. Cada resposta mostra de onde veio, e você
                  abre a reunião com um clique.
                </p>
              </div>
              <ul className="mx-auto flex max-w-xl flex-wrap justify-center gap-2">
                {EXAMPLE_QUESTIONS.map((example) => (
                  <li key={example}>
                    <button
                      type="button"
                      className="rounded-full border px-3 py-1.5 text-[13px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                      onClick={() => void send(example)}
                    >
                      {example}
                    </button>
                  </li>
                ))}
              </ul>
              {status && status.totalMeetings === 0 && (
                <p className="text-sm text-muted-foreground">Ainda não há reuniões salvas para consultar.</p>
              )}
            </section>
          )}

          {turns.map((turn) => (
            <TurnView key={turn.id} turn={turn} onOpen={openCitation} />
          ))}
          <div ref={bottomRef} />
        </div>
      </div>

      <div className="shrink-0 border-t bg-background">
        <div className="mx-auto w-full max-w-3xl space-y-2 px-4 py-3">
          {useLlm && notice && (
            <p className="flex items-start gap-2 rounded-lg bg-muted px-3 py-2 text-xs text-warning" role="note">
              <TriangleAlert className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              <span>{notice}</span>
            </p>
          )}
          <form
            className="flex items-end gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              void send(input);
            }}
          >
            <Textarea
              value={input}
              onChange={(event) => setInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  void send(input);
                }
              }}
              rows={1}
              maxLength={500}
              placeholder={meetingId ? 'Pergunte sobre esta reunião' : 'O que decidimos com o cliente X? Quais tarefas estão comigo?'}
              aria-label="Pergunta sobre as reuniões"
              className="max-h-40 min-h-[40px] flex-1 resize-none"
            />
            <Button type="submit" size="icon" disabled={!input.trim() || busy} aria-label="Enviar pergunta">
              {busy ? <Loader2 className="animate-spin" /> : <ArrowUp />}
            </Button>
          </form>
          <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
            <label className="flex items-center gap-2">
              <Switch checked={useLlm} onCheckedChange={setUseLlm} aria-label="Redigir resposta com o modelo" />
              <span>
                Redigir resposta
                {status ? (
                  <>
                    {' '}
                    com {providerLabel(status.provider)}
                    {!status.llmConfigured && ' (indisponível: só trechos)'}
                  </>
                ) : null}
              </span>
            </label>
            {status && (
              <span>
                {status.indexedMeetings} de {status.totalMeetings} reuniões indexadas
                {status.searchMode === 'basic' && ' · busca simples'}
              </span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function TurnView({ turn, onOpen }: { turn: Turn; onOpen: (citation: AskCitation) => void }) {
  const { response } = turn;
  return (
    <article className="space-y-3">
      <p className="ml-auto w-fit max-w-[85%] rounded-2xl bg-primary px-3.5 py-2 text-sm text-primary-foreground">{turn.question}</p>

      {turn.pending && (
        <div className="space-y-2" aria-label="Buscando nas reuniões" aria-live="polite">
          <div className="skeleton h-3.5 w-11/12" />
          <div className="skeleton h-3.5 w-8/12" />
        </div>
      )}

      {turn.error && <p className="text-sm text-danger">Não foi possível responder: {turn.error}</p>}

      {response && <AssistantView response={response} onOpen={onOpen} />}
    </article>
  );
}

function AssistantView({ response, onOpen }: { response: AskResponse; onOpen: (citation: AskCitation) => void }) {
  const parts = response.answer ? splitAnswer(response.answer, response.citations.length) : [];
  const scope = [response.periodLabel, response.clientFilter].filter(Boolean).join(' · ');

  return (
    <div className="space-y-3">
      {scope && <p className="text-xs text-muted-foreground">Filtro entendido na pergunta: {scope}</p>}

      {response.answerStatus === 'no_results' && (
        <p className="text-sm text-muted-foreground">
          Não encontrei isso nas reuniões salvas{response.periodLabel ? ` (${response.periodLabel})` : ''}. Tente outras palavras ou
          o nome do cliente.
        </p>
      )}

      {response.answer && (
        <p className="reading whitespace-pre-wrap" aria-live="polite">
          {parts.map((part, index) => {
            if (part.type === 'text') return <React.Fragment key={index}>{part.text}</React.Fragment>;
            const citation = response.citations[part.n - 1];
            return (
              <button
                key={index}
                type="button"
                onClick={() => onOpen(citation)}
                title={`${citation.meetingTitle} · ${citationLocation(citation)}`}
                className="mx-0.5 inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-md bg-muted px-1 align-baseline text-[11px] font-medium text-foreground transition-colors hover:bg-accent"
              >
                {part.n}
              </button>
            );
          })}
        </p>
      )}

      {response.answerNote && <p className="text-xs text-warning">{response.answerNote}</p>}

      {response.leavesMachine && (
        <p className="text-xs text-warning">
          Os trechos abaixo foram enviados a {providerLabel(response.provider)}
          {response.estimatedTokens > 0 && <> (cerca de {formatTokens(response.estimatedTokens)} de tokens)</>}. Provedores pagos
          cobram por uso.
        </p>
      )}

      {response.citations.length > 0 && (
        <section aria-label="Fontes">
          <h3 className="mb-1.5 text-xs font-medium text-muted-foreground">
            {response.answerStatus === 'generated' ? 'Fontes' : 'Trechos encontrados'}
          </h3>
          <ol className="space-y-1.5">
            {response.citations.map((citation) => (
              <li key={citation.n}>
                <button
                  type="button"
                  onClick={() => onOpen(citation)}
                  className="flex w-full items-start gap-2.5 rounded-lg border px-3 py-2 text-left transition-colors hover:bg-accent focus-visible:bg-accent"
                >
                  <span className="mt-0.5 inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-md bg-muted px-1 text-[11px] font-medium">
                    {citation.n}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-1.5 text-[13px] font-medium">
                      <FileText className="h-3.5 w-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
                      <span className="truncate">{citation.meetingTitle}</span>
                    </span>
                    <span className="block text-xs text-muted-foreground">
                      {[citation.client, formatDate(citation.meetingDate), citationLocation(citation)].filter(Boolean).join(' · ')}
                    </span>
                    <span className="mt-1 line-clamp-3 block whitespace-pre-wrap text-[13px] text-muted-foreground">
                      {citation.excerpt}
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ol>
        </section>
      )}
    </div>
  );
}
