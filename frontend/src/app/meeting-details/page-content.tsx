"use client";
import { useState, useEffect, useRef } from 'react';
import { motion } from 'framer-motion';
import { MeetingSummary, SummaryProcessResponse } from '@/types';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { TranscriptPanel } from '@/components/MeetingDetails/TranscriptPanel';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { MeetingDetailsSplitView, type MeetingDetailsTab } from '@/components/MeetingDetails/MeetingDetailsSplitView';
import { ModelConfig } from '@/components/ModelSettingsModal';

// Custom hooks
import { useMeetingData } from '@/hooks/meeting-details/useMeetingData';
import { useSummaryGeneration } from '@/hooks/meeting-details/useSummaryGeneration';
import { useTemplates } from '@/hooks/meeting-details/useTemplates';
import { useSpeakerSuggestions } from '@/hooks/meeting-details/useSpeakerSuggestions';
import { SpeakerSuggestions } from '@/components/MeetingDetails/SpeakerSuggestions';
import { useCopyOperations } from '@/hooks/meeting-details/useCopyOperations';
import { useMeetingOperations } from '@/hooks/meeting-details/useMeetingOperations';
import { useConfig } from '@/contexts/ConfigContext';
import { MoreHorizontal, Tags } from 'lucide-react';
import { TopBar } from '@/components/Shell/TopBar';
import { InlineTitle, type InlineTitleHandle } from '@/components/Shell/InlineTitle';
import { IconButton } from '@/components/ui/icon-button';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { MeetingChips } from '@/components/Organize/MeetingChips';
import { AutopilotPanel } from '@/components/Autopilot/AutopilotPanel';
import { MeetingMenuItems, useMeetingActions } from '@/components/Organize/MeetingActions';
import { formatDuration, formatMeetingWhen, plainMeetingOrg } from '@/lib/organize';

export default function PageContent({
  meeting,
  summaryData,
  initialSummary,
  shouldAutoGenerate = false,
  onAutoGenerateComplete,
  onMeetingUpdated,
  onRefetchTranscripts,
  // Pagination props for efficient transcript loading
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
}: {
  meeting: any;
  summaryData: MeetingSummary | null;
  initialSummary: SummaryProcessResponse | null;
  shouldAutoGenerate?: boolean;
  onAutoGenerateComplete?: () => void;
  onMeetingUpdated?: () => Promise<void>;
  onRefetchTranscripts?: () => Promise<void>;
  // Pagination props
  segments?: any[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;
}) {
  console.log('📄 PAGE CONTENT: Initializing with data:', {
    meetingId: meeting.id,
    summaryDataKeys: summaryData ? Object.keys(summaryData) : null,
    transcriptsCount: meeting.transcripts?.length
  });

  // State
  const [customPrompt, setCustomPrompt] = useState<string>('');
  const isRecording = false;
  const [activeTab, setActiveTab] = useState<MeetingDetailsTab>('transcript');

  // Ref to store the modal open function from SummaryGeneratorButtonGroup
  const openModelSettingsRef = useRef<(() => void) | null>(null);
  const autoSwitchedSummaryMeetingIdsRef = useRef(new Set<string>());
  const manuallySelectedTabMeetingIdsRef = useRef(new Set<string>());
  const autoGenerationStartedMeetingIdRef = useRef<string | null>(null);

  // Sidebar context
  const { serverAddress, organizedMeetings, organizeAvailable } = useSidebar();
  const { actions: meetingActions, dialogs: meetingDialogs } = useMeetingActions();
  const titleRef = useRef<InlineTitleHandle>(null);

  // Get model config from ConfigContext
  const { modelConfig, setModelConfig, isModelConfigLoading } = useConfig();

  // Custom hooks
  const meetingData = useMeetingData({ meeting, summaryData, onMeetingUpdated });
  const templates = useTemplates({ meetingId: meeting.id, transcripts: meetingData.transcripts });

  const renameSpeaker = async (speakerKey: string, displayName: string) => {
    await invoke('rename_speaker', {
      meetingId: meeting.id,
      speakerKey,
      displayName,
    });
    await onRefetchTranscripts?.();
  };
  const speakerSuggestions = useSpeakerSuggestions(meeting.id, meetingData.transcripts.length, renameSpeaker);

  // Callback to register the modal open function
  const handleRegisterModalOpen = (openFn: () => void) => {
    console.log('📝 Registering modal open function in PageContent');
    openModelSettingsRef.current = openFn;
  };

  // Callback to trigger modal open (called from error handler)
  const handleOpenModelSettings = () => {
    console.log('🔔 Opening model settings from PageContent');
    if (openModelSettingsRef.current) {
      openModelSettingsRef.current();
    } else {
      console.warn('⚠️ Modal open function not yet registered');
    }
  };

  // Save model config to backend database and sync via event
  const handleSaveModelConfig = async (config?: ModelConfig) => {
    if (!config) return;
    try {
      await invoke('api_save_model_config', {
        provider: config.provider,
        model: config.model,
        whisperModel: config.whisperModel,
        apiKey: config.apiKey ?? null,
        ollamaEndpoint: config.ollamaEndpoint ?? null,
      });

      // Emit event so ConfigContext and other listeners stay in sync
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', config);

      toast.success('Configurações do modelo salvas');
    } catch (error) {
      console.error('Failed to save model config:', error);
      toast.error('Não foi possível salvar as configurações do modelo');
    }
  };

  const summaryGeneration = useSummaryGeneration({
    initialSummary,
    meeting,
    transcripts: meetingData.transcripts,
    modelConfig: modelConfig,
    isModelConfigLoading,
    selectedTemplate: templates.selectedTemplate,
    onMeetingUpdated,
    updateMeetingTitle: meetingData.updateMeetingTitle,
    setAiSummary: meetingData.setAiSummary,
    onOpenModelSettings: handleOpenModelSettings,
  });

  const copyOperations = useCopyOperations({
    meeting,
    transcripts: meetingData.transcripts,
    meetingTitle: meetingData.meetingTitle,
    aiSummary: meetingData.aiSummary,
    blockNoteSummaryRef: meetingData.blockNoteSummaryRef,
  });

  const meetingOperations = useMeetingOperations({
    meeting,
  });

  // Track page view
  useEffect(() => {
    Analytics.trackPageView('meeting_details');
  }, []);

  // Linha organizada desta reunião (cliente, categoria, fixada); sem o recurso, só id e título
  const organizedRow = organizedMeetings.find((item) => item.id === meeting.id);
  const meetingRow = {
    ...(organizedRow ?? plainMeetingOrg(meeting.id, meetingData.meetingTitle)),
    title: meetingData.meetingTitle,
    createdAt: organizedRow?.createdAt || meeting.created_at || '',
  };

  // Um título novo vindo da barra lateral ou do título automático aparece aqui também
  const sidebarTitle = organizedRow?.title;
  const setPageTitle = meetingData.setMeetingTitle;
  const lastSidebarTitleRef = useRef(sidebarTitle);
  useEffect(() => {
    // Só reage a mudanças depois de abrir a página: o título recém-carregado do banco vale mais que a lista
    if (sidebarTitle && lastSidebarTitleRef.current !== undefined && sidebarTitle !== lastSidebarTitleRef.current) {
      setPageTitle(sidebarTitle);
    }
    lastSidebarTitleRef.current = sidebarTitle;
  }, [sidebarTitle, setPageTitle]);

  const meetingMeta = [formatMeetingWhen(meetingRow.createdAt), formatDuration(meetingRow.durationSeconds)]
    .filter(Boolean)
    .join(' · ');

  useEffect(() => {
    if (
      (meetingData.aiSummary || summaryGeneration.summaryStatus === 'completed')
      && !autoSwitchedSummaryMeetingIdsRef.current.has(meeting.id)
      && !manuallySelectedTabMeetingIdsRef.current.has(meeting.id)
    ) {
      autoSwitchedSummaryMeetingIdsRef.current.add(meeting.id);
      setActiveTab('summary');
    }
  }, [meeting.id, meetingData.aiSummary, summaryGeneration.summaryStatus]);

  // Auto-generate only after the model configuration has settled.
  useEffect(() => {
    if (
      !shouldAutoGenerate
      || summaryGeneration.summaryStatus !== 'idle'
      || isModelConfigLoading
      || meetingData.transcripts.length === 0
      || autoGenerationStartedMeetingIdRef.current === meeting.id
    ) {
      return;
    }

    autoGenerationStartedMeetingIdRef.current = meeting.id;
    console.log(`🤖 Auto-generating summary with ${modelConfig.provider}/${modelConfig.model}...`);
    onAutoGenerateComplete?.();
    void summaryGeneration.handleGenerateSummary('');
  }, [
    shouldAutoGenerate,
    meeting.id,
    meetingData.transcripts.length,
    isModelConfigLoading,
    modelConfig.provider,
    modelConfig.model,
    summaryGeneration.handleGenerateSummary,
    summaryGeneration.summaryStatus,
    onAutoGenerateComplete,
  ]);

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.14, ease: 'easeOut' }}
      className="flex h-full min-w-0 flex-col bg-background"
    >
      <TopBar
        title={
          <InlineTitle
            ref={titleRef}
            value={meetingData.meetingTitle}
            onSave={async (title) => {
              const saved = await meetingActions.saveTitle(meeting.id, title, 'meeting_details');
              if (saved) meetingData.setMeetingTitle(title);
              return saved;
            }}
          />
        }
        chips={
          <>
            <MeetingChips meeting={meetingRow} onClick={organizeAvailable ? () => meetingActions.classify(meetingRow) : undefined} />
            {meetingMeta && <span className="hidden truncate text-xs text-muted-foreground lg:inline">{meetingMeta}</span>}
          </>
        }
        actions={
          <>
            {organizeAvailable && !meetingRow.client && !meetingRow.category && (
              <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={() => meetingActions.classify(meetingRow)}>
                <Tags />
                Classificar
              </Button>
            )}
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <IconButton label="Mais ações">
                  <MoreHorizontal />
                </IconButton>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-56">
                <MeetingMenuItems
                  meeting={meetingRow}
                  actions={meetingActions}
                  organizeAvailable={organizeAvailable}
                  Item={DropdownMenuItem}
                  Separator={DropdownMenuSeparator}
                  onRename={() => setTimeout(() => titleRef.current?.startEditing(), 0)}
                />
              </DropdownMenuContent>
            </DropdownMenu>
          </>
        }
      />
      {meetingDialogs}
      <AutopilotPanel meetingId={meeting.id} onMeetingUpdated={onMeetingUpdated} />
      <SpeakerSuggestions
        suggestions={speakerSuggestions.suggestions}
        onConfirm={speakerSuggestions.confirm}
        onDismiss={speakerSuggestions.dismiss}
        onDismissAll={speakerSuggestions.dismissAll}
      />
      <div className="flex flex-1 min-w-0 overflow-hidden">
        <MeetingDetailsSplitView
          activeTab={activeTab}
          onTabChange={(tab) => {
            manuallySelectedTabMeetingIdsRef.current.add(meeting.id);
            setActiveTab(tab);
          }}
          transcript={
            <TranscriptPanel
              transcripts={meetingData.transcripts}
              customPrompt={customPrompt}
              onPromptChange={setCustomPrompt}
              onCopyTranscript={copyOperations.handleCopyTranscript}
              onOpenMeetingFolder={meetingOperations.handleOpenMeetingFolder}
              isRecording={isRecording}
              disableAutoScroll={true}
              usePagination={true}
              segments={segments}
              hasMore={hasMore}
              isLoadingMore={isLoadingMore}
              totalCount={totalCount}
              loadedCount={loadedCount}
              onLoadMore={onLoadMore}
              meetingId={meeting.id}
              meetingFolderPath={meeting.folder_path}
              onRefetchTranscripts={onRefetchTranscripts}
              onRenameSpeaker={renameSpeaker}
            />
          }
          summary={
            <SummaryPanel
              meeting={meeting}
              meetingTitle={meetingData.meetingTitle}
              summaryRef={meetingData.blockNoteSummaryRef}
              isSaving={meetingData.isSaving}
              isSummaryDirty={meetingData.isSummaryDirty}
              onSaveAll={meetingData.saveAllChanges}
              onCopySummary={copyOperations.handleCopySummary}
              aiSummary={meetingData.aiSummary}
              summaryStatus={summaryGeneration.summaryStatus}
              transcripts={meetingData.transcripts}
              modelConfig={modelConfig}
              setModelConfig={setModelConfig}
              onSaveModelConfig={handleSaveModelConfig}
              onGenerateSummary={summaryGeneration.handleGenerateSummary}
              onStopGeneration={summaryGeneration.handleStopGeneration}
              customPrompt={customPrompt}
              onSaveSummary={meetingData.handleSaveSummary}
              onSummaryChange={meetingData.handleSummaryChange}
              onDirtyChange={meetingData.setIsSummaryDirty}
              summaryError={summaryGeneration.summaryError}
              onRegenerateSummary={summaryGeneration.handleRegenerateSummary}
              getSummaryStatusMessage={summaryGeneration.getSummaryStatusMessage}
              availableTemplates={templates.availableTemplates}
              selectedTemplate={templates.selectedTemplate}
              onTemplateSelect={templates.handleTemplateSelection}
              autoTemplate={templates.autoTemplate}
              onAutoTemplateChange={templates.setAutoTemplate}
              isModelConfigLoading={isModelConfigLoading}
              onOpenModelSettings={handleRegisterModalOpen}
            />
          }
        />
      </div>
    </motion.div>
  );
}
