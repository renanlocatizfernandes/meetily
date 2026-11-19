'use client';

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { ChevronRight, Loader2, MoreHorizontal, Pin, Sparkles } from 'lucide-react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Skeleton } from '@/components/ui/skeleton';
import { MeetingMenuItems, type MeetingActions } from '@/components/Organize/MeetingActions';
import { groupMeetings, meetingSubtitle, type GroupBy, type MeetingOrg } from '@/lib/organize';

const COLLAPSED_GROUPS_KEY = 'meetily.sidebar.collapsedGroups';

function readCollapsedGroups(): Set<string> {
  try {
    const raw = localStorage.getItem(COLLAPSED_GROUPS_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    return new Set(Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string') : []);
  } catch {
    return new Set();
  }
}

interface ConversationListProps {
  meetings: MeetingOrg[];
  groupBy: GroupBy;
  activeId: string | null;
  isLoading: boolean;
  organizeAvailable: boolean;
  actions: MeetingActions;
  /** Trecho da transcrição que casou com a busca, por reunião. */
  snippets: Map<string, string>;
  /** Com busca ativa os grupos ficam sempre abertos. */
  forceExpanded: boolean;
  onOpen: (meeting: MeetingOrg) => void;
  emptyState: React.ReactNode;
}

export function ConversationList({
  meetings,
  groupBy,
  activeId,
  isLoading,
  organizeAvailable,
  actions,
  snippets,
  forceExpanded,
  onOpen,
  emptyState,
}: ConversationListProps) {
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());

  useEffect(() => {
    setCollapsedGroups(readCollapsedGroups());
  }, []);

  const toggleGroup = useCallback((id: string) => {
    setCollapsedGroups((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      try {
        localStorage.setItem(COLLAPSED_GROUPS_KEY, JSON.stringify([...next]));
      } catch {
        // Preferência opcional.
      }
      return next;
    });
  }, []);

  const groups = useMemo(() => groupMeetings(meetings, groupBy), [meetings, groupBy]);

  if (isLoading) {
    return (
      <div className="space-y-3 px-3 py-2" aria-busy="true" aria-label="Carregando reuniões">
        {[72, 56, 64, 48, 60].map((width, index) => (
          <div key={index} className="space-y-1.5">
            <Skeleton className="h-3.5 bg-sidebar-accent" style={{ width: `${width}%` }} />
            <Skeleton className="h-2.5 w-2/5 bg-sidebar-accent" />
          </div>
        ))}
      </div>
    );
  }

  if (meetings.length === 0) {
    return <>{emptyState}</>;
  }

  return (
    <nav aria-label="Conversas" className="px-2 pb-3">
      {groups.map((group) => {
        const groupId = `${groupBy}:${group.key}`;
        const expanded = forceExpanded || !collapsedGroups.has(groupId);
        return (
          <section key={groupId} className="mt-1">
            <button
              type="button"
              aria-expanded={expanded}
              onClick={() => toggleGroup(groupId)}
              className="group/header flex h-7 w-full items-center gap-1 rounded-md px-1.5 text-xs font-medium text-muted-foreground transition-colors hover:text-foreground"
            >
              <ChevronRight
                className={`h-3 w-3 shrink-0 transition-transform duration-150 ${expanded ? 'rotate-90' : ''}`}
                aria-hidden="true"
              />
              <span className="truncate">{group.label}</span>
              <span className="ml-auto tabular-nums text-muted-foreground/80" aria-label={`${group.meetings.length} reuniões`}>
                {group.meetings.length}
              </span>
            </button>
            {expanded && (
              <ul>
                {group.meetings.map((meeting) => (
                  <ConversationRow
                    key={meeting.id}
                    meeting={meeting}
                    active={meeting.id === activeId}
                    organizeAvailable={organizeAvailable}
                    actions={actions}
                    snippet={snippets.get(meeting.id) ?? null}
                    onOpen={onOpen}
                  />
                ))}
              </ul>
            )}
          </section>
        );
      })}
    </nav>
  );
}

interface ConversationRowProps {
  meeting: MeetingOrg;
  active: boolean;
  organizeAvailable: boolean;
  actions: MeetingActions;
  snippet: string | null;
  onOpen: (meeting: MeetingOrg) => void;
}

const ConversationRow = React.memo(function ConversationRow({
  meeting,
  active,
  organizeAvailable,
  actions,
  snippet,
  onOpen,
}: ConversationRowProps) {
  const subtitle = meetingSubtitle(meeting);
  const busy = actions.busyIds.has(meeting.id);

  return (
    <li>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            className={`group relative flex items-start rounded-lg transition-colors duration-150 ${
              active ? 'bg-sidebar-accent text-foreground' : 'text-sidebar-foreground hover:bg-sidebar-accent/70'
            }`}
          >
            <button
              type="button"
              aria-current={active ? 'page' : undefined}
              onClick={() => onOpen(meeting)}
              className="min-w-0 flex-1 rounded-lg px-2 py-1.5 text-left"
            >
              <span className="flex items-center gap-1.5">
                <span className={`truncate text-[13px] leading-5 ${active ? 'font-medium' : ''}`}>
                  {meeting.title || 'Reunião sem título'}
                </span>
                {busy ? (
                  <Loader2 className="h-3 w-3 shrink-0 animate-spin text-muted-foreground" aria-label="Gerando título" />
                ) : meeting.titleSource === 'auto' ? (
                  <span title="Título gerado automaticamente" className="shrink-0 text-muted-foreground/80">
                    <Sparkles className="h-3 w-3" aria-hidden="true" />
                    <span className="sr-only">Título gerado automaticamente</span>
                  </span>
                ) : null}
                {meeting.pinned && (
                  <span title="Fixada" className="shrink-0 text-muted-foreground/80">
                    <Pin className="h-3 w-3" aria-hidden="true" />
                    <span className="sr-only">Fixada</span>
                  </span>
                )}
              </span>
              {subtitle && (
                <span className="block truncate text-xs leading-4 text-muted-foreground">{subtitle}</span>
              )}
              {snippet && (
                <span className="mt-1 line-clamp-2 block rounded-md bg-warning-soft px-1.5 py-1 text-xs text-foreground">
                  <span className="font-medium text-warning">Trecho:</span> {snippet}
                </span>
              )}
            </button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  aria-label={`Ações de ${meeting.title || 'reunião'}`}
                  className="mr-1 mt-1 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity duration-150 hover:bg-accent hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100 data-[state=open]:opacity-100"
                >
                  <MoreHorizontal className="h-4 w-4" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-56">
                <MeetingMenuItems
                  meeting={meeting}
                  actions={actions}
                  organizeAvailable={organizeAvailable}
                  Item={DropdownMenuItem}
                  Separator={DropdownMenuSeparator}
                />
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-56">
          <MeetingMenuItems
            meeting={meeting}
            actions={actions}
            organizeAvailable={organizeAvailable}
            Item={ContextMenuItem}
            Separator={ContextMenuSeparator}
          />
        </ContextMenuContent>
      </ContextMenu>
    </li>
  );
});
