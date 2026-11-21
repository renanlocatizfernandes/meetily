import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';
import type { Transcript } from '@/types';
import {
  DEFAULT_TEMPLATE_ID,
  DETECTION_MIN_CHARS,
  buildDetectionText,
  readAutoTemplate,
  shouldApplySuggestion,
  writeAutoTemplate,
  type TemplateSuggestion,
} from '@/lib/template-detection';

interface UseTemplatesOptions {
  meetingId?: string;
  transcripts?: Transcript[];
}

export function useTemplates({ meetingId, transcripts }: UseTemplatesOptions = {}) {
  const [availableTemplates, setAvailableTemplates] = useState<Array<{
    id: string;
    name: string;
    description: string;
  }>>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string>(DEFAULT_TEMPLATE_ID);
  const [autoTemplate, setAutoTemplateState] = useState<boolean>(true);
  // Meetings where the person picked a template by hand: detection never overrides those.
  const manualMeetingsRef = useRef(new Set<string>());
  const detectedMeetingsRef = useRef(new Set<string>());
  const transcriptsRef = useRef(transcripts);
  // Read after an await: a state updater is not guaranteed to run before the next line.
  const selectedTemplateRef = useRef(selectedTemplate);
  selectedTemplateRef.current = selectedTemplate;
  transcriptsRef.current = transcripts;
  const hasTranscript = (transcripts?.length ?? 0) > 0;

  useEffect(() => {
    setAutoTemplateState(readAutoTemplate());
  }, []);

  // Fetch available templates on mount
  useEffect(() => {
    const fetchTemplates = async () => {
      try {
        const templates = await invokeTauri('api_list_templates') as Array<{
          id: string;
          name: string;
          description: string;
        }>;
        setAvailableTemplates(templates);
      } catch (error) {
        console.error('Failed to fetch templates:', error);
      }
    };
    fetchTemplates();
  }, []);

  // A new meeting starts from the default again.
  useEffect(() => {
    setSelectedTemplate(DEFAULT_TEMPLATE_ID);
  }, [meetingId]);

  // Suggest a template from the transcript, once per meeting, only when the person has not chosen.
  useEffect(() => {
    if (!autoTemplate || !meetingId || !hasTranscript || availableTemplates.length === 0) return;
    if (detectedMeetingsRef.current.has(meetingId) || manualMeetingsRef.current.has(meetingId)) return;
    const text = buildDetectionText(transcriptsRef.current ?? []);
    if (text.length < DETECTION_MIN_CHARS) return;
    detectedMeetingsRef.current.add(meetingId);
    let cancelled = false;
    (async () => {
      try {
        const suggestion = await invokeTauri('api_detect_meeting_template', { text }) as TemplateSuggestion;
        if (cancelled || manualMeetingsRef.current.has(meetingId)) return;
        const ids = availableTemplates.map((template) => template.id);
        if (!shouldApplySuggestion(suggestion, ids, selectedTemplateRef.current)) return;
        selectedTemplateRef.current = suggestion.template_id;
        setSelectedTemplate(suggestion.template_id);
        toast.info(`Modelo sugerido: ${suggestion.template_name}`, {
          description: 'Escolhido pelo conteúdo da conversa. Você pode trocar em "Modelo de resumo".',
        });
        Analytics.trackFeatureUsed('template_auto_selected');
      } catch (error) {
        // Detection is a convenience: a failure keeps the current template.
        console.error('Template detection failed:', error);
        detectedMeetingsRef.current.delete(meetingId);
      }
    })();
    return () => {
      cancelled = true;
      detectedMeetingsRef.current.delete(meetingId);
    };
  }, [autoTemplate, meetingId, hasTranscript, availableTemplates]);

  // Handle template selection
  const handleTemplateSelection = useCallback((templateId: string, templateName: string) => {
    if (meetingId) manualMeetingsRef.current.add(meetingId);
    setSelectedTemplate(templateId);
    toast.success('Modelo de resumo selecionado', {
      description: `O resumo será gerado com o modelo "${templateName}"`,
    });
    Analytics.trackFeatureUsed('template_selected');
  }, [meetingId]);

  const setAutoTemplate = useCallback((enabled: boolean) => {
    writeAutoTemplate(enabled);
    setAutoTemplateState(enabled);
  }, []);

  return {
    availableTemplates,
    selectedTemplate,
    handleTemplateSelection,
    autoTemplate,
    setAutoTemplate,
  };
}
