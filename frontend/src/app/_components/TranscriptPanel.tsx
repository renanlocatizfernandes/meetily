import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { PermissionWarning } from '@/components/PermissionWarning';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { ModalType } from '@/hooks/useModalState';
import { useIsLinux } from '@/hooks/usePlatform';
import { useMemo } from 'react';

/**
 * TranscriptPanel Component
 *
 * Displays transcript content with controls for copying and language settings.
 * Uses TranscriptContext, ConfigContext, and RecordingStateContext internally.
 */

interface TranscriptPanelProps {
  // indicates stop-processing state for transcripts; derived from backend statuses.
  isProcessingStop: boolean;
  isStopping: boolean;
  showModal: (name: ModalType, message?: string) => void;
}

export function TranscriptPanel({
  isProcessingStop,
  isStopping,
  showModal
}: TranscriptPanelProps) {
  // Contexts
  const { transcripts, transcriptContainerRef, renameSpeaker } = useTranscripts();
  const { isRecording, isPaused } = useRecordingState();
  const { checkPermissions, isChecking, hasSystemAudio, hasMicrophone } = usePermissionCheck();
  const isLinux = useIsLinux();

  // Convert transcripts to segments for virtualized view
  const segments = useMemo(() =>
    transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
      speaker: t.speaker,
      speakerKey: t.speaker_key,
    })),
    [transcripts]
  );

  return (
    <div ref={transcriptContainerRef} className="flex min-w-0 flex-1 flex-col overflow-y-auto custom-scrollbar">
      {/* Permission Warning - Not needed on Linux */}
      {!isRecording && !isChecking && !isLinux && (
        <div className="flex justify-center px-4 pt-4">
          <PermissionWarning
            hasMicrophone={hasMicrophone}
            hasSystemAudio={hasSystemAudio}
            onRecheck={checkPermissions}
            isRechecking={isChecking}
          />
        </div>
      )}

      {/* Transcript content: coluna de leitura centralizada, com folga para a barra de gravação */}
      <div className="content-column reading flex-1 pb-28">
        <VirtualizedTranscriptView
          segments={segments}
          isRecording={isRecording}
          isPaused={isPaused}
          isProcessing={isProcessingStop}
          isStopping={isStopping}
          enableStreaming={isRecording}
          showConfidence={true}
          onRenameSpeaker={renameSpeaker}
        />
      </div>
    </div>
  );
}
