import React, { useState, useEffect } from 'react';
import { Switch } from '@/components/ui/switch';
import { FolderOpen } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { DeviceSelection, SelectedDevices } from '@/components/DeviceSelection';
import Analytics from '@/lib/analytics';
import { cachedDeviceLabel } from '@/lib/audioDevices';
import { toast } from 'sonner';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useConfig } from '@/contexts/ConfigContext';

export interface RecordingPreferences {
  device_selection_version: [number, number];
  save_folder: string;
  auto_save: boolean;
  file_format: string;
  preferred_mic_device: string | null;
  preferred_system_device: string | null;
}

interface RecordingSettingsProps {
  onSave?: (preferences: RecordingPreferences) => void;
}

export function RecordingSettings({ onSave }: RecordingSettingsProps) {
  const [preferences, setPreferences] = useState<RecordingPreferences>({
    save_folder: '',
    auto_save: true,
    file_format: 'mp4',
    device_selection_version: [1, 1],
    preferred_mic_device: null,
    preferred_system_device: null
  });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [showRecordingNotification, setShowRecordingNotification] = useState(false);
  const { isRecording } = useRecordingState();
  const { setSelectedDevices } = useConfig();

  // Load recording preferences on component mount
  useEffect(() => {
    const loadPreferences = async () => {
      try {
        const prefs = await invoke<RecordingPreferences>('get_recording_preferences');
        setPreferences(prefs);
      } catch (error) {
        console.error('Failed to load recording preferences:', error);
        toast.error('Não foi possível carregar as preferências de gravação', { description: String(error) });
        // If loading fails, get default folder path
        try {
          const defaultPath = await invoke<string>('get_default_recordings_folder_path');
          setPreferences(prev => ({ ...prev, save_folder: defaultPath }));
        } catch (defaultError) {
          console.error('Failed to get default folder path:', defaultError);
        }
      } finally {
        setLoading(false);
      }
    };

    loadPreferences();
  }, []);

  // Load recording notification preference
  useEffect(() => {
    const loadNotificationPref = async () => {
      try {
        const { Store } = await import('@tauri-apps/plugin-store');
        const store = await Store.load('preferences.json');
        const show = await store.get<boolean>('show_recording_notification') ?? false;
        setShowRecordingNotification(show);
      } catch (error) {
        console.error('Failed to load notification preference:', error);
      }
    };
    loadNotificationPref();
  }, []);

  const handleAutoSaveToggle = async (enabled: boolean) => {
    const newPreferences = { ...preferences, auto_save: enabled };
    setPreferences(newPreferences);
    await savePreferences(newPreferences);

    // Track auto-save setting change
    await Analytics.track('auto_save_recording_toggled', {
      enabled: enabled.toString()
    });
  };

  const handleDeviceChange = async (devices: SelectedDevices) => {
    const newPreferences: RecordingPreferences = {
      ...preferences,
      device_selection_version: [
        devices.micDevice !== preferences.preferred_mic_device ? 1 : preferences.device_selection_version[0],
        devices.systemDevice !== preferences.preferred_system_device ? 1 : preferences.device_selection_version[1],
      ],
      preferred_mic_device: devices.micDevice,
      preferred_system_device: devices.systemDevice
    };
    setPreferences(newPreferences);
    // Sync the in-memory selection ConfigContext exposes to the start path.
    // Without this, the picker only writes to disk (loaded into context once at
    // app mount), so a newly-chosen mic isn't honored until the next launch —
    // start keeps sending the stale launch-time device.
    setSelectedDevices(devices);
    await savePreferences(newPreferences);

    // Track default device preference changes
    // Note: Individual device selection analytics are tracked in DeviceSelection component
    await Analytics.track('default_devices_changed', {
      has_preferred_microphone: (!!devices.micDevice).toString(),
      has_preferred_system_audio: (!!devices.systemDevice).toString()
    });
  };

  const handleOpenFolder = async () => {
    try {
      await invoke('open_recordings_folder');
    } catch (error) {
      console.error('Failed to open recordings folder:', error);
    }
  };

  const handleNotificationToggle = async (enabled: boolean) => {
    try {
      setShowRecordingNotification(enabled);
      const { Store } = await import('@tauri-apps/plugin-store');
      const store = await Store.load('preferences.json');
      await store.set('show_recording_notification', enabled);
      await store.save();
      toast.success('Preferência salva');
      await Analytics.track('recording_notification_preference_changed', {
        enabled: enabled.toString()
      });
    } catch (error) {
      console.error('Failed to save notification preference:', error);
      toast.error('Não foi possível salvar a preferência');
    }
  };

  const savePreferences = async (prefs: RecordingPreferences) => {
    setSaving(true);
    try {
      await invoke('set_recording_preferences', { preferences: prefs });
      const saved = await invoke<RecordingPreferences>('get_recording_preferences');
      setPreferences(saved);
      setSelectedDevices({ micDevice: saved.preferred_mic_device, systemDevice: saved.preferred_system_device });
      onSave?.(saved);

      // Show success toast with device details
      const micDevice = prefs.preferred_mic_device ? cachedDeviceLabel(prefs.preferred_mic_device) : 'Padrão';
      const systemDevice = prefs.preferred_system_device ? cachedDeviceLabel(prefs.preferred_system_device) : 'Padrão';
      toast.success("Dispositivos preferidos salvos", {
        description: `Microfone: ${micDevice}, áudio do sistema: ${systemDevice}`
      });
    } catch (error) {
      console.error('Failed to save recording preferences:', error);
      toast.error("Não foi possível salvar os dispositivos preferidos", {
        description: error instanceof Error ? error.message : String(error)
      });
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="animate-pulse">
        <div className="h-4 bg-accent rounded w-1/4 mb-4"></div>
        <div className="h-8 bg-accent rounded mb-4"></div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-semibold mb-4">Gravação</h3>
        <p className="text-sm text-muted-foreground mb-6">
          Defina como o áudio das reuniões é salvo.
        </p>
      </div>

      {/* Auto Save Toggle */}
      <div className="flex items-center justify-between p-4 border rounded-lg">
        <div className="flex-1">
          <div className="font-medium">Salvar o áudio das gravações</div>
          <div className="text-sm text-muted-foreground">
            Salva o arquivo de áudio automaticamente quando a gravação termina
          </div>
        </div>
        <Switch
          checked={preferences.auto_save}
          onCheckedChange={handleAutoSaveToggle}
          disabled={saving}
        />
      </div>

      {/* Folder Location - Only shown when auto_save is enabled */}
      {preferences.auto_save && (
        <div className="space-y-4">
          <div className="p-4 border rounded-lg bg-muted">
            <div className="font-medium mb-2">Local de salvamento</div>
            <div className="text-sm text-muted-foreground mb-3 break-all">
              {preferences.save_folder || 'Pasta padrão'}
            </div>
            <button
              onClick={handleOpenFolder}
              className="flex items-center gap-2 px-3 py-2 text-sm border border-border rounded-md hover:bg-muted transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Abrir pasta
            </button>
          </div>

          <div className="p-4 border rounded-lg bg-brand-soft">
            <div className="text-sm text-brand">
              <strong>Formato do arquivo:</strong> {preferences.file_format.toUpperCase()}
            </div>
            <div className="text-xs text-brand mt-1">
              As gravações são salvas com data e hora no nome: recording_YYYYMMDD_HHMMSS.{preferences.file_format}
            </div>
          </div>
        </div>
      )}

      {/* Info when auto_save is disabled */}
      {!preferences.auto_save && (
        <div className="p-4 border rounded-lg bg-warning-soft">
          <div className="text-sm text-warning">
            O salvamento de áudio está desativado. Ative "Salvar o áudio das gravações" para guardar o áudio das reuniões automaticamente.
          </div>
        </div>
      )}

      {/* Recording Notification Toggle */}
      <div className="flex items-center justify-between p-4 border rounded-lg">
        <div className="flex-1">
          <div className="font-medium">Aviso ao iniciar a gravação</div>
          <div className="text-sm text-muted-foreground">
            Mostra um lembrete para avisar os participantes quando a gravação começa
          </div>
        </div>
        <Switch
          checked={showRecordingNotification}
          onCheckedChange={handleNotificationToggle}
        />
      </div>

      {/* Device Preferences */}
      <div className="space-y-4">
        <div className="border-t pt-6">
          <h4 className="text-base font-medium text-foreground mb-4">Dispositivos de áudio padrão</h4>
          <p className="text-sm text-muted-foreground mb-4">
            Escolha o microfone e o áudio do sistema preferidos. Eles serão selecionados automaticamente nas próximas gravações.
          </p>

          {isRecording && (
            <p
              role="status"
              aria-live="polite"
              className="text-sm text-warning bg-warning-soft border border-warning/30 rounded-md p-2 mb-4"
            >
              A escolha de dispositivos fica bloqueada durante a gravação. Conectar um dispositivo novo no meio da gravação não troca para ele. Pare a reunião atual para mudar os dispositivos.
            </p>
          )}

          <div className="border rounded-lg p-4 bg-muted">
            <DeviceSelection
              selectedDevices={{
                micDevice: preferences.preferred_mic_device,
                systemDevice: preferences.preferred_system_device
              }}
              onDeviceChange={handleDeviceChange}
              disabled={saving || isRecording}
            />
          </div>
        </div>
      </div>
    </div>
  );
}