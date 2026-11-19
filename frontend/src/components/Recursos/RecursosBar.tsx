'use client';

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { FeatureToggles, useFeatures } from '@/contexts/FeatureContext';
import { useLiveAssist } from '@/contexts/LiveAssistContext';
import { useTranscripts } from '@/contexts/TranscriptContext';

const SWITCHES: Array<{ key: keyof FeatureToggles; label: string }> = [
  { key: 'internetSearch', label: 'Pesquisa' },
  { key: 'autoTasks', label: 'Tarefas' },
  { key: 'meetingMemory', label: 'Memória' },
  { key: 'calendarPrep', label: 'Agenda' },
  { key: 'voiceProfiles', label: 'Nomes' },
  { key: 'commitmentAlerts', label: 'Alertas' },
  { key: 'followupEmail', label: 'E-mail' },
  { key: 'meetingSearch', label: 'Busca' },
  { key: 'scoreboard', label: 'Placar' },
];

export function RecursosBar() {
  const { toggles, setToggle } = useFeatures();
  const { settings, saveSettings } = useLiveAssist();
  const { transcripts, meetingTitle } = useTranscripts();
  const [open, setOpen] = useState(false);
  const [question, setQuestion] = useState('');
  const [result, setResult] = useState('');
  const [memory, setMemory] = useState('');

  const flip = async (key: keyof FeatureToggles) => {
    try {
      await setToggle({ [key]: !toggles[key] } as Partial<FeatureToggles>);
    } catch {
      toast.error('Não foi possível salvar');
    }
  };

  const transcriptText = transcripts.map((item) => `${item.speaker || ''}: ${item.text}`).join('\n');

  return (
    <div className="relative">
      <button
        type="button"
        className="rounded-full border bg-card px-3 py-1 text-xs font-medium text-foreground shadow-sm hover:bg-accent"
        onClick={() => setOpen((value) => !value)}
      >
        Recursos
      </button>
      {open && (
        <div className="absolute bottom-10 left-0 z-40 w-[340px] space-y-3 rounded-xl border bg-popover p-3 text-left text-popover-foreground shadow-lg">
          <p className="text-xs text-muted-foreground">Um clique liga ou desliga. A pesquisa manda só o termo para a Wikipédia ou o DuckDuckGo.</p>
          <div className="flex flex-wrap gap-1">
            {SWITCHES.map((item) => (
              <button
                key={item.key}
                type="button"
                onClick={() => flip(item.key)}
                className={`rounded-full px-2 py-1 text-xs ${toggles[item.key] ? 'bg-primary/15 text-primary' : 'bg-muted text-muted-foreground'}`}
              >
                {item.label}
              </button>
            ))}
            <button
              type="button"
              onClick={() => setToggle({ translation: toggles.translation === 'off' ? 'auto' : 'off' }).catch(() => toast.error('Não foi possível salvar'))}
              className={`rounded-full px-2 py-1 text-xs ${toggles.translation === 'off' ? 'bg-muted text-muted-foreground' : 'bg-primary/15 text-primary'}`}
            >
              Tradução {toggles.translation === 'on' ? 'sempre' : toggles.translation === 'auto' ? 'auto' : 'off'}
            </button>
          </div>
          <div className="flex flex-wrap gap-1">
            <button type="button" className="rounded-md bg-muted px-2 py-1 text-xs text-foreground hover:bg-accent" onClick={async () => {
              try {
                const prepared = await invoke<{ context: string }>('prepare_from_calendar');
                await saveSettings({ ...settings, meetingContext: prepared.context });
                toast.success('Contexto preenchido pela agenda');
              } catch (error) {
                toast.error(String(error));
              }
            }}>Pela agenda</button>
            <label className="rounded-md bg-muted px-2 py-1 text-xs text-foreground hover:bg-accent">
              .ics
              <input type="file" accept=".ics,text/calendar" className="hidden" onChange={async (event) => {
                const file = event.target.files?.[0];
                if (!file) return;
                try {
                  const prepared = await invoke<{ context: string }>('prepare_from_ics', { ics: await file.text() });
                  await saveSettings({ ...settings, meetingContext: prepared.context });
                  toast.success('Contexto preenchido pelo .ics');
                } catch (error) {
                  toast.error(String(error));
                }
              }} />
            </label>
            <button type="button" className="rounded-md bg-muted px-2 py-1 text-xs text-foreground hover:bg-accent" onClick={async () => {
              try {
                const hits = await invoke<Array<{ title: string; excerpt: string }>>('recall_meetings', { hint: meetingTitle || settings.meetingContext });
                setMemory(hits.slice(0, 2).map((hit) => `${hit.title}: ${hit.excerpt}`).join('\n') || 'Nada parecido nas reuniões salvas.');
              } catch (error) {
                toast.error(String(error));
              }
            }}>Última vez</button>
            <button type="button" className="rounded-md bg-muted px-2 py-1 text-xs text-foreground hover:bg-accent" onClick={async () => {
              try {
                const board = await invoke<{ talk: Array<{ speaker: string; seconds: number }>; unanswered: string[]; coverage: number }>('meeting_scoreboard', {
                  turns: transcripts.map((item) => ({
                    speaker: item.speaker || '',
                    seconds: item.duration || 0,
                    text: item.text,
                    at: item.audio_start_time || 0,
                  })),
                  agenda: settings.meetingContext,
                });
                setResult(`Cobertura ${(board.coverage * 100).toFixed(0)}%. ${board.talk.map((item) => `${item.speaker} ${Math.round(item.seconds)}s`).join(', ')}. Sem resposta: ${board.unanswered.join(' | ') || 'nenhuma'}`);
              } catch (error) {
                toast.error(String(error));
              }
            }}>Placar</button>
            <button type="button" className="rounded-md bg-muted px-2 py-1 text-xs text-foreground hover:bg-accent" onClick={async () => {
              try {
                const path = await invoke<string>('export_action_ics', { text: transcriptText });
                toast.success(`Tarefas em ${path}`);
              } catch (error) {
                toast.error(String(error));
              }
            }}>Tarefas .ics</button>
            <button type="button" className="rounded-md bg-muted px-2 py-1 text-xs text-foreground hover:bg-accent" onClick={async () => {
              try {
                const draft = await invoke<{ body: string; mailto: string }>('draft_followup', { title: meetingTitle, summary: transcriptText.slice(0, 4000) });
                await navigator.clipboard.writeText(draft.body);
                window.location.href = draft.mailto;
                toast.success('Rascunho copiado. O app de e-mail abre sem enviar.');
              } catch (error) {
                toast.error(String(error));
              }
            }}>E-mail</button>
          </div>
          <form className="flex gap-1" onSubmit={async (event) => {
            event.preventDefault();
            try {
              const answer = await invoke<{ answer: string; leavesMachine: boolean; snippets: Array<{ when: string; excerpt: string }> }>('search_meetings', { question });
              const cites = answer.snippets.map((snippet) => `${snippet.when}: ${snippet.excerpt}`).join('\n');
              setResult(`${answer.answer}\n${cites}${answer.leavesMachine ? '\nO texto dos trechos foi para o provedor de resumo já configurado.' : ''}`);
            } catch (error) {
              toast.error(String(error));
            }
          }}>
            <input value={question} onChange={(event) => setQuestion(event.target.value)} placeholder="O que ficou combinado sobre preço?" className="min-w-0 flex-1 rounded border border-input bg-background px-2 py-1 text-xs" />
            <button type="submit" className="rounded-md bg-primary px-2 py-1 text-xs text-primary-foreground">Buscar</button>
          </form>
          {memory && <p className="whitespace-pre-wrap text-xs text-muted-foreground">{memory}</p>}
          {result && <p className="whitespace-pre-wrap text-xs text-muted-foreground">{result}</p>}
        </div>
      )}
    </div>
  );
}

export function RecursosSettings() {
  const { toggles, setToggle } = useFeatures();
  return (
    <div className="mt-6 space-y-4 rounded-xl border bg-card p-5">
      <div>
        <h2 className="text-lg font-semibold text-foreground">Recursos</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Tudo começa ligado, menos a tradução, que fica automática: só entra quando a fala parece inglês ou espanhol. A pesquisa na internet manda apenas o termo para a Wikipédia e o DuckDuckGo, sem chave. O resto fica nesta máquina, salvo o provedor de resumo que você já configurou.
        </p>
      </div>
      {SWITCHES.map((item) => (
        <label key={item.key} className="flex items-center justify-between gap-4 text-sm">
          <span>{item.label}</span>
          <input type="checkbox" checked={Boolean(toggles[item.key])} onChange={() => setToggle({ [item.key]: !toggles[item.key] }).catch(() => toast.error('Não foi possível salvar'))} />
        </label>
      ))}
      <label className="flex items-center justify-between gap-4 text-sm">
        <span>Tradução ao vivo</span>
        <select
          className="rounded border border-input bg-background px-2 py-1"
          value={toggles.translation}
          onChange={(event) => setToggle({ translation: event.target.value as FeatureToggles['translation'] }).catch(() => toast.error('Não foi possível salvar'))}
        >
          <option value="auto">Automática</option>
          <option value="on">Sempre</option>
          <option value="off">Desligada</option>
        </select>
      </label>
      <p className="text-sm text-amber-800">Pesquisa na internet: só o termo sai da máquina. Avise as outras pessoas de que a reunião está sendo gravada.</p>
    </div>
  );
}
