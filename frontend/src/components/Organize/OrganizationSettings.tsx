'use client';

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Loader2, Plus, Sparkles, X } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { Switch } from '@/components/ui/switch';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import {
  DEFAULT_ORGANIZATION_SETTINGS,
  TITLE_PLACEHOLDERS,
  cancelOrganizeAll,
  getOrganizationSettings,
  listenOrganizeEvents,
  organizeAllMeetings,
  previewTitleTemplate,
  saveOrganizationSettings,
  type OrganizationSettings as OrganizationSettingsValue,
} from '@/lib/organize';

function Card({ children }: { children: React.ReactNode }) {
  return <section className="rounded-xl border bg-card p-5">{children}</section>;
}

interface EditableListProps {
  id: string;
  label: string;
  hint: string;
  placeholder: string;
  items: string[];
  onChange: (items: string[]) => void;
}

function EditableList({ id, label, hint, placeholder, items, onChange }: EditableListProps) {
  const [draft, setDraft] = useState('');

  const add = () => {
    const value = draft.trim();
    if (!value) return;
    if (items.some((item) => item.toLocaleLowerCase('pt-BR') === value.toLocaleLowerCase('pt-BR'))) {
      toast.info(`“${value}” já está na lista`);
    } else {
      onChange([...items, value]);
    }
    setDraft('');
  };

  return (
    <div>
      <Label htmlFor={id} className="text-sm font-medium">{label}</Label>
      <p className="mt-0.5 text-[13px] text-muted-foreground">{hint}</p>
      <form
        className="mt-3 flex gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          add();
        }}
      >
        <Input id={id} value={draft} placeholder={placeholder} onChange={(event) => setDraft(event.target.value)} />
        <Button type="submit" variant="outline" disabled={!draft.trim()}>
          <Plus />
          Adicionar
        </Button>
      </form>
      {items.length === 0 ? (
        <p className="mt-3 text-[13px] text-muted-foreground">Nenhum item ainda.</p>
      ) : (
        <ul className="mt-3 flex flex-wrap gap-1.5">
          {items.map((item) => (
            <li key={item} className="inline-flex items-center gap-1 rounded-md bg-muted py-0.5 pl-2 pr-1 text-[13px]">
              {item}
              <button
                type="button"
                aria-label={`Remover ${item}`}
                className="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
                onClick={() => onChange(items.filter((existing) => existing !== item))}
              >
                <X className="h-3 w-3" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** Configurações → Organização: título automático, clientes, categorias e reuniões antigas. */
export function OrganizationSettings() {
  const { refetchMeetings } = useSidebar();
  const [settings, setSettings] = useState<OrganizationSettingsValue>(DEFAULT_ORGANIZATION_SETTINGS);
  const [state, setState] = useState<'loading' | 'ready' | 'unavailable'>('loading');
  const [templateDraft, setTemplateDraft] = useState(DEFAULT_ORGANIZATION_SETTINGS.titleTemplate);
  const [isRunning, setIsRunning] = useState(false);
  const [progress, setProgress] = useState<{ index: number; total: number } | null>(null);
  const templateInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let cancelled = false;
    getOrganizationSettings().then((loaded) => {
      if (cancelled) return;
      if (loaded) {
        setSettings(loaded);
        setTemplateDraft(loaded.titleTemplate);
        setState('ready');
      } else {
        setState('unavailable');
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const persist = useCallback(async (next: OrganizationSettingsValue) => {
    const previous = settings;
    setSettings(next);
    try {
      const saved = await saveOrganizationSettings(next);
      setSettings(saved);
      setTemplateDraft(saved.titleTemplate);
    } catch (error) {
      console.error('Failed to save organization settings:', error);
      setSettings(previous);
      setTemplateDraft(previous.titleTemplate);
      toast.error('Não foi possível salvar', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  }, [settings]);

  const preview = useMemo(() => {
    const now = new Date();
    return previewTitleTemplate(templateDraft, {
      cliente: settings.knownClients[0] ?? 'Acme',
      projeto: 'Portal',
      assunto: 'Alinhamento do cronograma',
      data: now.toLocaleDateString('pt-BR'),
      hora: now.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' }),
    });
  }, [templateDraft, settings.knownClients]);

  const insertPlaceholder = (token: string) => {
    const input = templateInputRef.current;
    const start = input?.selectionStart ?? templateDraft.length;
    const end = input?.selectionEnd ?? templateDraft.length;
    const next = `${templateDraft.slice(0, start)}${token}${templateDraft.slice(end)}`;
    setTemplateDraft(next);
    requestAnimationFrame(() => {
      input?.focus();
      input?.setSelectionRange(start + token.length, start + token.length);
    });
  };

  const commitTemplate = () => {
    const next = templateDraft.trim();
    if (!next) {
      setTemplateDraft(settings.titleTemplate);
      return;
    }
    if (next !== settings.titleTemplate) void persist({ ...settings, titleTemplate: next });
  };

  const runOrganizeAll = async () => {
    setIsRunning(true);
    setProgress(null);
    const unlisten = await listenOrganizeEvents({
      onProgress: ({ index, total }) => setProgress({ index, total }),
    });
    try {
      const result = await organizeAllMeetings(true);
      await refetchMeetings();
      if (result.processed === 0 && result.failed === 0) {
        toast.info('Nenhuma reunião para organizar');
      } else if (result.failed > 0) {
        toast.warning(`${result.processed} organizadas, ${result.failed} com falha`);
      } else {
        toast.success(result.processed === 1 ? '1 reunião organizada' : `${result.processed} reuniões organizadas`);
      }
    } catch (error) {
      console.error('Failed to organize meetings:', error);
      toast.error('Não foi possível organizar as reuniões', {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      unlisten();
      setIsRunning(false);
      setProgress(null);
    }
  };

  const cancelRun = async () => {
    try {
      await cancelOrganizeAll();
    } catch (error) {
      console.error('Failed to cancel organize:', error);
      toast.error('Não foi possível cancelar');
    }
  };

  if (state === 'loading') {
    return (
      <div className="space-y-4" aria-busy="true" aria-label="Carregando configurações de organização">
        <Skeleton className="h-20 w-full rounded-xl" />
        <Skeleton className="h-40 w-full rounded-xl" />
        <Skeleton className="h-28 w-full rounded-xl" />
      </div>
    );
  }

  if (state === 'unavailable') {
    return (
      <Card>
        <h3 className="text-sm font-medium">Organização indisponível</h3>
        <p className="mt-1 text-[13px] text-muted-foreground">
          Esta versão do app ainda não organiza as reuniões automaticamente. A lista de conversas continua funcionando por data.
        </p>
      </Card>
    );
  }

  const percent = progress && progress.total > 0 ? Math.round((progress.index / progress.total) * 100) : 0;

  return (
    <div className="space-y-4">
      <Card>
        <div className="flex items-center justify-between gap-6">
          <div>
            <Label htmlFor="organizar-automaticamente" className="text-sm font-medium">Organizar automaticamente</Label>
            <p className="mt-0.5 text-[13px] text-muted-foreground">
              Ao terminar uma gravação, o modelo local gera o título e sugere cliente, projeto, categoria e etiquetas.
            </p>
          </div>
          <Switch
            id="organizar-automaticamente"
            checked={settings.autoOrganize}
            onCheckedChange={(checked) => void persist({ ...settings, autoOrganize: checked })}
          />
        </div>
      </Card>

      <Card>
        <Label htmlFor="modelo-de-titulo" className="text-sm font-medium">Modelo de título</Label>
        <p className="mt-0.5 text-[13px] text-muted-foreground">
          Como o título automático é montado. Partes sem valor são omitidas.
        </p>
        <Input
          id="modelo-de-titulo"
          ref={templateInputRef}
          className="mt-3 font-mono text-[13px]"
          value={templateDraft}
          onChange={(event) => setTemplateDraft(event.target.value)}
          onBlur={commitTemplate}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              commitTemplate();
            }
          }}
        />
        <div className="mt-2 flex flex-wrap gap-1.5" aria-label="Marcadores disponíveis">
          {TITLE_PLACEHOLDERS.map((placeholder) => (
            <button
              key={placeholder.token}
              type="button"
              title={`${placeholder.description}. Clique para inserir.`}
              // onMouseDown mantém o foco (e o cursor) no campo
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => insertPlaceholder(placeholder.token)}
              className="rounded-md border px-1.5 py-0.5 font-mono text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            >
              {placeholder.token}
            </button>
          ))}
        </div>
        <div className="mt-3 rounded-lg bg-muted px-3 py-2" aria-live="polite">
          <p className="text-xs text-muted-foreground">Prévia</p>
          <p className="truncate text-sm">{preview || 'O modelo está vazio.'}</p>
        </div>
      </Card>

      <Card>
        <EditableList
          id="clientes-conhecidos"
          label="Clientes conhecidos"
          hint="Ajuda o modelo a reconhecer o cliente pela conversa e aparece como sugestão ao classificar."
          placeholder="Nome do cliente"
          items={settings.knownClients}
          onChange={(knownClients) => void persist({ ...settings, knownClients })}
        />
      </Card>

      <Card>
        <EditableList
          id="categorias"
          label="Categorias"
          hint="As opções do campo Categoria, por exemplo Comercial, Suporte ou Interna."
          placeholder="Nome da categoria"
          items={settings.categories}
          onChange={(categories) => void persist({ ...settings, categories })}
        />
      </Card>

      <Card>
        <div className="flex items-center justify-between gap-6">
          <div>
            <h3 className="text-sm font-medium">Organizar reuniões antigas</h3>
            <p className="mt-0.5 text-[13px] text-muted-foreground">
              Gera título e classificação para as reuniões que ainda não foram organizadas.
            </p>
          </div>
          {isRunning ? (
            <Button variant="outline" onClick={() => void cancelRun()}>Cancelar</Button>
          ) : (
            <Button variant="outline" onClick={() => void runOrganizeAll()}>
              <Sparkles />
              Organizar
            </Button>
          )}
        </div>
        {isRunning && (
          <div className="mt-4" role="status">
            <Progress value={percent} className="h-1.5" aria-label="Progresso da organização" />
            <p className="mt-2 flex items-center gap-2 text-[13px] text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
              {progress ? `Organizando ${Math.min(Math.max(progress.index, 1), progress.total)} de ${progress.total}…` : 'Preparando…'}
            </p>
          </div>
        )}
      </Card>
    </div>
  );
}
