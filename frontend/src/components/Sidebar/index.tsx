'use client';

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Check,
  ListFilter,
  MessageCircleQuestion,
  MessagesSquare,
  Mic,
  NotebookText,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Search,
  Settings,
  Upload,
  X,
} from 'lucide-react';
import { useRouter, usePathname } from 'next/navigation';
import { useSidebar } from './SidebarProvider';
import { ConversationList } from './ConversationList';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useImportDialog } from '@/contexts/ImportDialogContext';
import { useConfig } from '@/contexts/ConfigContext';
import { useFeatures } from '@/contexts/FeatureContext';
import { Button } from '@/components/ui/button';
import { IconButton } from '@/components/ui/icon-button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ThemeSwitch } from '@/components/Shell/ThemeSwitch';
import { SearchDialog } from '@/components/Shell/SearchDialog';
import { useMeetingActions } from '@/components/Organize/MeetingActions';
import { GROUP_BY_LABELS, meetingMatchesQuery, type GroupBy, type MeetingOrg } from '@/lib/organize';
import Logo from '../Logo';
import { formatElapsed } from '@/lib/elapsed';

const GROUP_BY_OPTIONS: GroupBy[] = ['date', 'client', 'category'];


interface NavItemProps {
  icon: React.ReactNode;
  label: string;
  active?: boolean;
  shortcut?: string;
  onClick: () => void;
}

function NavItem({ icon, label, active = false, shortcut, onClick }: NavItemProps) {
  return (
    <button
      type="button"
      aria-current={active ? 'page' : undefined}
      onClick={onClick}
      className={`flex h-8 w-full items-center gap-2.5 rounded-lg px-2 text-[13px] transition-colors duration-150 [&_svg]:size-4 [&_svg]:shrink-0 ${
        active
          ? 'bg-sidebar-accent font-medium text-foreground'
          : 'text-sidebar-foreground hover:bg-sidebar-accent/70'
      }`}
    >
      {icon}
      <span className="truncate">{label}</span>
      {shortcut && <kbd className="ml-auto font-sans text-xs text-muted-foreground">{shortcut}</kbd>}
    </button>
  );
}

const Sidebar: React.FC = () => {
  const router = useRouter();
  const pathname = usePathname();
  const {
    currentMeeting,
    setCurrentMeeting,
    isCollapsed,
    doodlePath,
    doodleDate,
    toggleCollapse,
    handleRecordingToggle,
    searchTranscripts,
    searchResults,
    isSearching,
    organizedMeetings,
    organizeAvailable,
    isLoadingMeetings,
    groupBy,
    setGroupBy,
  } = useSidebar();

  // Recording state from RecordingStateContext (single source of truth)
  const { isRecording, isPaused, activeDuration } = useRecordingState();
  const { openImportDialog } = useImportDialog();
  const { betaFeatures } = useConfig();
  const { toggles } = useFeatures();
  const { actions, dialogs } = useMeetingActions();

  const [searchQuery, setSearchQuery] = useState('');
  const [activeFilter, setActiveFilter] = useState<{ kind: 'client' | 'category' | 'pinned'; value: string } | null>(null);
  const [searchDialogOpen, setSearchDialogOpen] = useState(false);

  const isHomePage = pathname === '/';
  const isMeetingPage = pathname?.includes('/meeting-details') ?? false;
  const isAskPage = pathname === '/ask';
  const isSettingsPage = pathname === '/settings';
  const isFeedbackPage = pathname === '/feedback';

  const goToSettings = useCallback(() => {
    if (pathname === '/feedback') router.replace('/settings');
    else router.push('/settings');
  }, [pathname, router]);

  const goToFeedback = useCallback(() => {
    if (pathname === '/settings') router.replace('/feedback');
    else router.push('/feedback');
  }, [pathname, router]);

  const startNewMeeting = useCallback(() => {
    setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
    router.push('/');
  }, [router, setCurrentMeeting]);

  const openMeeting = useCallback((meeting: MeetingOrg) => {
    setCurrentMeeting({ id: meeting.id, title: meeting.title });
    const basePath = meeting.id.startsWith('intro-call') ? '/' :
      meeting.id.includes('-') ? `/meeting-details?id=${meeting.id}` : `/notes/${meeting.id}`;
    router.push(basePath);
  }, [router, setCurrentMeeting]);

  // The tray menu calls window.openSettings()
  useEffect(() => {
    (window as any).openSettings = () => {
      router.push('/settings');
    };
    return () => {
      delete (window as any).openSettings;
    };
  }, [router]);

  // ⌘K / Ctrl+K abre Buscar / Perguntar
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setSearchDialogOpen(true);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  // Busca no texto das transcrições enquanto a pessoa digita
  const handleSearchChange = useCallback(async (value: string) => {
    setSearchQuery(value);
    if (!value.trim()) return;
    await searchTranscripts(value);
  }, [searchTranscripts]);

  const trimmedQuery = searchQuery.trim();

  const snippets = useMemo(() => {
    const map = new Map<string, string>();
    if (trimmedQuery) {
      searchResults.forEach((result) => map.set(result.id, result.matchContext));
    }
    return map;
  }, [searchResults, trimmedQuery]);

  // Chips de filtro: clientes e categorias mais frequentes
  const filterChips = useMemo(() => {
    const count = (field: 'client' | 'category') => {
      const totals = new Map<string, number>();
      organizedMeetings.forEach((meeting) => {
        const value = meeting[field];
        if (value) totals.set(value, (totals.get(value) ?? 0) + 1);
      });
      return [...totals.entries()].sort((a, b) => b[1] - a[1]).slice(0, 4).map(([value]) => ({ kind: field, value }));
    };
    const chips: Array<{ kind: 'client' | 'category' | 'pinned'; value: string }> = [];
    if (organizedMeetings.some((meeting) => meeting.pinned)) chips.push({ kind: 'pinned', value: 'Fixadas' });
    return [...chips, ...count('client'), ...count('category')];
  }, [organizedMeetings]);

  const visibleMeetings = useMemo(() => {
    return organizedMeetings.filter((meeting) => {
      if (activeFilter) {
        if (activeFilter.kind === 'pinned' && !meeting.pinned) return false;
        if (activeFilter.kind === 'client' && meeting.client !== activeFilter.value) return false;
        if (activeFilter.kind === 'category' && meeting.category !== activeFilter.value) return false;
      }
      if (!trimmedQuery) return true;
      return meetingMatchesQuery(meeting, trimmedQuery) || snippets.has(meeting.id);
    });
  }, [organizedMeetings, activeFilter, trimmedQuery, snippets]);

  // Um filtro que deixou de existir (reunião excluída ou reclassificada) é removido
  useEffect(() => {
    if (activeFilter && !filterChips.some((chip) => chip.kind === activeFilter.kind && chip.value === activeFilter.value)) {
      setActiveFilter(null);
    }
  }, [activeFilter, filterChips]);

  const activeId = isMeetingPage ? currentMeeting?.id ?? null : null;
  const hasFilter = trimmedQuery !== '' || activeFilter !== null;

  const recordingLabel = isRecording
    ? `${isPaused ? 'Pausada' : 'Gravando'} · ${formatElapsed(activeDuration)}`
    : 'Gravar';

  const emptyState = hasFilter ? (
    <div className="px-4 py-8 text-center">
      <p className="text-[13px] text-muted-foreground">
        {trimmedQuery ? `Nada encontrado para “${trimmedQuery}”.` : 'Nenhuma reunião com este filtro.'}
      </p>
      <Button
        variant="ghost"
        size="sm"
        className="mt-2"
        onClick={() => {
          setSearchQuery('');
          setActiveFilter(null);
        }}
      >
        Limpar busca
      </Button>
    </div>
  ) : (
    <div className="px-4 py-8 text-center">
      <p className="text-[13px] text-muted-foreground">Suas reuniões gravadas aparecem aqui.</p>
      <Button variant="outline" size="sm" className="mt-3" onClick={startNewMeeting}>
        <Plus />
        Nova reunião
      </Button>
    </div>
  );

  const sharedOverlays = (
    <>
      {dialogs}
      <SearchDialog open={searchDialogOpen} onOpenChange={setSearchDialogOpen} onOpenMeeting={openMeeting} />
    </>
  );

  /* ---------- Trilho de ícones (recolhida) ---------- */
  if (isCollapsed) {
    return (
      <aside
        aria-label="Barra lateral"
        className="flex h-screen w-14 shrink-0 flex-col items-center gap-1 border-r border-sidebar-border bg-sidebar py-2 text-sidebar-foreground transition-[width] duration-150"
      >
        <IconButton label="Expandir barra lateral" tooltipSide="right" onClick={toggleCollapse}>
          <PanelLeftOpen />
        </IconButton>
        <div className="my-1 scale-[0.7]">
          <Logo isCollapsed doodlePath={doodlePath} doodleDate={doodleDate} />
        </div>
        <IconButton
          label="Nova reunião"
          tooltipSide="right"
          onClick={startNewMeeting}
          className="bg-primary text-primary-foreground hover:bg-primary/90 hover:text-primary-foreground"
        >
          <Plus />
        </IconButton>
        <IconButton label="Buscar ou perguntar (⌘K)" tooltipSide="right" onClick={() => setSearchDialogOpen(true)}>
          <Search />
        </IconButton>
        {toggles.meetingSearch && (
          <IconButton label="Perguntar às reuniões" tooltipSide="right" active={isAskPage} onClick={() => router.push('/ask')}>
            <MessageCircleQuestion />
          </IconButton>
        )}
        <IconButton
          label="Reuniões"
          tooltipSide="right"
          active={isHomePage || isMeetingPage}
          onClick={toggleCollapse}
        >
          <NotebookText />
        </IconButton>
        {betaFeatures.importAndRetranscribe && (
          <IconButton label="Importar áudio" tooltipSide="right" onClick={() => openImportDialog()}>
            <Upload />
          </IconButton>
        )}
        <IconButton label="Configurações" tooltipSide="right" active={isSettingsPage} onClick={goToSettings}>
          <Settings />
        </IconButton>

        <div className="mt-auto flex flex-col items-center gap-1">
          <IconButton
            label={isRecording ? `${recordingLabel}. Abrir a gravação` : 'Iniciar gravação'}
            tooltipSide="right"
            onClick={() => (isRecording ? router.push('/') : handleRecordingToggle())}
            className={isRecording ? 'text-recording' : ''}
          >
            {isRecording ? (
              <span className={`h-2.5 w-2.5 rounded-full bg-recording ${isPaused ? '' : 'animate-pulse'}`} />
            ) : (
              <Mic />
            )}
          </IconButton>
          <ThemeSwitch compact />
          <IconButton label="Enviar feedback" tooltipSide="right" active={isFeedbackPage} onClick={goToFeedback}>
            <MessagesSquare />
          </IconButton>
        </div>
        {sharedOverlays}
      </aside>
    );
  }

  /* ---------- Barra completa ---------- */
  return (
    <aside
      aria-label="Barra lateral"
      className="flex h-screen w-[264px] shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground transition-[width] duration-150"
    >
      {/* Área segura de arrasto + nome do app */}
      <div className="titlebar flex h-12 shrink-0 items-center justify-between gap-2 pl-3 pr-2">
        <div className="no-drag min-w-0">
          <Logo isCollapsed={false} />
        </div>
        <div className="no-drag">
          <IconButton label="Recolher barra lateral" onClick={toggleCollapse}>
            <PanelLeftClose />
          </IconButton>
        </div>
      </div>

      <div className="shrink-0 space-y-2 px-2">
        <Button className="h-8 w-full justify-start gap-2 px-2.5 text-[13px]" onClick={startNewMeeting}>
          <Plus />
          Nova reunião
        </Button>

        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
          <input
            type="search"
            value={searchQuery}
            onChange={(event) => void handleSearchChange(event.target.value)}
            placeholder="Buscar nas reuniões"
            aria-label="Buscar nas reuniões"
            className="h-8 w-full rounded-lg border border-sidebar-border bg-background/60 pl-8 pr-8 text-[13px] outline-none transition-colors placeholder:text-muted-foreground focus-visible:border-ring focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 [&::-webkit-search-cancel-button]:hidden"
          />
          {searchQuery && (
            <button
              type="button"
              aria-label="Limpar busca"
              onClick={() => void handleSearchChange('')}
              className="absolute right-1 top-1/2 inline-flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </div>

        <nav aria-label="Navegação principal" className="space-y-0.5">
          <NavItem
            icon={<NotebookText />}
            label="Reuniões"
            active={isHomePage || isMeetingPage}
            onClick={() => router.push('/')}
          />
          <NavItem
            icon={<Search />}
            label="Buscar / Perguntar"
            shortcut="⌘K"
            onClick={() => setSearchDialogOpen(true)}
          />
          {toggles.meetingSearch && (
            <NavItem
              icon={<MessageCircleQuestion />}
              label="Perguntar às reuniões"
              active={isAskPage}
              onClick={() => router.push('/ask')}
            />
          )}
          {betaFeatures.importAndRetranscribe && (
            <NavItem icon={<Upload />} label="Importar áudio" onClick={() => openImportDialog()} />
          )}
          <NavItem icon={<Settings />} label="Configurações" active={isSettingsPage} onClick={goToSettings} />
        </nav>
      </div>

      {/* Conversas */}
      <div className="mt-3 flex min-h-0 flex-1 flex-col border-t border-sidebar-border">
        <div className="flex h-9 shrink-0 items-center justify-between pl-4 pr-2">
          <span className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
            Conversas
            {trimmedQuery && isSearching && <span className="animate-pulse font-normal">buscando…</span>}
          </span>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                aria-label={`Agrupar por: ${GROUP_BY_LABELS[groupBy]}`}
                className="inline-flex h-7 items-center gap-1.5 rounded-md px-1.5 text-xs text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground"
              >
                <ListFilter className="h-3.5 w-3.5" aria-hidden="true" />
                {GROUP_BY_LABELS[groupBy]}
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-44">
              <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">Agrupar por</DropdownMenuLabel>
              {GROUP_BY_OPTIONS.map((option) => (
                <DropdownMenuItem
                  key={option}
                  disabled={option !== 'date' && !organizeAvailable}
                  onSelect={() => setGroupBy(option)}
                >
                  <span className="flex-1">{GROUP_BY_LABELS[option]}</span>
                  {groupBy === option && <Check className="text-brand" />}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        {filterChips.length > 0 && (
          <div className="flex max-h-[3.75rem] shrink-0 flex-wrap gap-1 overflow-y-auto px-3 pb-2 custom-scrollbar" aria-label="Filtros">
            {filterChips.map((chip) => {
              const selected = activeFilter?.kind === chip.kind && activeFilter.value === chip.value;
              return (
                <button
                  key={`${chip.kind}:${chip.value}`}
                  type="button"
                  aria-pressed={selected}
                  onClick={() => setActiveFilter(selected ? null : chip)}
                  className={`h-6 max-w-[9rem] shrink-0 truncate rounded-full border px-2 text-xs transition-colors duration-150 ${
                    selected
                      ? 'border-primary bg-primary text-primary-foreground'
                      : 'border-sidebar-border text-muted-foreground hover:bg-sidebar-accent hover:text-foreground'
                  }`}
                >
                  {chip.value}
                </button>
              );
            })}
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto custom-scrollbar">
          <ConversationList
            meetings={visibleMeetings}
            groupBy={organizeAvailable ? groupBy : 'date'}
            activeId={activeId}
            isLoading={isLoadingMeetings}
            organizeAvailable={organizeAvailable}
            actions={actions}
            snippets={snippets}
            forceExpanded={hasFilter}
            onOpen={openMeeting}
            emptyState={emptyState}
          />
        </div>
      </div>

      {/* Rodapé: gravação, tema, feedback e configurações */}
      <div className="flex h-11 shrink-0 items-center gap-1 border-t border-sidebar-border px-2">
        <button
          type="button"
          onClick={() => (isRecording ? router.push('/') : handleRecordingToggle())}
          aria-label={isRecording ? `${recordingLabel}. Abrir a gravação` : 'Iniciar gravação'}
          title={isRecording ? recordingLabel : undefined}
          className={`inline-flex h-7 min-w-0 shrink items-center gap-2 rounded-md px-2 text-xs transition-colors duration-150 hover:bg-sidebar-accent ${
            isRecording ? 'font-medium text-recording' : 'text-muted-foreground hover:text-foreground'
          }`}
        >
          {isRecording ? (
            <span className={`h-2 w-2 shrink-0 rounded-full bg-recording ${isPaused ? '' : 'animate-pulse'}`} aria-hidden="true" />
          ) : (
            <Mic className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          )}
          {/* Durante a gravação mostra só o tempo, para caber ao lado dos ícones */}
          <span className="truncate tabular-nums">
            {isRecording ? formatElapsed(activeDuration) : recordingLabel}
          </span>
        </button>
        <span className="ml-auto" />
        <div className="flex shrink-0 items-center gap-1">
        <ThemeSwitch />
        <IconButton label="Enviar feedback" tooltipSide="top" size="sm" active={isFeedbackPage} onClick={goToFeedback}>
          <MessagesSquare />
        </IconButton>
        <IconButton label="Configurações" tooltipSide="top" size="sm" active={isSettingsPage} onClick={goToSettings}>
          <Settings />
        </IconButton>
        </div>
      </div>
      {sharedOverlays}
    </aside>
  );
};

export default Sidebar;
