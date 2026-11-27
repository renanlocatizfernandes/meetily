import { DeviceSelection } from "@/components/DeviceSelection";
import { cachedDeviceLabel } from "@/lib/audioDevices";
import { LanguageSelection } from "@/components/LanguageSelection";
import { TranscriptSettings } from "@/components/TranscriptSettings";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { toast } from "sonner";
import { useConfig } from "@/contexts/ConfigContext";
import { useRecordingState } from "@/contexts/RecordingStateContext";

type modalType = "modelSettings" | "deviceSettings" | "languageSettings" | "modelSelector" | "errorAlert" | "chunkDropWarning";

/**
 * SettingsModals Component
 *
 * All settings modals consolidated into a single component.
 * Uses ConfigContext and RecordingStateContext internally - no prop drilling needed!
 */

interface SettingsModalsProps {
  modals: {
    modelSettings: boolean;
    deviceSettings: boolean;
    languageSettings: boolean;
    modelSelector: boolean;
    errorAlert: boolean;
    chunkDropWarning: boolean;
  };
  messages: {
    errorAlert: string;
    chunkDropWarning: string;
    modelSelector: string;
  };
  onClose: (name: modalType) => void;
}

export function SettingsModals({
  modals,
  messages,
  onClose,
}: SettingsModalsProps) {
  // Contexts
  const {
    selectedDevices,
    setSelectedDevices,
    selectedLanguage,
    setSelectedLanguage,
    transcriptModelConfig,
    setTranscriptModelConfig,
    showConfidenceIndicator,
    toggleConfidenceIndicator,
  } = useConfig();

  const { isRecording } = useRecordingState();

  return <>
    {/* Device Settings Modal */}
    {modals.deviceSettings && (
      <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
        <div className="bg-card rounded-lg p-6 max-w-md w-full mx-4 shadow-md">
          <div className="flex justify-between items-center mb-4">
            <h3 className="text-lg font-semibold text-foreground">Dispositivos de áudio</h3>
            <button
              onClick={() => onClose('deviceSettings')}
              className="text-muted-foreground hover:text-foreground"
            >
              <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <DeviceSelection
            selectedDevices={selectedDevices}
            onDeviceChange={setSelectedDevices}
            disabled={isRecording}
          />

          <div className="mt-6 flex justify-end">
            <button
              onClick={() => {
                const micDevice = selectedDevices.micDevice ? cachedDeviceLabel(selectedDevices.micDevice) : 'Padrão';
                const systemDevice = selectedDevices.systemDevice ? cachedDeviceLabel(selectedDevices.systemDevice) : 'Padrão';
                toast.success("Dispositivos selecionados", {
                  description: `Microfone: ${micDevice}, áudio do sistema: ${systemDevice}`
                });
                onClose('deviceSettings');
              }}
              className="px-4 py-2 text-sm font-medium text-white bg-primary rounded-md hover:bg-primary/90 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-ring"
            >
              Concluir
            </button>
          </div>
        </div>
      </div>
    )}

    {/* Language Settings Modal */}
    {modals.languageSettings && (
      <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
        <div className="bg-card rounded-lg p-6 max-w-md w-full mx-4 shadow-md">
          <div className="flex justify-between items-center mb-4">
            <h3 className="text-lg font-semibold text-foreground">Idioma</h3>
            <button
              onClick={() => onClose('languageSettings')}
              className="text-muted-foreground hover:text-foreground"
            >
              <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <LanguageSelection
            selectedLanguage={selectedLanguage}
            onLanguageChange={setSelectedLanguage}
            disabled={isRecording}
            provider={transcriptModelConfig.provider}
          />

          <div className="mt-6 flex justify-end">
            <button
              onClick={() => onClose('languageSettings')}
              className="px-4 py-2 text-sm font-medium text-white bg-primary rounded-md hover:bg-primary/90 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-ring"
            >
              Concluir
            </button>
          </div>
        </div>
      </div>
    )}

    {/* Model Selection Modal */}
    {modals.modelSelector && (
      <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
        <div className="bg-card rounded-lg max-w-4xl w-full mx-4 shadow-md max-h-[90vh] flex flex-col">
          {/* Fixed Header */}
          <div className="flex justify-between items-center p-6 pb-4 border-b border-border">
            <h3 className="text-lg font-semibold text-foreground">
              {messages.modelSelector ? 'Configure o reconhecimento de fala' : 'Modelo de transcrição'}
            </h3>
            <button
              onClick={() => onClose('modelSelector')}
              className="text-muted-foreground hover:text-foreground"
            >
              <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          {/* Scrollable Content */}
          <div className="flex-1 overflow-y-auto p-6 pt-4">
            <TranscriptSettings
              transcriptModelConfig={transcriptModelConfig}
              setTranscriptModelConfig={setTranscriptModelConfig}
              onModelSelect={() => onClose('modelSelector')}
              card={false}
            />
          </div>

          {/* Fixed Footer */}
          <div className="p-6 pt-4 border-t border-border flex items-center justify-between">
            {/* Confidence Indicator Toggle */}
            <div className="flex items-center gap-3">
              <label className="relative inline-flex items-center cursor-pointer">
                <input
                  type="checkbox"
                  checked={showConfidenceIndicator}
                  onChange={(e) => toggleConfidenceIndicator(e.target.checked)}
                  className="sr-only peer"
                />
                <div className="w-11 h-6 bg-accent peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-ring rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-card after:border-border after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary"></div>
              </label>
              <div>
                <p className="text-sm font-medium text-foreground">Mostrar indicadores de confiança</p>
                <p className="text-xs text-muted-foreground">Mostra pontos coloridos com a confiança da transcrição</p>
              </div>
            </div>

            <button
              onClick={() => onClose('modelSelector')}
              className="px-4 py-2 text-sm font-medium text-foreground bg-muted rounded-md hover:bg-accent focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-ring"
            >
              {messages.modelSelector ? 'Cancelar' : 'Concluir'}
            </button>
          </div>
        </div>
      </div>
    )}

    {/* Error Alert Modal */}
    {modals.errorAlert && (
      <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
        <Alert className="max-w-md mx-4 border-danger/30 bg-card shadow-md">
          <AlertTitle className="text-danger">Gravação interrompida</AlertTitle>
          <AlertDescription className="text-danger">
            {messages.errorAlert}
            <button
              onClick={() => onClose('errorAlert')}
              className="ml-2 text-danger hover:text-danger underline"
            >
              Fechar
            </button>
          </AlertDescription>
        </Alert>
      </div>
    )}

    {/* Chunk Drop Warning Modal */}
    {modals.chunkDropWarning && (
      <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
        <Alert className="max-w-lg mx-4 border-warning/30 bg-card shadow-md">
          <AlertTitle className="text-warning">Aviso de desempenho da transcrição</AlertTitle>
          <AlertDescription className="text-warning">
            {messages.chunkDropWarning}
            <button
              onClick={() => onClose('chunkDropWarning')}
              className="ml-2 text-warning hover:text-warning underline"
            >
              Fechar
            </button>
          </AlertDescription>
        </Alert>
      </div>
    )}
  </>
}
