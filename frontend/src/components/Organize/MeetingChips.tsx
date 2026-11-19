import React from 'react';
import { Building2, FolderKanban, Pin, Tag } from 'lucide-react';
import type { MeetingOrg } from '@/lib/organize';

function Chip({ icon, children, title }: { icon: React.ReactNode; children: React.ReactNode; title: string }) {
  return (
    <span
      title={title}
      className="inline-flex max-w-[11rem] items-center gap-1 rounded-md border bg-card px-1.5 py-0.5 text-xs text-muted-foreground [&_svg]:size-3 [&_svg]:shrink-0"
    >
      {icon}
      <span className="truncate">{children}</span>
    </span>
  );
}

/** Etiquetas de cliente, projeto e categoria da reunião (some o que não existir). */
export function MeetingChips({ meeting, onClick }: { meeting: MeetingOrg; onClick?: () => void }) {
  const chips = (
    <>
      {meeting.pinned && <Chip icon={<Pin />} title="Reunião fixada">Fixada</Chip>}
      {meeting.client && <Chip icon={<Building2 />} title={`Cliente: ${meeting.client}`}>{meeting.client}</Chip>}
      {meeting.project && <Chip icon={<FolderKanban />} title={`Projeto: ${meeting.project}`}>{meeting.project}</Chip>}
      {meeting.category && <Chip icon={<Tag />} title={`Categoria: ${meeting.category}`}>{meeting.category}</Chip>}
    </>
  );
  if (!meeting.pinned && !meeting.client && !meeting.project && !meeting.category) return null;
  if (!onClick) return <>{chips}</>;
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label="Editar classificação"
      className="hidden items-center gap-1.5 rounded-md sm:flex"
    >
      {chips}
    </button>
  );
}
