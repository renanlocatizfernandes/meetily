'use client';

import React, { useEffect, useId, useMemo, useRef, useState } from 'react';
import { Loader2, Sparkles, X } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import {
  EMPTY_FACETS,
  getOrganizationSettings,
  listMeetingFacets,
  organizeMeeting,
  updateMeetingClassification,
  type MeetingFacets,
  type MeetingOrg,
} from '@/lib/organize';

const NO_CATEGORY = '__none__';

interface ComboInputProps {
  id: string;
  value: string;
  onChange: (value: string) => void;
  options: string[];
  placeholder?: string;
}

/** Campo de texto livre com sugestões (combobox simples, navegável por teclado). */
function ComboInput({ id, value, onChange, options, placeholder }: ComboInputProps) {
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(-1);
  const listId = `${id}-opcoes`;
  const filtered = useMemo(() => {
    const query = value.trim().toLocaleLowerCase('pt-BR');
    const unique = Array.from(new Set(options));
    const matches = query ? unique.filter((option) => option.toLocaleLowerCase('pt-BR').includes(query)) : unique;
    return matches.filter((option) => option !== value).slice(0, 8);
  }, [options, value]);
  const showList = open && filtered.length > 0;

  return (
    <div className="relative">
      <Input
        id={id}
        role="combobox"
        aria-expanded={showList}
        aria-controls={listId}
        aria-autocomplete="list"
        autoComplete="off"
        value={value}
        placeholder={placeholder}
        onChange={(event) => {
          onChange(event.target.value);
          setOpen(true);
          setHighlight(-1);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onKeyDown={(event) => {
          if (!showList) return;
          if (event.key === 'ArrowDown') {
            event.preventDefault();
            setHighlight((current) => (current + 1) % filtered.length);
          } else if (event.key === 'ArrowUp') {
            event.preventDefault();
            setHighlight((current) => (current <= 0 ? filtered.length - 1 : current - 1));
          } else if (event.key === 'Enter' && highlight >= 0) {
            event.preventDefault();
            onChange(filtered[highlight]);
            setOpen(false);
          } else if (event.key === 'Escape') {
            event.stopPropagation();
            setOpen(false);
          }
        }}
      />
      {showList && (
        <ul
          id={listId}
          role="listbox"
          className="absolute left-0 right-0 top-full z-50 mt-1 max-h-48 overflow-y-auto rounded-lg border bg-popover p-1 text-sm shadow-md custom-scrollbar"
        >
          {filtered.map((option, index) => (
            <li
              key={option}
              role="option"
              aria-selected={index === highlight}
              className={`cursor-default truncate rounded-md px-2 py-1.5 ${index === highlight ? 'bg-accent text-accent-foreground' : 'hover:bg-accent'}`}
              // onMouseDown evita que o blur do campo feche a lista antes do clique
              onMouseDown={(event) => {
                event.preventDefault();
                onChange(option);
                setOpen(false);
              }}
            >
              {option}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

interface TagInputProps {
  id: string;
  tags: string[];
  onChange: (tags: string[]) => void;
  suggestions: string[];
}

function TagInput({ id, tags, onChange, suggestions }: TagInputProps) {
  const [draft, setDraft] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  const addTag = (raw: string) => {
    const tag = raw.trim().replace(/,+$/, '').trim();
    if (!tag) return;
    if (!tags.some((existing) => existing.toLocaleLowerCase('pt-BR') === tag.toLocaleLowerCase('pt-BR'))) {
      onChange([...tags, tag]);
    }
    setDraft('');
  };

  const available = suggestions
    .filter((suggestion) => !tags.some((tag) => tag.toLocaleLowerCase('pt-BR') === suggestion.toLocaleLowerCase('pt-BR')))
    .slice(0, 6);

  return (
    <div className="space-y-2">
      <div
        className="flex min-h-9 flex-wrap items-center gap-1.5 rounded-lg border border-input px-2 py-1.5 focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/60"
        onClick={() => inputRef.current?.focus()}
      >
        {tags.map((tag) => (
          <span key={tag} className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-[13px]">
            {tag}
            <button
              type="button"
              aria-label={`Remover etiqueta ${tag}`}
              className="rounded-sm text-muted-foreground hover:text-foreground"
              onClick={(event) => {
                event.stopPropagation();
                onChange(tags.filter((existing) => existing !== tag));
              }}
            >
              <X className="h-3 w-3" />
            </button>
          </span>
        ))}
        <input
          ref={inputRef}
          id={id}
          value={draft}
          autoComplete="off"
          placeholder={tags.length === 0 ? 'Digite e pressione Enter' : ''}
          className="min-w-[8rem] flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground focus-visible:outline-none"
          onChange={(event) => {
            const next = event.target.value;
            if (next.endsWith(',')) addTag(next);
            else setDraft(next);
          }}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              addTag(draft);
            } else if (event.key === 'Backspace' && draft === '' && tags.length > 0) {
              onChange(tags.slice(0, -1));
            }
          }}
          onBlur={() => addTag(draft)}
        />
      </div>
      {available.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {available.map((suggestion) => (
            <button
              key={suggestion}
              type="button"
              className="rounded-md border border-dashed px-2 py-0.5 text-xs text-muted-foreground transition-colors hover:border-solid hover:bg-accent hover:text-foreground"
              onClick={() => addTag(suggestion)}
            >
              + {suggestion}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

interface ClassifyDialogProps {
  meeting: MeetingOrg | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: (meeting: MeetingOrg) => void;
}

export function ClassifyDialog({ meeting, open, onOpenChange, onSaved }: ClassifyDialogProps) {
  const formId = useId();
  const [client, setClient] = useState('');
  const [project, setProject] = useState('');
  const [category, setCategory] = useState('');
  const [tags, setTags] = useState<string[]>([]);
  const [facets, setFacets] = useState<MeetingFacets>(EMPTY_FACETS);
  const [knownClients, setKnownClients] = useState<string[]>([]);
  const [categories, setCategories] = useState<string[]>([]);
  const [isSuggesting, setIsSuggesting] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const meetingId = meeting?.id ?? null;

  // Carrega os valores da reunião e as opções sempre que o diálogo abre
  useEffect(() => {
    if (!open || !meeting) return;
    setClient(meeting.client ?? '');
    setProject(meeting.project ?? '');
    setCategory(meeting.category ?? '');
    setTags(meeting.tags);

    let cancelled = false;
    Promise.all([listMeetingFacets(), getOrganizationSettings()]).then(([loadedFacets, settings]) => {
      if (cancelled) return;
      setFacets(loadedFacets);
      setKnownClients(settings?.knownClients ?? []);
      setCategories(settings?.categories ?? []);
    });
    return () => {
      cancelled = true;
    };
    // Recarrega só ao abrir ou trocar de reunião, não a cada atualização da linha.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, meetingId]);

  const clientOptions = useMemo(
    () => [...knownClients, ...facets.clients.map((facet) => facet.name)],
    [knownClients, facets.clients],
  );
  const projectOptions = useMemo(() => facets.projects.map((facet) => facet.name), [facets.projects]);
  const categoryOptions = useMemo(() => {
    const all = [...categories, ...facets.categories.map((facet) => facet.name)];
    if (category) all.push(category);
    return Array.from(new Set(all));
  }, [categories, facets.categories, category]);

  const handleSuggest = async () => {
    if (!meeting) return;
    setIsSuggesting(true);
    try {
      const suggestion = await organizeMeeting(meeting.id, true);
      let result = suggestion;
      // "Sugerir" não deve trocar um título que a pessoa escreveu à mão.
      if (meeting.titleSource === 'manual' && meeting.title && suggestion.title !== meeting.title) {
        try {
          await invoke('api_save_meeting_title', { meetingId: meeting.id, title: meeting.title });
          result = { ...suggestion, title: meeting.title, titleSource: 'manual' };
        } catch (error) {
          console.error('Failed to restore manual title:', error);
        }
      }
      setClient(result.client ?? '');
      setProject(result.project ?? '');
      setCategory(result.category ?? '');
      setTags(result.tags);
      onSaved(result);
      toast.success('Sugestão aplicada', { description: 'Revise os campos e salve se quiser ajustar.' });
    } catch (error) {
      console.error('Failed to suggest classification:', error);
      toast.error('Não foi possível sugerir a classificação', {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsSuggesting(false);
    }
  };

  const handleSave = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!meeting) return;
    setIsSaving(true);
    try {
      const saved = await updateMeetingClassification(meeting.id, {
        client: client.trim(),
        project: project.trim(),
        category: category.trim(),
        tags,
        pinned: null,
      });
      onSaved(saved);
      toast.success('Classificação salva');
      onOpenChange(false);
    } catch (error) {
      console.error('Failed to save classification:', error);
      toast.error('Não foi possível salvar a classificação', {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsSaving(false);
    }
  };

  const busy = isSuggesting || isSaving;

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!busy) onOpenChange(next); }}>
      <DialogContent className="grid-cols-[minmax(0,1fr)] sm:max-w-[480px]">
        <DialogHeader className="min-w-0">
          <DialogTitle>Classificar reunião</DialogTitle>
          <DialogDescription className="truncate">{meeting?.title}</DialogDescription>
        </DialogHeader>
        <form id={formId} className="min-w-0 space-y-4" onSubmit={handleSave}>
          <div className="space-y-1.5">
            <Label htmlFor={`${formId}-cliente`}>Cliente</Label>
            <ComboInput
              id={`${formId}-cliente`}
              value={client}
              onChange={setClient}
              options={clientOptions}
              placeholder="Escolha ou digite um cliente"
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor={`${formId}-projeto`}>Projeto</Label>
            <ComboInput
              id={`${formId}-projeto`}
              value={project}
              onChange={setProject}
              options={projectOptions}
              placeholder="Opcional"
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor={`${formId}-categoria`}>Categoria</Label>
            <Select
              value={category || NO_CATEGORY}
              onValueChange={(value) => setCategory(value === NO_CATEGORY ? '' : value)}
            >
              <SelectTrigger id={`${formId}-categoria`}>
                <SelectValue placeholder="Sem categoria" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_CATEGORY}>Sem categoria</SelectItem>
                {categoryOptions.map((option) => (
                  <SelectItem key={option} value={option}>{option}</SelectItem>
                ))}
              </SelectContent>
            </Select>
            {categoryOptions.length === 0 && (
              <p className="text-xs text-muted-foreground">Cadastre categorias em Configurações → Organização.</p>
            )}
          </div>
          <div className="space-y-1.5">
            <Label htmlFor={`${formId}-etiquetas`}>Etiquetas</Label>
            <TagInput
              id={`${formId}-etiquetas`}
              tags={tags}
              onChange={setTags}
              suggestions={facets.tags.map((facet) => facet.name)}
            />
          </div>
        </form>
        <DialogFooter className="gap-2 sm:justify-between sm:space-x-0">
          <Button type="button" variant="outline" onClick={handleSuggest} disabled={busy}>
            {isSuggesting ? <Loader2 className="animate-spin" /> : <Sparkles />}
            {isSuggesting ? 'Sugerindo…' : 'Sugerir automaticamente'}
          </Button>
          <Button type="submit" form={formId} disabled={busy}>
            {isSaving && <Loader2 className="animate-spin" />}
            Salvar
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
