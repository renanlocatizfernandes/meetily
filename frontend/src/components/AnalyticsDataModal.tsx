'use client';

import React from 'react';
import { X, Info, Shield } from 'lucide-react';

interface AnalyticsDataModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirmDisable: () => void;
}

export default function AnalyticsDataModal({ isOpen, onClose, onConfirmDisable }: AnalyticsDataModalProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50">
      <div className="bg-card rounded-lg shadow-md max-w-2xl w-full mx-4 max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-border">
          <div className="flex items-center gap-3">
            <Shield className="w-6 h-6 text-brand" />
            <h2 className="text-xl font-semibold text-foreground">O que os dados de uso coletam</h2>
          </div>
          <button
            onClick={onClose}
            className="text-muted-foreground/80 hover:text-muted-foreground transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-6 space-y-6">
          {/* Privacy Notice */}
          <div className="bg-success-soft border border-success/30 rounded-lg p-4">
            <div className="flex items-start gap-3">
              <Info className="w-5 h-5 text-success mt-0.5 flex-shrink-0" />
              <div className="text-sm text-success">
                <p className="font-semibold mb-1">Sua privacidade está protegida</p>
                <p>A coleta vem desativada. Se você ativar, coletamos <strong>somente dados de uso anônimos</strong>. Nenhum conteúdo de reunião, nome, caminho de arquivo ou informação pessoal é coletado.</p>
              </div>
            </div>
          </div>

          {/* Data Categories */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold text-foreground">Dados coletados quando ativado:</h3>

            {/* Model Preferences */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">1. Preferências de modelo</h4>
              <ul className="text-sm text-foreground space-y-1 ml-4">
                <li>• Modelo de transcrição (ex.: "Whisper large-v3", "Parakeet")</li>
                <li>• Modelo de resumo (ex.: "Llama 3.2", "Claude Sonnet")</li>
                <li>• Provedor do modelo (ex.: "Local", "Ollama", "OpenRouter")</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">Ajuda a entender quais modelos são preferidos</p>
            </div>

            {/* Meeting Metrics */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">2. Métricas anônimas de reunião</h4>
              <ul className="text-sm text-foreground space-y-1 ml-4">
                <li>• Duração da gravação (ex.: "125 segundos")</li>
                <li>• Duração das pausas (ex.: "5 segundos")</li>
                <li>• Número de trechos de transcrição</li>
                <li>• Número de trechos de áudio processados</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">Ajuda a otimizar o desempenho e entender padrões de uso</p>
            </div>

            {/* Device Types */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">3. Tipos de dispositivo (não os nomes)</h4>
              <ul className="text-sm text-foreground space-y-1 ml-4">
                <li>• Tipo do microfone: "Bluetooth", "Wired" (com fio) ou "Unknown" (desconhecido)</li>
                <li>• Tipo do áudio do sistema: "Bluetooth", "Wired" (com fio) ou "Unknown" (desconhecido)</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">Ajuda a melhorar a compatibilidade. Os nomes dos dispositivos NÃO são coletados</p>
            </div>

            {/* Usage Patterns */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">4. Padrões de uso do app</h4>
              <ul className="text-sm text-foreground space-y-1 ml-4">
                <li>• Eventos de abertura e fechamento do app</li>
                <li>• Duração da sessão</li>
                <li>• Uso de recursos (ex.: "settings changed")</li>
                <li>• Ocorrências de erro (ajuda a corrigir falhas)</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">Ajuda a melhorar a experiência de uso</p>
            </div>

            {/* Platform Info */}
            <div className="border border-border rounded-lg p-4">
              <h4 className="font-semibold text-foreground mb-2">5. Informações da plataforma</h4>
              <ul className="text-sm text-foreground space-y-1 ml-4">
                <li>• Sistema operacional (ex.: "macOS", "Windows")</li>
                <li>• Versão do app (incluída automaticamente em todos os eventos)</li>
                <li>• Arquitetura (ex.: "x86_64", "aarch64")</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-2 italic">Ajuda a priorizar o suporte a plataformas</p>
            </div>
          </div>

          {/* What We DON'T Collect */}
          <div className="bg-danger-soft border border-danger/30 rounded-lg p-4">
            <h4 className="font-semibold text-danger mb-2">O que NÃO é coletado:</h4>
            <ul className="text-sm text-danger space-y-1 ml-4">
              <li>• ❌ Nomes ou títulos de reuniões</li>
              <li>• ❌ Nomes de arquivo, caminhos de arquivo ou pastas de reunião</li>
              <li>• ❌ Transcrições ou conteúdo de reuniões</li>
              <li>• ❌ Gravações de áudio</li>
              <li>• ❌ Nomes de dispositivo (só os tipos: Bluetooth ou com fio)</li>
              <li>• ❌ Informações pessoais</li>
              <li>• ❌ Qualquer dado que identifique você</li>
            </ul>
          </div>

          {/* Example Event */}
          <div className="bg-muted border border-border rounded-lg p-4">
            <h4 className="font-semibold text-foreground mb-2">Exemplo de evento:</h4>
            <pre className="text-xs text-foreground overflow-x-auto">
              {`{
  "event": "meeting_ended",
  "app_version": "0.4.1",
  "transcription_provider": "parakeet",
  "transcription_model": "parakeet-tdt-0.6b-v3-int8",
  "summary_provider": "ollama",
  "summary_model": "llama3.2:latest",
  "total_duration_seconds": "125.5",
  "microphone_device_type": "Wired",
  "system_audio_device_type": "Bluetooth",
  "chunks_processed": "150",
  "had_fatal_error": "false"
}`}
            </pre>
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between gap-4 p-6 border-t border-border bg-muted">
          <button
            onClick={onClose}
            className="px-4 py-2 text-foreground bg-card border border-border rounded-md hover:bg-muted transition-colors"
          >
            Manter ativado
          </button>
          <button
            onClick={onConfirmDisable}
            className="px-4 py-2 text-white bg-red-600 rounded-md hover:bg-red-700 transition-colors"
          >
            Confirmar: desativar dados de uso
          </button>
        </div>
      </div>
    </div>
  );
}
