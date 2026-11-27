'use client';

import React, { useEffect, useState } from 'react';
import { BookOpen, FileDown, FileText } from 'lucide-react';
import { toast } from 'sonner';
import { useRouter } from 'next/navigation';
import { errorText } from '@/lib/tauriSafe';
import {
  exportMeetingFile,
  exportToObsidian,
  getWorkspaceSettings,
  obsidianNoteFor,
  openObsidianNote,
  type ExportFormat,
} from '@/lib/workspace';

interface ItemProps {
  onSelect?: (event: Event) => void;
  children?: React.ReactNode;
}

/** Itens "Exportar" e "Obsidian" do menu da reunião. */
export function ExportMenuItems({ meetingId, Item }: { meetingId: string; Item: React.ComponentType<ItemProps> }) {
  const router = useRouter();
  const [hasVault, setHasVault] = useState(false);
  const [hasNote, setHasNote] = useState(false);

  useEffect(() => {
    getWorkspaceSettings().then((settings) => setHasVault(settings.obsidianVault !== ''));
    obsidianNoteFor(meetingId).then((note) => setHasNote(!!note?.exists));
  }, [meetingId]);

  const saveFile = async (format: ExportFormat) => {
    try {
      const path = await exportMeetingFile(meetingId, format);
      if (path) toast.success('Arquivo salvo', { description: path });
    } catch (error) {
      toast.error('Não foi possível exportar', { description: errorText(error) });
    }
  };

  const sendToObsidian = async () => {
    if (!hasVault) {
      toast.info('Conecte um cofre do Obsidian primeiro');
      router.push('/settings?tab=integrations');
      return;
    }
    try {
      const note = await exportToObsidian(meetingId, false);
      setHasNote(true);
      toast.success(hasNote ? 'Nota atualizada no Obsidian' : 'Nota criada no Obsidian', {
        description: note.path,
        action: { label: 'Abrir', onClick: () => void openObsidianNote(meetingId).catch(() => undefined) },
      });
    } catch (error) {
      toast.error('Não foi possível enviar ao Obsidian', { description: errorText(error) });
    }
  };

  return (
    <>
      <Item onSelect={() => void saveFile('txt')}>
        <FileText /> Exportar em texto (.txt)
      </Item>
      <Item onSelect={() => void saveFile('md')}>
        <FileDown /> Exportar em Markdown (.md)
      </Item>
      <Item onSelect={() => void sendToObsidian()}>
        <BookOpen /> {hasNote ? 'Atualizar no Obsidian' : 'Enviar ao Obsidian'}
      </Item>
      {hasNote && (
        <Item
          onSelect={() =>
            void openObsidianNote(meetingId).catch((error) =>
              toast.error('Não foi possível abrir o Obsidian', { description: errorText(error) }),
            )
          }
        >
          <BookOpen /> Abrir no Obsidian
        </Item>
      )}
    </>
  );
}
