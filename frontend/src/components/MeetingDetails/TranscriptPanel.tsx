"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { useEffect, useMemo, useState } from 'react';
import { ConversationView } from './ConversationView';
import { PeopleAndTasks } from './PeopleAndTasks';
import { cn } from '@/lib/utils';

type PanelView = 'chat' | 'transcript' | 'people';

const VIEWS: Array<{ value: PanelView; label: string; short?: string }> = [
  { value: 'chat', label: 'Conversa' },
  { value: 'transcript', label: 'Transcrição' },
  { value: 'people', label: 'Pessoas e tarefas', short: 'Pessoas' },
];

const VIEW_KEY = 'meetily.meetingDetails.leftView';

interface TranscriptPanelProps {
  transcripts: Transcript[];
  customPrompt: string;
  onPromptChange: (value: string) => void;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  isRecording: boolean;
  disableAutoScroll?: boolean;

  // Optional pagination props (when using virtualization)
  usePagination?: boolean;
  segments?: TranscriptSegmentData[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;

  // Retranscription props
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
  onRenameSpeaker?: (speakerKey: string, displayName: string) => void;
}

export function TranscriptPanel({
  transcripts,
  customPrompt,
  onPromptChange,
  onCopyTranscript,
  onOpenMeetingFolder,
  isRecording,
  disableAutoScroll = false,
  usePagination = false,
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
  onRenameSpeaker,
}: TranscriptPanelProps) {
  // Convert transcripts to segments if pagination is not used but we want virtualization
  const convertedSegments = useMemo(() => {
    if (usePagination && segments) {
      return segments;
    }
    // Convert transcripts to segments for virtualization
    return transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
      speaker: t.speaker,
      speakerKey: t.speaker_key,
    }));
  }, [transcripts, usePagination, segments]);

  const [view, setView] = useState<PanelView>('transcript');
  const [speakersVersion, setSpeakersVersion] = useState(0);
  const canSwitch = !!meetingId && !isRecording;

  useEffect(() => {
    try {
      const stored = localStorage.getItem(VIEW_KEY);
      if (stored && VIEWS.some((item) => item.value === stored)) setView(stored as PanelView);
    } catch {
      // Preferência opcional.
    }
  }, []);

  const changeView = (next: PanelView) => {
    setView(next);
    try {
      localStorage.setItem(VIEW_KEY, next);
    } catch {
      // Preferência opcional.
    }
  };

  const activeView: PanelView = canSwitch ? view : 'transcript';

  return (
    <div className="flex h-full min-w-0 w-full bg-background flex-col relative @container">
      {canSwitch && (
        <div role="tablist" aria-label="Como ver a reunião" className="flex shrink-0 gap-0.5 border-b border-border px-2 pt-1.5">
          {VIEWS.map((item) => (
            <button
              key={item.value}
              type="button"
              role="tab"
              aria-selected={activeView === item.value}
              title={item.label}
              onClick={() => changeView(item.value)}
              className={cn(
                '-mb-px whitespace-nowrap border-b-2 px-2 py-1.5 text-[13px] transition-colors',
                activeView === item.value
                  ? 'border-primary font-medium text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground',
              )}
            >
              {item.short ? (
                <>
                  <span className="@[21rem]:hidden">{item.short}</span>
                  <span className="hidden @[21rem]:inline">{item.label}</span>
                </>
              ) : item.label}
            </button>
          ))}
        </div>
      )}
      {activeView === 'chat' && meetingId && (
        <div className="min-h-0 flex-1">
          <ConversationView meetingId={meetingId} refreshKey={speakersVersion + convertedSegments.length} />
        </div>
      )}
      {activeView === 'people' && meetingId && (
        <div className="min-h-0 flex-1">
          <PeopleAndTasks
            meetingId={meetingId}
            onSpeakersChanged={() => {
              setSpeakersVersion((value) => value + 1);
              void onRefetchTranscripts?.();
            }}
          />
        </div>
      )}
      <div className={cn('min-h-0 flex-1 flex-col', activeView === 'transcript' ? 'flex' : 'hidden')}>
      {/* Title area */}
      <div className="px-3 py-2 border-b border-border">
        <TranscriptButtonGroup
          transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
          onCopyTranscript={onCopyTranscript}
          onOpenMeetingFolder={onOpenMeetingFolder}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onRefetchTranscripts={onRefetchTranscripts}
        />
      </div>

      {/* Transcript content - use virtualized view for better performance */}
      <div className="reading flex-1 overflow-hidden pb-4">
        <VirtualizedTranscriptView
          segments={convertedSegments}
          isRecording={isRecording}
          isPaused={false}
          isProcessing={false}
          isStopping={false}
          enableStreaming={false}
          showConfidence={true}
          disableAutoScroll={disableAutoScroll}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
          onRenameSpeaker={onRenameSpeaker}
        />
      </div>

      {/* Custom prompt input at bottom of transcript section */}
      {!isRecording && convertedSegments.length > 0 && (
        <div className="p-2 border-t border-border">
          <textarea
            placeholder="Contexto para o resumo: quem participou, objetivo da reunião, pontos importantes…"
            className="w-full px-3 py-2 border border-border rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-ring focus:border-primary bg-card min-h-[72px] resize-y"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
        </div>
      )}
      </div>
    </div>
  );
}
