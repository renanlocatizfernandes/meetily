'use client';

import { BookOpen, ClipboardList, Globe, HelpCircle, History, MessageSquare, User } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { LiveAssistOrigin, originLabel, originNeedsCare, originTitle } from '@/lib/liveAssistContext';

const ICONS: Record<string, LucideIcon> = {
  reuniao: History,
  perfil: User,
  glossario: BookOpen,
  internet: Globe,
  pauta: ClipboardList,
  conversa: MessageSquare,
  modelo: HelpCircle,
};

/** Discreet tag that says where a suggestion came from. */
export function OriginTag({ origin }: { origin: LiveAssistOrigin }) {
  const Icon = ICONS[origin.kind] ?? HelpCircle;
  const care = originNeedsCare(origin);
  return (
    <span
      className={`mb-1.5 inline-flex max-w-full items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium leading-4 text-muted-foreground ${
        care ? 'border-dashed border-border bg-transparent' : 'border-border bg-muted'
      }`}
      title={originTitle(origin)}
    >
      <Icon className="h-3 w-3 shrink-0" aria-hidden="true" />
      <span className="sr-only">Origem: </span>
      <span className="truncate">{originLabel(origin)}</span>
    </span>
  );
}
