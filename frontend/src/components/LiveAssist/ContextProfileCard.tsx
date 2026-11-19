'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Switch } from '@/components/ui/switch';
import { SettingsCard } from '@/components/LiveAssist/SettingsCard';
import {
  DEFAULT_CONTEXT_SETTINGS,
  GLOSSARY_MAX_CHARS,
  LiveAssistContextSettings,
  countGlossaryTerms,
} from '@/lib/liveAssistContext';

const FIELD = 'w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground';

/** Owner profile, glossary and the switch for earlier meetings of the same client. */
export function ContextProfileCard() {
  const [draft, setDraft] = useState<LiveAssistContextSettings>(DEFAULT_CONTEXT_SETTINGS);
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    invoke<LiveAssistContextSettings>('get_live_assist_context_settings')
      .then((settings) => {
        if (!cancelled && settings) setDraft({ ...DEFAULT_CONTEXT_SETTINGS, ...settings });
      })
      .catch((error) => console.error('Falha ao carregar o perfil da ajuda ao vivo', error))
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const save = async () => {
    setSaving(true);
    try {
      const saved = await invoke<LiveAssistContextSettings>('save_live_assist_context_settings', { settings: draft });
      setDraft({ ...DEFAULT_CONTEXT_SETTINGS, ...saved });
      toast.success('Perfil salvo');
    } catch (error) {
      toast.error('Não foi possível salvar o perfil');
      console.error(error);
    } finally {
      setSaving(false);
    }
  };

  const terms = countGlossaryTerms(draft.glossary);
  const disabled = !draft.profileEnabled;

  return (
    <SettingsCard
      title="Perfil e contexto das sugestões"
      description="Vale para todas as reuniões e fica só neste computador. A ajuda ao vivo usa estes dados para explicar termos e responder sobre a sua empresa, e mostra a origem em cada sugestão. O contexto de uma reunião específica continua no campo acima."
    >
      <label className="flex items-center justify-between gap-4">
        <span className="text-sm font-medium text-foreground">Usar meu perfil e glossário</span>
        <Switch checked={draft.profileEnabled} onCheckedChange={(profileEnabled) => setDraft({ ...draft, profileEnabled })} />
      </label>

      <label className="block space-y-1">
        <span className="text-sm font-medium text-foreground">Quem sou</span>
        <input
          className={FIELD}
          value={draft.about}
          disabled={disabled}
          onChange={(event) => setDraft({ ...draft, about: event.target.value })}
          placeholder="Nome e papel. Exemplo: Renan, consultor de dados"
          maxLength={300}
        />
      </label>

      <label className="block space-y-1">
        <span className="text-sm font-medium text-foreground">Empresa</span>
        <input
          className={FIELD}
          value={draft.company}
          disabled={disabled}
          onChange={(event) => setDraft({ ...draft, company: event.target.value })}
          placeholder="Nome e o que a empresa faz"
          maxLength={300}
        />
      </label>

      <label className="block space-y-1">
        <span className="text-sm font-medium text-foreground">Produtos e serviços</span>
        <textarea
          className={`${FIELD} min-h-20`}
          value={draft.offerings}
          disabled={disabled}
          onChange={(event) => setDraft({ ...draft, offerings: event.target.value })}
          placeholder="O que você oferece, em poucas linhas"
          maxLength={600}
        />
      </label>

      <label className="block space-y-1">
        <span className="text-sm font-medium text-foreground">Glossário de termos e siglas</span>
        <textarea
          className={`${FIELD} min-h-32 font-mono`}
          value={draft.glossary}
          disabled={disabled}
          onChange={(event) => setDraft({ ...draft, glossary: event.target.value })}
          placeholder={'SLA: acordo de nível de serviço\nFinOps: gestão de custo de nuvem'}
          maxLength={GLOSSARY_MAX_CHARS}
        />
        <span className="text-xs text-muted-foreground">
          Um termo por linha, no formato TERMO: explicação. {terms === 1 ? '1 termo reconhecido' : `${terms} termos reconhecidos`}. Em cada sugestão entram só os termos ditos na conversa, até 4.
        </span>
      </label>

      <label className="flex items-start justify-between gap-4 border-t border-border pt-4">
        <span>
          <span className="block text-sm font-medium text-foreground">Usar reuniões anteriores do mesmo cliente</span>
          <span className="mt-1 block text-xs text-muted-foreground">
            Lê decisões e pendências dos resumos já salvos das últimas 4 reuniões do cliente e cita a data. Depende do recurso Memória estar ligado. O cliente é reconhecido pelo título, pelo contexto da reunião ou pela conversa.
          </span>
        </span>
        <Switch checked={draft.usePastMeetings} onCheckedChange={(usePastMeetings) => setDraft({ ...draft, usePastMeetings })} />
      </label>

      <p className="text-xs text-muted-foreground">
        Termos do glossário e combinados anteriores que batem com a conversa aparecem na hora, sem passar pelo modelo. O contexto enviado ao modelo ocupa no máximo 900 caracteres por sugestão, para não deixar um modelo pequeno mais lento. Com um provedor na nuvem, o perfil e as reuniões anteriores não são enviados.
      </p>

      <button
        type="button"
        onClick={save}
        disabled={saving || !loaded}
        className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground disabled:opacity-60"
      >
        {saving ? 'Salvando…' : 'Salvar perfil'}
      </button>
    </SettingsCard>
  );
}
