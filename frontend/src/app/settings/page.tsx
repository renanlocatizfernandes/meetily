'use client';

import React, { useState, useEffect } from 'react';
import {
  Settings2,
  Mic,
  Database as DatabaseIcon,
  SparkleIcon,
  FlaskConical,
  HelpCircle,
  FolderTree,
  Info,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { TranscriptSettings } from '@/components/TranscriptSettings';
import { RecordingSettings } from '@/components/RecordingSettings';
import { AutoRecordSettings } from '@/components/AutoRecord/AutoRecordSettings';
import { MeetingEndSettings } from '@/components/AutoRecord/MeetingEndSettings';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { SummaryModelSettings } from '@/components/SummaryModelSettings';
import { BetaSettings } from '@/components/BetaSettings';
import { LiveAssistSettings } from '@/components/LiveAssist/LiveAssistSettings';
import { RecursosSettings } from '@/components/Recursos/RecursosBar';
import { OrganizationSettings } from '@/components/Organize/OrganizationSettings';
import { ModelAdvisorCard } from '@/components/ModelAdvisor';
import { About } from '@/components/About';
import { useConfig } from '@/contexts/ConfigContext';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { TopBar } from '@/components/Shell/TopBar';

// Seções das configurações (os valores das abas existentes foram mantidos)
const TABS = [
  { value: 'general', label: 'Geral', description: 'Notificações, armazenamento e privacidade.', icon: Settings2 },
  { value: 'recording', label: 'Gravação', description: 'Dispositivos, arquivos de áudio e gravação automática.', icon: Mic },
  { value: 'Transcriptionmodels', label: 'Transcrição', description: 'Modelo e idioma da transcrição.', icon: DatabaseIcon },
  { value: 'summaryModels', label: 'Resumo', description: 'Modelo, idioma e geração automática do resumo.', icon: SparkleIcon },
  { value: 'organization', label: 'Organização', description: 'Títulos automáticos, clientes e categorias.', icon: FolderTree },
  { value: 'liveAssist', label: 'Ajuda ao vivo', description: 'Sugestões durante a reunião e recursos.', icon: HelpCircle },
  { value: 'beta', label: 'Beta', description: 'Recursos em teste.', icon: FlaskConical },
  { value: 'about', label: 'Sobre', description: 'Versão e atualizações.', icon: Info },
] as const;

type TabValue = (typeof TABS)[number]['value'];

const ACTIVE_TAB_KEY = 'meetily.settings.activeTab';

export default function SettingsPage() {
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();
  const [activeTab, setActiveTab] = useState<TabValue>('general');

  // Volta para a última seção aberta nesta sessão
  useEffect(() => {
    try {
      const stored = sessionStorage.getItem(ACTIVE_TAB_KEY);
      if (stored && TABS.some((tab) => tab.value === stored)) setActiveTab(stored as TabValue);
    } catch {
      // Sem armazenamento, abre em Geral.
    }
  }, []);

  const handleTabChange = (value: string) => {
    setActiveTab(value as TabValue);
    try {
      sessionStorage.setItem(ACTIVE_TAB_KEY, value);
    } catch {
      // Preferência opcional.
    }
  };

  // Load saved transcript configuration on mount
  useEffect(() => {
    const loadTranscriptConfig = async () => {
      try {
        const config = await invoke('api_get_transcript_config') as any;
        if (config) {
          console.log('Loaded saved transcript config:', config);
          setTranscriptModelConfig({
            provider: config.provider || 'localWhisper',
            model: config.model || 'large-v3',
            apiKey: config.apiKey || null
          });
        }
      } catch (error) {
        console.error('Failed to load transcript config:', error);
      }
    };
    loadTranscriptConfig();
  }, [setTranscriptModelConfig]);

  // Link direto para uma seção, por exemplo /settings?tab=liveAssist
  useEffect(() => {
    const tab = new URLSearchParams(window.location.search).get('tab');
    if (tab && TABS.some((item) => item.value === tab)) setActiveTab(tab as TabValue);
  }, []);

  const current = TABS.find((tab) => tab.value === activeTab) ?? TABS[0];

  return (
    <div className="flex h-full flex-col bg-background">
      <TopBar title={<span className="block truncate px-2">Configurações</span>} />

      <Tabs
        value={activeTab}
        onValueChange={handleTabChange}
        orientation="vertical"
        className="flex min-h-0 flex-1"
      >
        {/* Coluna esquerda: lista de seções */}
        <TabsList
          aria-label="Seções das configurações"
          className="flex h-auto w-[208px] shrink-0 flex-col items-stretch justify-start gap-0.5 overflow-y-auto rounded-none border-r bg-transparent p-2 custom-scrollbar"
        >
          {TABS.map((tab) => {
            const Icon = tab.icon;
            return (
              <TabsTrigger
                key={tab.value}
                value={tab.value}
                className="h-8 justify-start gap-2.5 rounded-lg px-2.5 text-[13px] font-normal text-muted-foreground shadow-none transition-colors duration-150 hover:bg-accent/70 hover:text-foreground data-[state=active]:bg-accent data-[state=active]:font-medium data-[state=active]:text-foreground data-[state=active]:shadow-none"
              >
                <Icon className="h-4 w-4 shrink-0" />
                {tab.label}
              </TabsTrigger>
            );
          })}
        </TabsList>

        {/* Coluna direita: conteúdo da seção */}
        <div className="min-w-0 flex-1 overflow-y-auto custom-scrollbar">
          <div className="mx-auto w-full max-w-[820px] px-8 py-6">
            <header className="mb-5">
              <h1 className="text-lg font-semibold tracking-tight">{current.label}</h1>
              <p className="mt-0.5 text-sm text-muted-foreground">{current.description}</p>
            </header>

            <TabsContent value="general" className="mt-0">
              <PreferenceSettings />
            </TabsContent>
            <TabsContent value="recording" className="mt-0">
              <RecordingSettings />
              <AutoRecordSettings />
              <MeetingEndSettings />
            </TabsContent>
            <TabsContent value="Transcriptionmodels" className="mt-0">
              <ModelAdvisorCard roles={['speech', 'voices']} />
              <TranscriptSettings
                transcriptModelConfig={transcriptModelConfig}
                setTranscriptModelConfig={setTranscriptModelConfig}
              />
            </TabsContent>
            <TabsContent value="summaryModels" className="mt-0">
              <ModelAdvisorCard roles={['assistant']} />
              <SummaryModelSettings />
            </TabsContent>
            <TabsContent value="organization" className="mt-0">
              <OrganizationSettings />
            </TabsContent>
            <TabsContent value="liveAssist" className="mt-0">
              <LiveAssistSettings />
              <RecursosSettings />
            </TabsContent>
            <TabsContent value="beta" className="mt-0">
              <BetaSettings />
            </TabsContent>
            <TabsContent value="about" className="mt-0">
              <div className="rounded-xl border bg-card p-5">
                <About />
              </div>
            </TabsContent>
          </div>
        </div>
      </Tabs>
    </div>
  );
}
