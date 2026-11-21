'use client';

import { useState, useEffect, useRef } from 'react';
import { RecordingControls } from '@/components/RecordingControls';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useConfig } from '@/contexts/ConfigContext';
import { StatusOverlays } from '@/app/_components/StatusOverlays';
import Analytics from '@/lib/analytics';
import { SettingsModals } from './_components/SettingsModal';
import { TranscriptPanel } from './_components/TranscriptPanel';
import { useModalState } from '@/hooks/useModalState';
import { useRecordingStateSync } from '@/hooks/useRecordingStateSync';
import { useRecordingStart } from '@/hooks/useRecordingStart';
import { useRecordingStop } from '@/hooks/useRecordingStop';
import { useTranscriptRecovery } from '@/hooks/useTranscriptRecovery';
import { TranscriptRecovery } from '@/components/TranscriptRecovery';
import { indexedDBService } from '@/services/indexedDBService';
import { toast } from 'sonner';
import { useRouter } from 'next/navigation';
import { LiveAssistPanel } from '@/components/LiveAssist/LiveAssistPanel';
import { AutoRecordBanner } from '@/components/AutoRecord/AutoRecordBanner';
import { RecursosBar } from '@/components/Recursos/RecursosBar';
import { useLiveAssist } from '@/contexts/LiveAssistContext';
import { TopBar } from '@/components/Shell/TopBar';
import { IconButton } from '@/components/ui/icon-button';
import { Copy, Languages, SlidersHorizontal } from 'lucide-react';
import { formatElapsed } from '@/lib/elapsed';


// Resting shape of the level bars; while recording they move with a CSS animation.
const BAR_HEIGHTS = ['58%', '76%', '58%'];
export default function Home() {
  // Local page state (not moved to contexts)
  const [isRecording, setIsRecordingState] = useState(false);
  const barHeights = BAR_HEIGHTS;
  const [showRecoveryDialog, setShowRecoveryDialog] = useState(false);

  // Use contexts for state management
  const { meetingTitle, transcripts, copyTranscript } = useTranscripts();
  const { transcriptModelConfig, selectedDevices } = useConfig();
  const recordingState = useRecordingState();

  // Extract status from global state
  const { status, isStopping, isProcessing, isSaving } = recordingState;

  // Hooks
  const { hasMicrophone } = usePermissionCheck();
  const { setIsMeetingActive, isCollapsed: sidebarCollapsed, refetchMeetings } = useSidebar();
  const { modals, messages, showModal, hideModal } = useModalState(transcriptModelConfig);
  const { isRecordingDisabled, setIsRecordingDisabled } = useRecordingStateSync(isRecording, setIsRecordingState, setIsMeetingActive);
  const { handleRecordingStart } = useRecordingStart(isRecording, setIsRecordingState, showModal);

  // Get handleRecordingStop function and setIsStopping (state comes from global context)
  const { handleRecordingStop, setIsStopping } = useRecordingStop(
    setIsRecordingState,
    setIsRecordingDisabled
  );

  // Recovery hook
  const {
    recoverableMeetings,
    isLoading: isLoadingRecovery,
    isRecovering,
    checkForRecoverableTranscripts,
    recoverMeeting,
    loadMeetingTranscripts,
    deleteRecoverableMeeting
  } = useTranscriptRecovery();

  const router = useRouter();
  const { settings: liveAssistSettings, status: liveAssistStatus, panelOpen, setPanelOpen } = useLiveAssist();

  useEffect(() => {
    // Track page view
    Analytics.trackPageView('home');
  }, []);

  // Startup recovery check
  useEffect(() => {
    const performStartupChecks = async () => {
      try {
        // Skip recovery check if currently recording or processing stop
        // This prevents the recovery dialog from showing when:
        if (recordingState.isRecording ||
          status === RecordingStatus.STOPPING ||
          status === RecordingStatus.PROCESSING_TRANSCRIPTS ||
          status === RecordingStatus.SAVING) {
          console.log('Skipping recovery check - recording in progress or processing');
          return;
        }

        // 1. Clean up old meetings (7+ days)
        try {
          await indexedDBService.deleteOldMeetings(7);
        } catch (error) {
          console.warn('⚠️ Failed to clean up old meetings:', error);
        }

        // 2. Clean up saved meetings (24+ hours after save)
        try {
          await indexedDBService.deleteSavedMeetings(24);
        } catch (error) {
          console.warn('⚠️ Failed to clean up saved meetings:', error);
        }

        // 3. Always check for recoverable meetings on startup
        // Don't skip based on sessionStorage - we need to check every time
        await checkForRecoverableTranscripts();
      } catch (error) {
        console.error('Failed to perform startup checks:', error);
      }
    };

    performStartupChecks();
  }, [checkForRecoverableTranscripts, recordingState.isRecording, status]);

  // Watch for recoverable meetings changes and show dialog once per session
  const recoveryNavTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => {
    if (recoveryNavTimer.current) clearTimeout(recoveryNavTimer.current);
  }, []);

  // With nothing left to recover, the dialog may show again in a later session.
  // (Read from the current list: the recover handler only sees the list from before it ran.)
  useEffect(() => {
    if (recoverableMeetings.length === 0) sessionStorage.removeItem('recovery_dialog_shown');
  }, [recoverableMeetings]);

  useEffect(() => {
    // Only show dialog if we have meetings and haven't shown it yet this session
    if (recoverableMeetings.length > 0) {
      const shownThisSession = sessionStorage.getItem('recovery_dialog_shown');
      if (!shownThisSession) {
        setShowRecoveryDialog(true);
        sessionStorage.setItem('recovery_dialog_shown', 'true');
      }
    }
  }, [recoverableMeetings]);

  // Handle recovery with toast notifications and navigation
  const handleRecovery = async (meetingId: string) => {
    try {
      const result = await recoverMeeting(meetingId);

      if (result.success) {
        toast.success('Reunião recuperada', {
          description: result.audioRecoveryStatus?.status === 'success'
            ? 'Transcrição e áudio recuperados'
            : 'Transcrição recuperada (sem áudio disponível)',
          action: result.meetingId ? {
            label: 'Ver reunião',
            onClick: () => {
              // The person chose to go now: drop the automatic navigation below.
              if (recoveryNavTimer.current) clearTimeout(recoveryNavTimer.current);
              recoveryNavTimer.current = null;
              router.push(`/meeting-details?id=${result.meetingId}`);
            }
          } : undefined,
          duration: 10000,
        });

        // Refresh sidebar to show the newly recovered meeting
        await refetchMeetings();

        // Auto-navigate after a short delay
        if (result.meetingId) {
          if (recoveryNavTimer.current) clearTimeout(recoveryNavTimer.current);
          recoveryNavTimer.current = setTimeout(() => {
            recoveryNavTimer.current = null;
            router.push(`/meeting-details?id=${result.meetingId}`);
          }, 2000);
        }
      }
    } catch (error) {
      toast.error('Não foi possível recuperar a reunião', {
        description: error instanceof Error ? error.message : 'Erro desconhecido',
      });
      throw error;
    }
  };

  // Handle dialog close - clear session flag if no meetings left
  const handleDialogClose = () => {
    setShowRecoveryDialog(false);
    // If user closes dialog and there are no more meetings, clear the flag
    // This allows the dialog to show again next session if new meetings appear
    if (recoverableMeetings.length === 0) {
      sessionStorage.removeItem('recovery_dialog_shown');
    }
  };

  // Computed values using global status
  const isProcessingStop = status === RecordingStatus.PROCESSING_TRANSCRIPTS || isProcessing;

  const isHelpVisible = recordingState.isRecording && liveAssistSettings.enabled;
  const elapsedLabel = formatElapsed(recordingState.activeDuration);
  const hasLiveTitle = recordingState.isRecording && meetingTitle && meetingTitle !== '+ New Call';

  return (
    <div className="flex h-full flex-col bg-background">
      {/* All Modals supported*/}
      <SettingsModals
        modals={modals}
        messages={messages}
        onClose={hideModal}
      />

      {/* Recovery Dialog */}
      <TranscriptRecovery
        isOpen={showRecoveryDialog}
        onClose={handleDialogClose}
        recoverableMeetings={recoverableMeetings}
        onRecover={handleRecovery}
        onDelete={deleteRecoverableMeeting}
        onLoadPreview={loadMeetingTranscripts}
      />
      <TopBar
        title={<span className="block truncate">{hasLiveTitle ? meetingTitle : 'Nova reunião'}</span>}
        chips={recordingState.isRecording ? (
          <span className="inline-flex items-center gap-1.5 rounded-md border px-1.5 py-0.5 text-xs text-muted-foreground">
            <span className={`h-1.5 w-1.5 rounded-full bg-recording ${recordingState.isPaused ? '' : 'animate-pulse'}`} aria-hidden="true" />
            {recordingState.isPaused ? 'Pausada' : 'Gravando'}
          </span>
        ) : undefined}
        actions={
          <>
            {transcripts?.length > 0 && (
              <IconButton label="Copiar transcrição" onClick={copyTranscript}>
                <Copy />
              </IconButton>
            )}
            {transcriptModelConfig.provider === 'localWhisper' && (
              <IconButton label="Idioma da transcrição" onClick={() => showModal('languageSettings')}>
                <Languages />
              </IconButton>
            )}
          </>
        }
      />
      <AutoRecordBanner />
      <div className="relative flex min-h-0 flex-1 overflow-hidden">
        <TranscriptPanel
          isProcessingStop={isProcessingStop}
          isStopping={isStopping}
          showModal={showModal}
        />
        <LiveAssistPanel />

        {/* Barra de gravação: sempre visível, centralizada embaixo, como um campo de mensagem */}
        {(hasMicrophone || isRecording) &&
          status !== RecordingStatus.PROCESSING_TRANSCRIPTS &&
          status !== RecordingStatus.SAVING && (
            <div className="pointer-events-none absolute inset-x-0 bottom-5 z-10 flex justify-center px-4">
              <div
                role="toolbar"
                aria-label="Controles de gravação"
                className="pointer-events-auto flex h-14 max-w-full items-center gap-1 rounded-full border bg-card pl-3 pr-2 shadow-[0_4px_20px_hsl(0_0%_0%/0.08)]"
              >
                <RecursosBar />
                {isHelpVisible && (
                  <button
                    type="button"
                    aria-pressed={panelOpen}
                    onClick={() => setPanelOpen(!panelOpen)}
                    className={`inline-flex h-8 items-center gap-2 rounded-full px-3 text-xs font-medium transition-colors duration-150 ${
                      panelOpen ? 'bg-brand-soft text-brand' : 'text-muted-foreground hover:bg-accent hover:text-foreground'
                    }`}
                  >
                    <span className={`h-2 w-2 rounded-full ${liveAssistStatus.busy ? 'bg-amber-500' : 'bg-emerald-500'}`} aria-hidden="true" />
                    Ajuda ao vivo
                  </button>
                )}
                <span className="mx-1 h-6 w-px bg-border" aria-hidden="true" />
                <RecordingControls
                  isRecording={recordingState.isRecording}
                  onRecordingStop={(callApi = true) => handleRecordingStop(callApi)}
                  onRecordingStart={handleRecordingStart}
                  onTranscriptReceived={() => { }} // Not actually used by RecordingControls
                  onStopInitiated={() => setIsStopping(true)}
                  barHeights={barHeights}
                  onTranscriptionError={(message) => {
                    showModal('errorAlert', message);
                  }}
                  isRecordingDisabled={isRecordingDisabled}
                  isParentProcessing={isProcessingStop}
                  selectedDevices={selectedDevices}
                  meetingName={meetingTitle}
                />
                <span
                  className={`w-12 text-center text-[13px] tabular-nums ${recordingState.isRecording ? 'text-foreground' : 'text-muted-foreground'}`}
                  aria-label={recordingState.isRecording ? `Tempo de gravação ${elapsedLabel}` : 'Gravação parada'}
                >
                  {recordingState.isRecording ? elapsedLabel : '00:00'}
                </span>
                <span className="mx-1 h-6 w-px bg-border" aria-hidden="true" />
                <IconButton
                  label={recordingState.isRecording ? 'Dispositivos de áudio (bloqueado durante a gravação)' : 'Dispositivos de áudio'}
                  tooltipSide="top"
                  onClick={() => showModal('deviceSettings')}
                  className="rounded-full"
                >
                  <SlidersHorizontal />
                </IconButton>
              </div>
            </div>
          )}

        {/* Status Overlays - Processing and Saving */}
        <StatusOverlays
          isProcessing={status === RecordingStatus.PROCESSING_TRANSCRIPTS && !recordingState.isRecording}
          isSaving={status === RecordingStatus.SAVING}
          sidebarCollapsed={sidebarCollapsed}
        />
      </div>
    </div>
  );
}
