'use client';

import React, { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { CornerDownLeft, FileText, Loader2, MessageCircleQuestion, Search } from 'lucide-react';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { VisuallyHidden } from '@/components/ui/visually-hidden';
import { Button } from '@/components/ui/button';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useFeatures } from '@/contexts/FeatureContext';
import { meetingMatchesQuery, meetingSubtitle, type MeetingOrg } from '@/lib/organize';

interface AskAnswer {
  answer: string;
  leavesMachine: boolean;
  snippets: Array<{ when: string; excerpt: string }>;
}

interface SearchDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onOpenMeeting: (meeting: MeetingOrg) => void;
}

/**
 * Buscar / Perguntar: filtra as reuniões por título, cliente, projeto, categoria e etiquetas,
 * procura no texto das transcrições e, com "Perguntar", pede uma resposta ao modelo já configurado.
 */
export function SearchDialog({ open, onOpenChange, onOpenMeeting }: SearchDialogProps) {
  const { organizedMeetings, searchTranscripts, searchResults, isSearching } = useSidebar();
  const { toggles } = useFeatures();
  const [query, setQuery] = useState('');
  const [answer, setAnswer] = useState<AskAnswer | null>(null);
  const [askError, setAskError] = useState<string | null>(null);
  const [isAsking, setIsAsking] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const trimmed = query.trim();

  useEffect(() => {
    if (!open) return;
    setAnswer(null);
    setAskError(null);
  }, [open]);

  // Busca no texto das transcrições com um pequeno atraso
  useEffect(() => {
    if (!open) return;
    const timer = setTimeout(() => {
      void searchTranscripts(trimmed);
    }, 250);
    return () => clearTimeout(timer);
    // searchTranscripts muda de identidade a cada render do provedor.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [trimmed, open]);

  const results = useMemo(() => {
    if (!trimmed) return organizedMeetings.slice(0, 8).map((meeting) => ({ meeting, snippet: null as string | null }));
    const transcriptHits = new Map(searchResults.map((result) => [result.id, result.matchContext]));
    return organizedMeetings
      .filter((meeting) => meetingMatchesQuery(meeting, trimmed) || transcriptHits.has(meeting.id))
      .slice(0, 30)
      .map((meeting) => ({ meeting, snippet: transcriptHits.get(meeting.id) ?? null }));
  }, [organizedMeetings, searchResults, trimmed]);

  const ask = async () => {
    if (!trimmed || isAsking) return;
    setIsAsking(true);
    setAskError(null);
    setAnswer(null);
    try {
      setAnswer(await invoke<AskAnswer>('search_meetings', { question: trimmed }));
    } catch (error) {
      setAskError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsAsking(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="top-[12%] max-w-[640px] translate-y-0 gap-0 overflow-hidden p-0 [&>button]:hidden">
        <VisuallyHidden>
          <DialogTitle>Buscar ou perguntar</DialogTitle>
          <DialogDescription>Busque nas reuniões salvas ou faça uma pergunta sobre elas.</DialogDescription>
        </VisuallyHidden>
        <form
          className="flex items-center gap-2 border-b px-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (toggles.meetingSearch) void ask();
          }}
        >
          <Search className="h-4 w-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <input
            ref={inputRef}
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Buscar reuniões ou perguntar: o que ficou combinado sobre preço?"
            aria-label="Buscar ou perguntar"
            className="h-12 min-w-0 flex-1 bg-transparent text-[15px] outline-none placeholder:text-muted-foreground focus-visible:outline-none"
          />
          {toggles.meetingSearch && (
            <Button type="submit" size="sm" disabled={!trimmed || isAsking}>
              {isAsking ? <Loader2 className="animate-spin" /> : <MessageCircleQuestion />}
              Perguntar
            </Button>
          )}
        </form>

        <div className="max-h-[60vh] overflow-y-auto custom-scrollbar">
          {(isAsking || answer || askError) && (
            <section className="border-b px-4 py-3" aria-live="polite">
              <h3 className="mb-1.5 text-xs font-medium text-muted-foreground">Resposta</h3>
              {isAsking && (
                <div className="space-y-2" aria-label="Gerando resposta">
                  <div className="skeleton h-3.5 w-11/12" />
                  <div className="skeleton h-3.5 w-9/12" />
                </div>
              )}
              {askError && <p className="text-sm text-danger">Não foi possível responder: {askError}</p>}
              {answer && (
                <div className="space-y-2">
                  <p className="reading whitespace-pre-wrap">{answer.answer}</p>
                  {answer.snippets.length > 0 && (
                    <ul className="space-y-1.5">
                      {answer.snippets.map((snippet, index) => (
                        <li key={index} className="rounded-lg bg-muted px-3 py-2 text-[13px]">
                          <span className="font-medium">{snippet.when}</span>
                          <span className="text-muted-foreground"> · {snippet.excerpt}</span>
                        </li>
                      ))}
                    </ul>
                  )}
                  {answer.leavesMachine && (
                    <p className="text-xs text-warning">
                      O texto dos trechos foi enviado ao provedor de resumo que você configurou.
                    </p>
                  )}
                </div>
              )}
            </section>
          )}

          <section className="p-2">
            <h3 className="flex items-center gap-2 px-2 py-1.5 text-xs font-medium text-muted-foreground">
              {trimmed ? 'Reuniões encontradas' : 'Recentes'}
              {isSearching && <Loader2 className="h-3 w-3 animate-spin" aria-label="Buscando nas transcrições" />}
            </h3>
            {results.length === 0 ? (
              <p className="px-2 py-6 text-center text-sm text-muted-foreground">
                {organizedMeetings.length === 0 ? 'Nenhuma reunião salva ainda.' : `Nada encontrado para “${trimmed}”.`}
              </p>
            ) : (
              <ul>
                {results.map(({ meeting, snippet }) => (
                  <li key={meeting.id}>
                    <button
                      type="button"
                      className="flex w-full items-start gap-2.5 rounded-lg px-2 py-2 text-left transition-colors hover:bg-accent focus-visible:bg-accent"
                      onClick={() => {
                        onOpenMeeting(meeting);
                        onOpenChange(false);
                      }}
                    >
                      <FileText className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm">{meeting.title}</span>
                        {meetingSubtitle(meeting) && (
                          <span className="block truncate text-xs text-muted-foreground">{meetingSubtitle(meeting)}</span>
                        )}
                        {snippet && (
                          <span className="mt-0.5 line-clamp-2 block text-xs text-muted-foreground">
                            <span className="font-medium text-warning">Trecho:</span> {snippet}
                          </span>
                        )}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>

        <footer className="flex items-center justify-between border-t px-4 py-2 text-xs text-muted-foreground">
          <span>Busca em título, cliente, projeto, categoria, etiquetas e transcrição.</span>
          {toggles.meetingSearch && (
            <span className="hidden items-center gap-1 sm:inline-flex">
              <CornerDownLeft className="h-3 w-3" aria-hidden="true" /> perguntar
            </span>
          )}
        </footer>
      </DialogContent>
    </Dialog>
  );
}
