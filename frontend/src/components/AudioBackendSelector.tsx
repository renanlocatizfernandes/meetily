import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Info } from 'lucide-react';

export interface BackendInfo {
  id: string;
  name: string;
  description: string;
}

interface AudioBackendSelectorProps {
  currentBackend?: string;
  onBackendChange?: (backend: string) => void;
  disabled?: boolean;
}

export function AudioBackendSelector({
  currentBackend: propBackend,
  onBackendChange,
  disabled = false,
}: AudioBackendSelectorProps) {
  const [backends, setBackends] = useState<BackendInfo[]>([]);
  const [currentBackend, setCurrentBackend] = useState<string>('coreaudio');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showTooltip, setShowTooltip] = useState(false);

  // Load available backends and current selection
  useEffect(() => {
    const loadBackends = async () => {
      try {
        setLoading(true);
        setError(null);

        // Get backend info (includes name and description)
        const backendInfo = await invoke<BackendInfo[]>('get_audio_backend_info');
        setBackends(backendInfo);

        // Get current backend if not provided via props
        if (!propBackend) {
          const current = await invoke<string>('get_current_audio_backend');
          setCurrentBackend(current);
        } else {
          setCurrentBackend(propBackend);
        }
      } catch (err) {
        console.error('Failed to load audio backends:', err);
        setError('Não foi possível carregar as opções de captura');
      } finally {
        setLoading(false);
      }
    };

    loadBackends();
  }, [propBackend]);

  // Handle backend selection
  const handleBackendChange = async (backendId: string) => {
    try {
      setError(null);
      await invoke('set_audio_backend', { backend: backendId });
      setCurrentBackend(backendId);

      // Notify parent component
      if (onBackendChange) {
        onBackendChange(backendId);
      }

      console.log(`Audio backend changed to: ${backendId}`);
    } catch (err) {
      console.error('Failed to set audio backend:', err);
      setError('Não foi possível trocar o método de captura. Tente de novo.');
    }
  };

  // Only show selector if there are multiple backends
  if (loading) {
    return (
      <div className="animate-pulse">
        <div className="h-4 bg-accent rounded w-32 mb-2"></div>
        <div className="h-10 bg-accent rounded"></div>
      </div>
    );
  }

  // Hide if only one backend available
  if (backends.length <= 1) {
    return null;
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <label className="text-sm font-medium text-foreground">
          Método de captura do áudio do sistema
        </label>
        <div className="relative">
          <button
            type="button"
            onMouseEnter={() => setShowTooltip(true)}
            onMouseLeave={() => setShowTooltip(false)}
            className="text-muted-foreground/80 hover:text-muted-foreground transition-colors"
          >
            <Info className="h-4 w-4" />
          </button>
          {showTooltip && (
            <div className="absolute z-10 left-6 top-0 w-64 p-3 text-xs bg-primary text-white rounded-lg shadow-md">
              <p className="font-semibold mb-1">Métodos de captura de áudio:</p>
              <ul className="space-y-1">
                {backends.map((backend) => (
                  <li key={backend.id}>
                    <span className="font-medium">{backend.name}:</span> {backend.description}
                  </li>
                ))}
              </ul>
              <p className="mt-2 text-muted-foreground/80">
                Teste os métodos para ver qual funciona melhor no seu computador.
              </p>
            </div>
          )}
        </div>
      </div>

      {error && (
        <div className="p-2 text-xs text-danger bg-danger-soft border border-danger/30 rounded-md">
          {error}
        </div>
      )}

      <div className="space-y-2">
        {backends.map((backend) => {
          // Disable Core Audio option
          const isCoreAudio = backend.id === 'screencapturekit';
          const isDisabled = disabled || isCoreAudio;

          return (
            <label
              key={backend.id}
              className={`flex items-start p-3 border rounded-lg transition-all ${
                currentBackend === backend.id
                  ? 'border-primary bg-brand-soft'
                  : 'border-border hover:border-input bg-card'
              } ${isDisabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`}
            >
              <input
                type="radio"
                name="audioBackend"
                value={backend.id}
                checked={currentBackend === backend.id}
                onChange={() => handleBackendChange(backend.id)}
                disabled={isDisabled}
                className="mt-1 h-4 w-4 text-brand focus:ring-ring border-border"
              />
              <div className="ml-3 flex-1">
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium text-foreground">
                    {backend.name}
                  </span>
                  {currentBackend === backend.id && (
                    <span className="text-xs font-medium text-brand bg-brand-soft px-2 py-0.5 rounded">
                      Ativo
                    </span>
                  )}
                  {isCoreAudio && (
                    <span className="text-xs font-medium text-muted-foreground bg-muted px-2 py-0.5 rounded">
                      Desativado
                    </span>
                  )}
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{backend.description}</p>
              </div>
            </label>
          );
        })}
      </div>

      <div className="text-xs text-muted-foreground space-y-1">
        <p>• O método escolhido só afeta a captura do áudio do sistema</p>
        <p>• O microfone sempre usa o método padrão</p>
        <p>• As mudanças valem para as próximas gravações</p>
      </div>
    </div>
  );
}