"use client";

import { useState, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, FolderOpen, RefreshCw } from 'lucide-react';
import Analytics from '@/lib/analytics';
import { RetranscribeDialog } from './RetranscribeDialog';
import { useConfig } from '@/contexts/ConfigContext';


interface TranscriptButtonGroupProps {
  transcriptCount: number;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
}


export function TranscriptButtonGroup({
  transcriptCount,
  onCopyTranscript,
  onOpenMeetingFolder,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
}: TranscriptButtonGroupProps) {
  const { betaFeatures } = useConfig();
  const [showRetranscribeDialog, setShowRetranscribeDialog] = useState(false);

  const handleRetranscribeComplete = useCallback(async () => {
    // Refetch transcripts to show the updated data
    if (onRefetchTranscripts) {
      await onRefetchTranscripts();
    }
  }, [onRefetchTranscripts]);

  return (
    <div className="flex items-center justify-center w-full gap-2">
      <ButtonGroup>
        <Button
          variant="outline"
          size="sm"
          className="px-2 @[22rem]:px-3"
          onClick={() => {
            Analytics.trackButtonClick('copy_transcript', 'meeting_details');
            onCopyTranscript();
          }}
          disabled={transcriptCount === 0}
          title={transcriptCount === 0 ? 'Não há transcrição' : 'Copiar transcrição'}
        >
          <Copy />
          <span className="hidden @[22rem]:inline">Copiar</span>
        </Button>

        <Button
          size="sm"
          variant="outline"
          className="px-2 @[22rem]:px-4"
          onClick={() => {
            Analytics.trackButtonClick('open_recording_folder', 'meeting_details');
            onOpenMeetingFolder();
          }}
          title="Abrir pasta da gravação"
        >
          <FolderOpen className="@[22rem]:mr-2" size={18} />
          <span className="hidden @[22rem]:inline">Gravação</span>
        </Button>

        {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="bg-brand-soft hover:bg-primary/20 text-brand border-primary/30 px-2 @[22rem]:px-4"
            onClick={() => {
              Analytics.trackButtonClick('enhance_transcript', 'meeting_details');
              setShowRetranscribeDialog(true);
            }}
            title="Transcrever de novo para melhorar a transcrição do áudio gravado"
          >
            <RefreshCw className="@[22rem]:mr-2" size={18} />
            <span className="hidden @[22rem]:inline">Melhorar</span>
          </Button>
        )}
      </ButtonGroup>

      {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
        <RetranscribeDialog
          open={showRetranscribeDialog}
          onOpenChange={setShowRetranscribeDialog}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onComplete={handleRetranscribeComplete}
        />
      )}
    </div>
  );
}
