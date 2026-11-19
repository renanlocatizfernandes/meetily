'use client';

import React, { useCallback, useState } from 'react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { Pencil, Pin, PinOff, Sparkles, Tags, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';
import { useSidebar, type CurrentMeeting } from '@/components/Sidebar/SidebarProvider';
import { ConfirmationModal } from '@/components/ConfirmationModel/confirmation-modal';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { organizeMeeting, updateMeetingClassification, type MeetingOrg } from '@/lib/organize';
import { ClassifyDialog } from './ClassifyDialog';

export interface MeetingActions {
  /** Abre o diálogo de renomear. */
  rename: (meeting: MeetingOrg) => void;
  classify: (meeting: MeetingOrg) => void;
  autoTitle: (meeting: MeetingOrg) => Promise<void>;
  togglePin: (meeting: MeetingOrg) => Promise<void>;
  remove: (meeting: MeetingOrg) => void;
  /** Renomeia direto (edição no lugar). Devolve `false` se não salvou. */
  saveTitle: (meetingId: string, title: string, source?: string) => Promise<boolean>;
  /** Reuniões com "Gerar título automático" em andamento. */
  busyIds: ReadonlySet<string>;
}

/**
 * Ações de uma reunião (renomear, classificar, título automático, fixar, excluir), compartilhadas
 * pela barra lateral e pela barra superior. Renderize `dialogs` uma vez em quem usa o hook.
 */
export function useMeetingActions(): { actions: MeetingActions; dialogs: React.ReactNode } {
  const router = useRouter();
  const {
    meetings,
    setMeetings,
    currentMeeting,
    setCurrentMeeting,
    organizedMeetings,
    organizeAvailable,
    upsertOrganizedMeeting,
  } = useSidebar();

  const [renameTarget, setRenameTarget] = useState<MeetingOrg | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [classifyTargetId, setClassifyTargetId] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<MeetingOrg | null>(null);
  const [busyIds, setBusyIds] = useState<ReadonlySet<string>>(new Set());

  const saveTitle = useCallback(async (meetingId: string, title: string, source = 'sidebar') => {
    const newTitle = title.trim();
    if (!newTitle) {
      toast.error('O título não pode ficar vazio');
      return false;
    }
    try {
      await invoke('api_save_meeting_title', { meetingId, title: newTitle });
      setMeetings(meetings.map((m: CurrentMeeting) => (m.id === meetingId ? { ...m, title: newTitle } : m)));
      if (currentMeeting?.id === meetingId) {
        setCurrentMeeting({ id: meetingId, title: newTitle });
      }
      const existing = organizedMeetings.find((m) => m.id === meetingId);
      if (existing && organizeAvailable) {
        upsertOrganizedMeeting({ ...existing, title: newTitle, titleSource: 'manual' });
      }
      Analytics.trackButtonClick('edit_meeting_title', source);
      toast.success('Título atualizado');
      return true;
    } catch (error) {
      console.error('Failed to update meeting title:', error);
      toast.error('Não foi possível atualizar o título', {
        description: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
  }, [meetings, setMeetings, currentMeeting, setCurrentMeeting, organizedMeetings, organizeAvailable, upsertOrganizedMeeting]);

  const rename = useCallback((meeting: MeetingOrg) => {
    setRenameTarget(meeting);
    setRenameValue(meeting.title);
  }, []);

  const confirmRename = async () => {
    if (!renameTarget) return;
    if (await saveTitle(renameTarget.id, renameValue)) {
      setRenameTarget(null);
      setRenameValue('');
    }
  };

  const autoTitle = useCallback(async (meeting: MeetingOrg) => {
    setBusyIds((current) => new Set(current).add(meeting.id));
    const toastId = toast.loading('Gerando título…', { description: meeting.title });
    try {
      const organized = await organizeMeeting(meeting.id, true);
      upsertOrganizedMeeting(organized);
      toast.success('Título gerado', { id: toastId, description: organized.title });
    } catch (error) {
      console.error('Failed to organize meeting:', error);
      toast.error('Não foi possível gerar o título', {
        id: toastId,
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setBusyIds((current) => {
        const next = new Set(current);
        next.delete(meeting.id);
        return next;
      });
    }
  }, [upsertOrganizedMeeting]);

  const togglePin = useCallback(async (meeting: MeetingOrg) => {
    try {
      const updated = await updateMeetingClassification(meeting.id, { pinned: !meeting.pinned });
      upsertOrganizedMeeting(updated);
    } catch (error) {
      console.error('Failed to pin meeting:', error);
      toast.error(meeting.pinned ? 'Não foi possível desafixar' : 'Não foi possível fixar', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  }, [upsertOrganizedMeeting]);

  const handleDelete = async (itemId: string) => {
    try {
      await invoke('api_delete_meeting', { meetingId: itemId });
      setMeetings(meetings.filter((m: CurrentMeeting) => m.id !== itemId));
      Analytics.trackMeetingDeleted(itemId);
      toast.success('Reunião excluída', { description: 'Todos os dados associados foram removidos.' });
      if (currentMeeting?.id === itemId) {
        setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
        router.push('/');
      }
    } catch (error) {
      console.error('Failed to delete meeting:', error);
      toast.error('Não foi possível excluir a reunião', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const classifyTarget = classifyTargetId
    ? organizedMeetings.find((meeting) => meeting.id === classifyTargetId) ?? null
    : null;

  const actions: MeetingActions = {
    rename,
    classify: (meeting) => setClassifyTargetId(meeting.id),
    autoTitle,
    togglePin,
    remove: setDeleteTarget,
    saveTitle,
    busyIds,
  };

  const dialogs = (
    <>
      <ConfirmationModal
        isOpen={deleteTarget !== null}
        text="Tem certeza de que quer excluir esta reunião? Esta ação não pode ser desfeita."
        onConfirm={() => {
          if (deleteTarget) void handleDelete(deleteTarget.id);
          setDeleteTarget(null);
        }}
        onCancel={() => setDeleteTarget(null)}
      />

      <Dialog open={renameTarget !== null} onOpenChange={(open) => { if (!open) setRenameTarget(null); }}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>Renomear reunião</DialogTitle>
          </DialogHeader>
          <form
            className="space-y-4"
            onSubmit={(event) => {
              event.preventDefault();
              void confirmRename();
            }}
          >
            <div className="space-y-1.5">
              <Label htmlFor="meeting-title">Título</Label>
              <Input
                id="meeting-title"
                value={renameValue}
                onChange={(event) => setRenameValue(event.target.value)}
                placeholder="Título da reunião"
                autoFocus
              />
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setRenameTarget(null)}>Cancelar</Button>
              <Button type="submit">Salvar</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <ClassifyDialog
        meeting={classifyTarget}
        open={classifyTarget !== null}
        onOpenChange={(open) => { if (!open) setClassifyTargetId(null); }}
        onSaved={upsertOrganizedMeeting}
      />
    </>
  );

  return { actions, dialogs };
}

type MenuItemComponent = React.ComponentType<{
  onSelect?: (event: Event) => void;
  disabled?: boolean;
  className?: string;
  children?: React.ReactNode;
}>;

interface MeetingMenuItemsProps {
  meeting: MeetingOrg;
  actions: MeetingActions;
  /** Classificar, título automático e fixar dependem do backend de organização. */
  organizeAvailable: boolean;
  Item: MenuItemComponent;
  Separator: React.ComponentType<{ className?: string }>;
  /** Na barra superior, renomear acontece no lugar. */
  onRename?: () => void;
}

/** Os mesmos itens para o menu de contexto (barra lateral) e o menu "Mais ações" (barra superior). */
export function MeetingMenuItems({ meeting, actions, organizeAvailable, Item, Separator, onRename }: MeetingMenuItemsProps) {
  const busy = actions.busyIds.has(meeting.id);
  return (
    <>
      <Item onSelect={() => (onRename ? onRename() : actions.rename(meeting))}>
        <Pencil />
        Renomear
      </Item>
      {organizeAvailable && (
        <>
          <Item onSelect={() => actions.classify(meeting)}>
            <Tags />
            Classificar…
          </Item>
          <Item disabled={busy} onSelect={() => void actions.autoTitle(meeting)}>
            <Sparkles />
            {busy ? 'Gerando título…' : 'Gerar título automático'}
          </Item>
          <Item onSelect={() => void actions.togglePin(meeting)}>
            {meeting.pinned ? <PinOff /> : <Pin />}
            {meeting.pinned ? 'Desafixar' : 'Fixar'}
          </Item>
        </>
      )}
      <Separator />
      <Item className="text-danger focus:bg-danger-soft focus:text-danger" onSelect={() => actions.remove(meeting)}>
        <Trash2 />
        Excluir
      </Item>
    </>
  );
}
