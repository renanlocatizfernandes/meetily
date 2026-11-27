'use client';

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { Building2, Loader2, Plus } from 'lucide-react';
import { toast } from 'sonner';
import { TopBar } from '@/components/Shell/TopBar';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Skeleton } from '@/components/ui/skeleton';
import { Textarea } from '@/components/ui/textarea';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { foldText } from '@/lib/agenda';
import { formatMeetingWhen } from '@/lib/organize';
import { errorText } from '@/lib/tauriSafe';
import {
  formatDuration,
  listClients,
  onWorkspaceEvents,
  removeClient,
  renameClient,
  saveClient,
  type ClientInfo,
} from '@/lib/workspace';

function splitList(text: string): string[] {
  return text
    .split(/[,\n;]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

interface EditorProps {
  client: ClientInfo | null;
  /** `true` para criar um cliente novo. */
  creating: boolean;
  others: string[];
  onClose: () => void;
  onSaved: (clients: ClientInfo[]) => void;
}

function ClientEditor({ client, creating, others, onClose, onSaved }: EditorProps) {
  const [name, setName] = useState('');
  const [aliases, setAliases] = useState('');
  const [domains, setDomains] = useState('');
  const [notes, setNotes] = useState('');
  const [busy, setBusy] = useState(false);
  const open = creating || client !== null;

  useEffect(() => {
    setName(client?.name ?? '');
    setAliases(client?.aliases.join(', ') ?? '');
    setDomains(client?.domains.join(', ') ?? '');
    setNotes(client?.notes ?? '');
  }, [client, creating]);

  const trimmed = name.trim();
  const renamed = client !== null && trimmed !== '' && trimmed !== client.name;
  const merging = renamed && others.some((other) => foldText(other) === foldText(trimmed));

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!trimmed || busy) return;
    setBusy(true);
    try {
      if (renamed && client) await renameClient(client.name, trimmed);
      onSaved(await saveClient({ name: trimmed, aliases: splitList(aliases), domains: splitList(domains), notes }));
      toast.success(merging ? `Clientes unidos em “${trimmed}”` : 'Cliente salvo');
      onClose();
    } catch (error) {
      toast.error('Não foi possível salvar o cliente', { description: errorText(error) });
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!client || busy) return;
    setBusy(true);
    try {
      onSaved(await removeClient(client.name, false));
      toast.success('Cliente removido da lista', { description: 'As reuniões continuam com o nome, sem as regras.' });
      onClose();
    } catch (error) {
      toast.error('Não foi possível remover', { description: errorText(error) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{client ? 'Editar cliente' : 'Novo cliente'}</DialogTitle>
            <DialogDescription>
              Apelidos e domínios de e-mail ajudam o app a reconhecer o cliente sozinho nas próximas reuniões.
            </DialogDescription>
          </DialogHeader>
          <div>
            <Label htmlFor="cliente-nome">Nome</Label>
            <Input id="cliente-nome" className="mt-1.5" value={name} onChange={(event) => setName(event.target.value)} autoFocus />
            {merging && (
              <p className="mt-1.5 text-[13px] text-amber-700 dark:text-amber-400">
                Já existe “{trimmed}”. Ao salvar, as reuniões e regras dos dois serão unidas.
              </p>
            )}
          </div>
          <div>
            <Label htmlFor="cliente-apelidos">Apelidos</Label>
            <Input
              id="cliente-apelidos"
              className="mt-1.5"
              placeholder="Outros nomes usados nas reuniões, separados por vírgula"
              value={aliases}
              onChange={(event) => setAliases(event.target.value)}
            />
          </div>
          <div>
            <Label htmlFor="cliente-dominios">Domínios de e-mail</Label>
            <Input
              id="cliente-dominios"
              className="mt-1.5"
              placeholder="acme.com.br, acme.com"
              value={domains}
              onChange={(event) => setDomains(event.target.value)}
            />
            <p className="mt-1 text-[13px] text-muted-foreground">
              Reunião com um participante desse domínio é marcada com este cliente.
            </p>
          </div>
          <div>
            <Label htmlFor="cliente-notas">Anotações</Label>
            <Textarea id="cliente-notas" className="mt-1.5" rows={3} value={notes} onChange={(event) => setNotes(event.target.value)} />
          </div>
          <DialogFooter className="gap-2 sm:justify-between">
            {client ? (
              <Button type="button" variant="ghost" className="text-destructive hover:text-destructive" onClick={remove} disabled={busy}>
                Remover da lista
              </Button>
            ) : <span />}
            <div className="flex gap-2">
              <Button type="button" variant="outline" onClick={onClose}>Cancelar</Button>
              <Button type="submit" disabled={!trimmed || busy}>
                {busy && <Loader2 className="animate-spin" />}
                {merging ? 'Unir' : 'Salvar'}
              </Button>
            </div>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function ClientsView() {
  const [clients, setClients] = useState<ClientInfo[] | null>(null);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState<ClientInfo | null>(null);
  const [creating, setCreating] = useState(false);

  const reload = useCallback(() => {
    listClients().then(setClients);
  }, []);

  useEffect(() => {
    reload();
    return onWorkspaceEvents(['clients-updated', 'meeting-organized', 'tasks-updated'], reload);
  }, [reload]);

  const filtered = useMemo(() => {
    const terms = foldText(query).split(/\s+/).filter(Boolean);
    return (clients ?? []).filter((client) => {
      const haystack = foldText([client.name, ...client.aliases, ...client.domains, ...client.people].join(' '));
      return terms.every((term) => haystack.includes(term));
    });
  }, [clients, query]);

  return (
    <div className="flex h-full min-w-0 flex-col bg-background">
      <TopBar
        title={<span className="block truncate px-2">Clientes</span>}
        actions={
          <Button size="sm" variant="outline" onClick={() => setCreating(true)}>
            <Plus /> Novo cliente
          </Button>
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto custom-scrollbar">
        <div className="mx-auto w-full max-w-[920px] px-6 py-5">
          <Input
            placeholder="Buscar por nome, apelido, domínio ou pessoa"
            aria-label="Buscar clientes"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />

          {clients === null ? (
            <div className="mt-4 grid gap-3 sm:grid-cols-2" aria-busy="true" aria-label="Carregando clientes">
              <Skeleton className="h-32 rounded-xl" />
              <Skeleton className="h-32 rounded-xl" />
            </div>
          ) : filtered.length === 0 ? (
            <div className="flex flex-col items-center gap-2 py-14 text-center">
              <Building2 className="h-8 w-8 text-muted-foreground" />
              <p className="text-sm font-medium">{query ? 'Nenhum cliente com esse filtro.' : 'Nenhum cliente ainda'}</p>
              {!query && (
                <p className="max-w-sm text-[13px] text-muted-foreground">
                  Os clientes aparecem conforme as reuniões são organizadas. Você também pode cadastrar um, com apelidos e domínio de e-mail.
                </p>
              )}
            </div>
          ) : (
            <ul className="mt-4 grid gap-3 sm:grid-cols-2">
              {filtered.map((client) => {
                const stats = [
                  client.meetings === 1 ? '1 reunião' : `${client.meetings} reuniões`,
                  formatDuration(client.totalSeconds),
                  client.lastMeeting ? `última ${formatMeetingWhen(client.lastMeeting)}` : '',
                ].filter(Boolean);
                return (
                  <li key={client.name}>
                    <button
                      type="button"
                      onClick={() => setEditing(client)}
                      className="flex h-full w-full flex-col rounded-xl border bg-card p-4 text-left transition-colors hover:bg-accent/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      <div className="flex items-start justify-between gap-2">
                        <span className="min-w-0 truncate text-sm font-medium">{client.name}</span>
                        {client.openTasks > 0 && (
                          <span className="shrink-0 rounded-md bg-primary/10 px-1.5 py-0.5 text-xs text-primary">
                            {client.openTasks === 1 ? '1 tarefa aberta' : `${client.openTasks} tarefas abertas`}
                          </span>
                        )}
                      </div>
                      <p className="mt-0.5 text-xs text-muted-foreground">{stats.join(' · ')}</p>
                      {(client.aliases.length > 0 || client.domains.length > 0) && (
                        <div className="mt-2 flex flex-wrap gap-1">
                          {client.aliases.map((alias) => (
                            <span key={`a-${alias}`} className="rounded-md bg-muted px-1.5 py-0.5 text-xs">{alias}</span>
                          ))}
                          {client.domains.map((domain) => (
                            <span key={`d-${domain}`} className="rounded-md bg-muted px-1.5 py-0.5 font-mono text-[11px]">@{domain}</span>
                          ))}
                        </div>
                      )}
                      {client.people.length > 0 && (
                        <p className="mt-2 line-clamp-2 text-xs text-muted-foreground">
                          {client.people.slice(0, 8).join(', ')}{client.people.length > 8 ? ` e mais ${client.people.length - 8}` : ''}
                        </p>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      <ClientEditor
        client={editing}
        creating={creating}
        others={(clients ?? []).filter((client) => client.name !== editing?.name).map((client) => client.name)}
        onClose={() => {
          setEditing(null);
          setCreating(false);
        }}
        onSaved={setClients}
      />
    </div>
  );
}
