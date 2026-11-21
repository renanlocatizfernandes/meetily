'use client';

import React, { createContext, useContext, useState, useEffect } from 'react';
import { usePathname, useRouter } from 'next/navigation';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import type { SummaryProcessResponse } from '@/types';
import {
  listMeetingsOrganized,
  listenOrganizeEvents,
  plainMeetingOrg,
  type GroupBy,
  type MeetingOrg,
} from '@/lib/organize';

const COLLAPSED_STORAGE_KEY = 'meetily.sidebar.collapsed';
const GROUP_BY_STORAGE_KEY = 'meetily.sidebar.groupBy';

function readStored(key: string): string | null {
  try {
    return typeof window === 'undefined' ? null : localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStored(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Preferência de layout é opcional.
  }
}



interface SidebarItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  children?: SidebarItem[];
}

export interface CurrentMeeting {
  id: string;
  title: string;
}

// Search result type for transcript search
interface TranscriptSearchResult {
  id: string;
  title: string;
  matchContext: string;
  timestamp: string;
}

interface SummaryPoll {
  processId: string;
  timer: NodeJS.Timeout;
  inFlight: boolean;
}

interface SidebarContextType {
  currentMeeting: CurrentMeeting | null;
  setCurrentMeeting: (meeting: CurrentMeeting | null) => void;
  sidebarItems: SidebarItem[];
  isCollapsed: boolean;
  doodlePath: string | null;
  doodleDate: string | null;
  toggleCollapse: () => void;
  meetings: CurrentMeeting[];
  setMeetings: (meetings: CurrentMeeting[]) => void;
  isMeetingActive: boolean;
  setIsMeetingActive: (active: boolean) => void;
  handleRecordingToggle: () => void;
  searchTranscripts: (query: string) => Promise<void>;
  searchResults: TranscriptSearchResult[];
  isSearching: boolean;
  setServerAddress: (address: string) => void;
  serverAddress: string;
  transcriptServerAddress: string;
  setTranscriptServerAddress: (address: string) => void;
  // Summary polling management
  startSummaryPolling: (
    meetingId: string,
    processId: string,
    onUpdate: (result: SummaryProcessResponse) => void | Promise<void>
  ) => void;
  stopSummaryPolling: (meetingId: string, processId?: string) => void;
  // Refetch meetings from backend
  refetchMeetings: () => Promise<void>;
  // Conversas organizadas (cliente, categoria, fixadas). Cai para a lista simples se o backend não tiver o recurso.
  organizedMeetings: MeetingOrg[];
  organizeAvailable: boolean;
  isLoadingMeetings: boolean;
  upsertOrganizedMeeting: (meeting: MeetingOrg) => void;
  groupBy: GroupBy;
  setGroupBy: (groupBy: GroupBy) => void;

}

const SidebarContext = createContext<SidebarContextType | null>(null);

export const useSidebar = () => {
  const context = useContext(SidebarContext);
  if (!context) {
    throw new Error('useSidebar must be used within a SidebarProvider');
  }
  return context;
};

export function SidebarProvider({ children }: { children: React.ReactNode }) {
  const [currentMeeting, setCurrentMeeting] = useState<CurrentMeeting | null>({ id: 'intro-call', title: '+ New Call' });
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [organized, setOrganized] = useState<MeetingOrg[] | null>(null);
  const [isLoadingMeetings, setIsLoadingMeetings] = useState(true);
  const [groupBy, setGroupByState] = useState<GroupBy>('date');
  const [doodlePath, setDoodlePath] = useState<string | null>(null);
  const [doodleDate, setDoodleDate] = useState<string | null>(null);
  const [meetings, setMeetings] = useState<CurrentMeeting[]>([]);
  const [sidebarItems, setSidebarItems] = useState<SidebarItem[]>([]);
  const [isMeetingActive, setIsMeetingActive] = useState(false);
  const [searchResults, setSearchResults] = useState<any[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [serverAddress, setServerAddress] = useState('');
  const [transcriptServerAddress, setTranscriptServerAddress] = useState('');
  const summaryPollsRef = React.useRef(new Map<string, SummaryPoll>());

  // Use recording state from RecordingStateContext (single source of truth)
  const { isRecording } = useRecordingState();

  const pathname = usePathname();
  const router = useRouter();

  useEffect(() => {
    const sync = () => {
      invoke<string | null>('doodle_sync').then(path => {
        setDoodlePath(path);
        setDoodleDate(new Date().toISOString().slice(0, 10));
      }).catch(() => setDoodlePath(null));
    };
    const onVisibility = () => { if (document.visibilityState === 'visible') sync(); };
    sync();
    document.addEventListener('visibilitychange', onVisibility);
    return () => document.removeEventListener('visibilitychange', onVisibility);
  }, []);

  // Extract fetchMeetings as a reusable function
  const fetchMeetings = React.useCallback(async () => {
    if (serverAddress) {
      // A lista organizada é opcional: `null` significa "use a lista simples".
      const organizedPromise = listMeetingsOrganized();
      try {
        const meetings = await invoke('api_get_meetings') as Array<{ id: string, title: string }>;
        const transformedMeetings = meetings.map((meeting: any) => ({
          id: meeting.id,
          title: meeting.title
        }));
        setMeetings(transformedMeetings);
        setOrganized(await organizedPromise);
        Analytics.trackBackendConnection(true);
      } catch (error) {
        console.error('Error fetching meetings:', error);
        const organizedList = await organizedPromise;
        setOrganized(organizedList);
        setMeetings(organizedList ? organizedList.map(({ id, title }) => ({ id, title })) : []);
        Analytics.trackBackendConnection(false, error instanceof Error ? error.message : 'Unknown error');
      } finally {
        setIsLoadingMeetings(false);
      }
    }
  }, [serverAddress]);

  useEffect(() => {
    fetchMeetings();
  }, [serverAddress, fetchMeetings]);

  useEffect(() => {
    const fetchSettings = async () => {
      setServerAddress('http://localhost:5167');
      setTranscriptServerAddress('http://127.0.0.1:8178/stream');
    };
    fetchSettings();
  }, []);

  const baseItems: SidebarItem[] = [
    {
      id: 'meetings',
      title: 'Reuniões',
      type: 'folder' as const,
      children: [
        ...meetings.map(meeting => ({ id: meeting.id, title: meeting.title, type: 'file' as const }))
      ]
    },
  ];


  const toggleCollapse = () => {
    setIsCollapsed((current) => {
      writeStored(COLLAPSED_STORAGE_KEY, current ? '0' : '1');
      return !current;
    });
  };

  // Restaura as preferências da barra lateral
  useEffect(() => {
    setIsCollapsed(readStored(COLLAPSED_STORAGE_KEY) === '1');
    const storedGroupBy = readStored(GROUP_BY_STORAGE_KEY);
    if (storedGroupBy === 'date' || storedGroupBy === 'client' || storedGroupBy === 'category') {
      setGroupByState(storedGroupBy);
    }
  }, []);

  const setGroupBy = React.useCallback((next: GroupBy) => {
    setGroupByState(next);
    writeStored(GROUP_BY_STORAGE_KEY, next);
  }, []);

  // Atualiza (ou insere) uma linha organizada e mantém o título da lista simples em sincronia
  const upsertOrganizedMeeting = React.useCallback((meeting: MeetingOrg) => {
    setOrganized((current) => {
      if (current === null) return current;
      return current.some((item) => item.id === meeting.id)
        ? current.map((item) => (item.id === meeting.id ? meeting : item))
        : [meeting, ...current];
    });
    if (meeting.title) {
      setMeetings((current) => current.map((item) => (
        item.id === meeting.id && item.title !== meeting.title ? { ...item, title: meeting.title } : item
      )));
      setCurrentMeeting((current) => (
        current && current.id === meeting.id && current.title !== meeting.title
          ? { ...current, title: meeting.title }
          : current
      ));
    }
  }, []);

  // O backend avisa quando termina de organizar uma reunião
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    listenOrganizeEvents({ onOrganized: upsertOrganizedMeeting }).then((unlisten) => {
      if (disposed) unlisten();
      else cleanup = unlisten;
    });
    return () => {
      disposed = true;
      cleanup?.();
    };
  }, [upsertOrganizedMeeting]);

  // A lista simples é a fonte de quais reuniões existem e do título; a organizada acrescenta os metadados.
  const organizedMeetings = React.useMemo<MeetingOrg[]>(() => {
    if (organized === null) {
      return meetings.map((meeting) => plainMeetingOrg(meeting.id, meeting.title));
    }
    const byId = new Map(organized.map((item) => [item.id, item]));
    return meetings.map((meeting) => {
      const item = byId.get(meeting.id);
      return item ? { ...item, title: meeting.title || item.title } : plainMeetingOrg(meeting.id, meeting.title);
    });
  }, [organized, meetings]);

  // Update current meeting when on home page
  useEffect(() => {
    if (pathname === '/') {
      setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
    }
    setSidebarItems(baseItems);
  }, [pathname]);

  // Update sidebar items when meetings change
  useEffect(() => {
    setSidebarItems(baseItems);
  }, [meetings]);

  // Function to handle recording toggle from sidebar
  const handleRecordingToggle = () => {
    if (!isRecording) {
      // Check if already on home page
      if (pathname === '/') {
        // Already on home - trigger recording directly via custom event
        console.log('Triggering recording from sidebar (already on home page)');
        window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'));
      } else {
        // Not on home - navigate and use auto-start mechanism
        console.log('Navigating to home page with auto-start flag');
        sessionStorage.setItem('autoStartRecording', 'true');
        router.push('/');
      }

      // Track recording initiation from sidebar
      Analytics.trackButtonClick('start_recording', 'sidebar');
    }
    // The actual recording start/stop is handled in the Home component
  };

  // Each keystroke starts a search; only the newest one may show its results.
  const searchSeqRef = React.useRef(0);

  // Function to search through meeting transcripts
  const searchTranscripts = async (query: string) => {
    const id = ++searchSeqRef.current;
    if (!query.trim()) {
      setSearchResults([]);
      setIsSearching(false);
      return;
    }

    try {
      setIsSearching(true);
      const results = await invoke('api_search_transcripts', { query }) as TranscriptSearchResult[];
      if (id === searchSeqRef.current) setSearchResults(results);
    } catch (error) {
      console.error('Error searching transcripts:', error);
      if (id === searchSeqRef.current) setSearchResults([]);
    } finally {
      if (id === searchSeqRef.current) setIsSearching(false);
    }
  };

  // Summary polling management
  const stopSummaryPolling = React.useCallback((meetingId: string, processId?: string) => {
    const poll = summaryPollsRef.current.get(meetingId);
    if (!poll || (processId && poll.processId !== processId)) {
      return;
    }
    clearInterval(poll.timer);
    summaryPollsRef.current.delete(meetingId);
  }, []);

  const startSummaryPolling = React.useCallback((
    meetingId: string,
    processId: string,
    onUpdate: (result: SummaryProcessResponse) => void | Promise<void>
  ) => {
    stopSummaryPolling(meetingId);
    let pollCount = 0;
    const maxPolls = 200;
    const poll = async () => {
      const entry = summaryPollsRef.current.get(meetingId);
      if (!entry || entry.processId !== processId || entry.inFlight) {
        return;
      }
      entry.inFlight = true;
      try {
        pollCount += 1;
        if (pollCount >= maxPolls) {
          await onUpdate({
            status: 'error',
            meetingName: null,
            meeting_id: meetingId,
            start: processId,
            end: null,
            data: null,
            error: 'A geração do resumo passou de 15 minutos. Tente de novo ou confira a configuração do modelo.',
          });
          if (summaryPollsRef.current.get(meetingId) === entry) {
            stopSummaryPolling(meetingId, processId);
          }
          return;
        }

        const result = await invoke<SummaryProcessResponse>('api_get_summary', { meetingId });
        const current = summaryPollsRef.current.get(meetingId);
        if (current !== entry || result.start !== processId) {
          return;
        }
        await onUpdate(result);
        if (summaryPollsRef.current.get(meetingId) !== entry) return;
        if (
          result.status === 'completed'
          || result.status === 'error'
          || result.status === 'failed'
          || result.status === 'cancelled'
          || (result.status === 'idle' && pollCount > 1)
        ) {
          stopSummaryPolling(meetingId, processId);
        }
      } catch (error) {
        const current = summaryPollsRef.current.get(meetingId);
        if (current !== entry) {
          return;
        }
        try {
          await onUpdate({
            status: 'error',
            meetingName: null,
            meeting_id: meetingId,
            start: processId,
            end: null,
            data: null,
            error: error instanceof Error ? error.message : 'Erro desconhecido',
          });
        } catch (callbackError) {
          console.error('Failed to handle summary polling error:', callbackError);
        } finally {
          if (summaryPollsRef.current.get(meetingId) === entry) {
            stopSummaryPolling(meetingId, processId);
          }
        }
      } finally {
        const current = summaryPollsRef.current.get(meetingId);
        if (current === entry) {
          current.inFlight = false;
        }
      }
    };

    const timer = setInterval(() => void poll(), 5000);
    summaryPollsRef.current.set(meetingId, { processId, timer, inFlight: false });
  }, [stopSummaryPolling]);

  useEffect(() => () => {
    summaryPollsRef.current.forEach(({ timer }) => clearInterval(timer));
    summaryPollsRef.current.clear();
  }, []);



  return (
    <SidebarContext.Provider value={{
      currentMeeting,
      setCurrentMeeting,
      sidebarItems,
      isCollapsed,
      doodlePath,
      doodleDate,
      toggleCollapse,
      meetings,
      setMeetings,
      isMeetingActive,
      setIsMeetingActive,
      handleRecordingToggle,
      searchTranscripts,
      searchResults,
      isSearching,
      setServerAddress,
      serverAddress,
      transcriptServerAddress,
      setTranscriptServerAddress,
      startSummaryPolling,
      stopSummaryPolling,
      refetchMeetings: fetchMeetings,
      organizedMeetings,
      organizeAvailable: organized !== null,
      isLoadingMeetings,
      upsertOrganizedMeeting,
      groupBy,
      setGroupBy,

    }}>
      {children}
    </SidebarContext.Provider>
  );
}
