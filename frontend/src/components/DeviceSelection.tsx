import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { RefreshCw, Mic, Speaker, CircleHelp } from 'lucide-react';
import { AudioLevelMeter, CompactAudioLevelMeter } from './AudioLevelMeter';
import { AudioBackendSelector } from './AudioBackendSelector';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import Analytics from '@/lib/analytics';
import {
  AudioDevice,
  SelectedDevices,
  decodeDeviceOption,
  deviceLabel,
  encodeDeviceOption,
  fetchAudioDevices,
  isDeviceAvailable,
  cachedDeviceLabel,
} from '@/lib/audioDevices';

export type { AudioDevice, SelectedDevices };

export interface AudioLevelData {
  device_name: string;
  device_type: string;
  rms_level: number;
  peak_level: number;
  is_active: boolean;
}

export interface AudioLevelUpdate {
  timestamp: number;
  levels: AudioLevelData[];
}

interface DeviceSelectionProps {
  selectedDevices: SelectedDevices;
  onDeviceChange: (devices: SelectedDevices) => void;
  disabled?: boolean;
}

export function DeviceSelection({ selectedDevices, onDeviceChange, disabled = false }: DeviceSelectionProps) {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [audioLevels, setAudioLevels] = useState<Map<string, AudioLevelData>>(new Map());
  const [isMonitoring, setIsMonitoring] = useState(false);
  const [showLevels, setShowLevels] = useState(false);

  // Filter devices by type
  const inputDevices = devices.filter(device => device.device_type === 'Input');
  const outputDevices = devices.filter(device => device.device_type === 'Output');

  // Fetch available audio devices
  const fetchDevices = async () => {
    try {
      setError(null);
      const result = await fetchAudioDevices();
      setDevices(result);
      console.log('Fetched audio devices:', result);
    } catch (err) {
      console.error('Failed to fetch audio devices:', err);
      setError('Não foi possível carregar os dispositivos de áudio. Confira os ajustes de som do sistema.');
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  // Load devices on component mount
  useEffect(() => {
    fetchDevices();
  }, []);

  // Set up audio level event listener
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setupAudioLevelListener = async () => {
      try {
        unlisten = await listen<AudioLevelUpdate>('audio-levels', (event) => {
          const levelUpdate = event.payload;
          const newLevels = new Map<string, AudioLevelData>();

          levelUpdate.levels.forEach(level => {
            newLevels.set(level.device_name, level);
          });

          setAudioLevels(newLevels);
        });
      } catch (err) {
        console.error('Failed to setup audio level listener:', err);
      }
    };

    setupAudioLevelListener();

    // Cleanup function
    return () => {
      if (unlisten) {
        unlisten();
      }
      // Stop monitoring when component unmounts
      if (isMonitoring) {
        stopAudioLevelMonitoring();
      }
    };
  }, [isMonitoring]);

  // Handle device refresh
  const handleRefresh = async () => {
    setRefreshing(true);
    await fetchDevices();
  };

  // Helper function to detect device category and Bluetooth status
  const labelFor = (id: string) => {
    const match = devices.find(d => d.name === id);
    return match ? deviceLabel(match) : id;
  };

  const getDeviceMetadata = (deviceName: string | null) => {
    if (deviceName === null) return { isBluetooth: false, category: 'default' };
    const nameLower = deviceName.toLowerCase();

    // Detect if it's Bluetooth
    const isBluetooth = nameLower.includes('airpods')
      || nameLower.includes('bluetooth')
      || nameLower.includes('wireless')
      || nameLower.includes('wh-')  // Sony WH-* series
      || nameLower.includes('bt ');

    // Categorize device
    let category = 'wired';
    if (nameLower.includes('airpods')) {
      category = 'airpods';
    } else if (isBluetooth) {
      category = 'bluetooth';
    }

    return { isBluetooth, category };
  };

  // Handle microphone device selection
  const handleMicDeviceChange = (optionValue: string) => {
    const deviceName = decodeDeviceOption(optionValue);
    if (deviceName === undefined) return;
    const newDevices = {
      ...selectedDevices,
      micDevice: deviceName
    };
    onDeviceChange(newDevices);

    // Track device selection analytics with enhanced metadata
    const metadata = getDeviceMetadata(deviceName === null ? null : labelFor(deviceName));
    Analytics.track('microphone_selected', {
      device_category: metadata.category,
      is_bluetooth: metadata.isBluetooth.toString(),
      has_system_audio: (!!selectedDevices.systemDevice).toString()
    }).catch(err => console.error('Failed to track microphone selection:', err));
  };

  // Handle system audio device selection
  const handleSystemDeviceChange = (optionValue: string) => {
    const deviceName = decodeDeviceOption(optionValue);
    if (deviceName === undefined) return;
    const newDevices = {
      ...selectedDevices,
      systemDevice: deviceName
    };
    onDeviceChange(newDevices);

    // Track device selection analytics with enhanced metadata
    const metadata = getDeviceMetadata(deviceName === null ? null : labelFor(deviceName));
    Analytics.track('system_audio_selected', {
      device_category: metadata.category,
      is_bluetooth: metadata.isBluetooth.toString(),
      has_microphone: (!!selectedDevices.micDevice).toString()
    }).catch(err => console.error('Failed to track system audio selection:', err));
  };

  // Start audio level monitoring
  const startAudioLevelMonitoring = async () => {
    try {
      // Only monitor input devices for now (microphones)
      const deviceNames = inputDevices.map(device => device.name);
      if (deviceNames.length === 0) {
        setError('Nenhum microfone encontrado para monitorar');
        return;
      }

      await invoke('start_audio_level_monitoring', { deviceNames });
      setIsMonitoring(true);
      setShowLevels(true);
      console.log('Started audio level monitoring for input devices:', deviceNames);
    } catch (err) {
      console.error('Failed to start audio level monitoring:', err);
      setError('Não foi possível iniciar o monitoramento do nível de áudio');
    }
  };

  // Stop audio level monitoring
  const stopAudioLevelMonitoring = async () => {
    try {
      await invoke('stop_audio_level_monitoring');
      setIsMonitoring(false);
      setAudioLevels(new Map());
      console.log('Stopped audio level monitoring');
    } catch (err) {
      console.error('Failed to stop audio level monitoring:', err);
    }
  };

  // Toggle audio level monitoring
  const toggleAudioLevelMonitoring = async () => {
    if (isMonitoring) {
      await stopAudioLevelMonitoring();
    } else {
      await startAudioLevelMonitoring();
    }
  };

  if (loading) {
    return (
      <div className="p-4 space-y-4">
        <div className="animate-pulse">
          <div className="h-4 bg-accent rounded w-1/3 mb-4"></div>
          <div className="h-10 bg-accent rounded mb-3"></div>
          <div className="h-10 bg-accent rounded"></div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium text-foreground">Dispositivos de áudio</h4>
        <div className="flex items-center space-x-2">
          {/* TODO: Monitoring */}
          {/* <button */}
          {/*   onClick={toggleAudioLevelMonitoring} */}
          {/*   disabled={disabled || inputDevices.length === 0} */}
          {/*   className={`px-3 py-1 text-xs font-medium rounded-md transition-colors ${ */}
          {/*     isMonitoring */}
          {/*       ? 'bg-danger-soft text-danger hover:bg-danger/20' */}
          {/*       : 'bg-success-soft text-success hover:bg-success/20' */}
          {/*   } disabled:pointer-events-none disabled:opacity-50`} */}
          {/*   title={inputDevices.length === 0 ? 'No microphones available to test' : ''} */}
          {/* > */}
          {/*   {isMonitoring ? 'Stop Test' : 'Test Mic'} */}
          {/* </button> */}
          <button
            onClick={handleRefresh}
            disabled={refreshing || disabled}
            className="h-8 w-8 p-0 inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors hover:bg-muted disabled:pointer-events-none disabled:opacity-50"
          >
            <RefreshCw className={`h-4 w-4 ${refreshing ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>

      {error && (
        <div className="p-3 text-sm text-danger bg-danger-soft border border-danger/30 rounded-md">
          {error}
        </div>
      )}

      <div className="space-y-3">
        {/* Microphone Selection */}
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Mic className="h-4 w-4 text-muted-foreground" />
            <Label htmlFor="mic-selection" className="text-sm font-medium text-foreground">
              Microfone
            </Label>
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  aria-label="Sobre a escolha do microfone"
                  className="text-muted-foreground/80 hover:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-full"
                >
                  <CircleHelp aria-hidden="true" className="h-4 w-4" />
                </button>
              </TooltipTrigger>
              <TooltipContent className="max-w-xs">
                Escolha o microfone da gravação. O padrão segue o microfone selecionado no sistema.
              </TooltipContent>
            </Tooltip>
          </div>
          <Select
            value={encodeDeviceOption(selectedDevices.micDevice)}
            onValueChange={handleMicDeviceChange}
            disabled={disabled}
          >
            <SelectTrigger id="mic-selection" className="w-full">
              <SelectValue placeholder="Selecione o microfone" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={encodeDeviceOption(null)}>Microfone padrão</SelectItem>
              {selectedDevices.micDevice &&
                !isDeviceAvailable(devices, 'Input', selectedDevices.micDevice) && (
                  <SelectItem value={encodeDeviceOption(selectedDevices.micDevice)}>
                    {cachedDeviceLabel(selectedDevices.micDevice)} (indisponível, escolha um dispositivo)
                  </SelectItem>
                )}
              {inputDevices.map((device) => (
                <SelectItem
                  key={device.name}
                  value={encodeDeviceOption(device.name)}
                >
                  {deviceLabel(device)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {inputDevices.length === 0 && (
            <p className="text-xs text-muted-foreground">Nenhum microfone encontrado</p>
          )}

          {/* Audio Level Meters for Input Devices */}
          {showLevels && inputDevices.length > 0 && (
            <div className="space-y-2 pt-2 border-t border-border">
              <p className="text-xs text-muted-foreground font-medium">Níveis do microfone:</p>
              {inputDevices.map((device) => {
                const levelData = audioLevels.get(device.name);
                return (
                  <div key={`level-${device.name}`} className="space-y-1">
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-muted-foreground truncate max-w-[200px]">
                        {deviceLabel(device)}
                      </span>
                      {levelData && (
                        <CompactAudioLevelMeter
                          rmsLevel={levelData.rms_level}
                          peakLevel={levelData.peak_level}
                          isActive={levelData.is_active}
                        />
                      )}
                    </div>
                    {levelData && (
                      <AudioLevelMeter
                        rmsLevel={levelData.rms_level}
                        peakLevel={levelData.peak_level}
                        isActive={levelData.is_active}
                        deviceName={deviceLabel(device)}
                        size="small"
                      />
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* System Audio Selection */}
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Speaker className="h-4 w-4 text-muted-foreground" />
            <Label htmlFor="system-selection" className="text-sm font-medium text-foreground">
              System Audio
            </Label>
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  aria-label="Sobre a escolha do áudio do sistema"
                  className="text-muted-foreground/80 hover:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-full"
                >
                  <CircleHelp aria-hidden="true" className="h-4 w-4" />
                </button>
              </TooltipTrigger>
              <TooltipContent className="max-w-xs">
                Escolha o áudio a gravar. O padrão segue a saída selecionada no sistema.
              </TooltipContent>
            </Tooltip>
          </div>

          <Select
            value={encodeDeviceOption(selectedDevices.systemDevice)}
            onValueChange={handleSystemDeviceChange}
            disabled={disabled}
          >
            <SelectTrigger id="system-selection" className="w-full">
              <SelectValue placeholder="Selecione o áudio do sistema" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={encodeDeviceOption(null)}>Áudio do sistema padrão</SelectItem>
              {selectedDevices.systemDevice &&
                !isDeviceAvailable(devices, 'Output', selectedDevices.systemDevice) && (
                  <SelectItem value={encodeDeviceOption(selectedDevices.systemDevice)}>
                    {cachedDeviceLabel(selectedDevices.systemDevice)} (indisponível, escolha um dispositivo)
                  </SelectItem>
                )}
              {outputDevices.map((device) => (
                <SelectItem
                  key={device.name}
                  value={encodeDeviceOption(device.name)}
                >
                  {deviceLabel(device)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          {outputDevices.length === 0 && (
            <p className="text-xs text-muted-foreground">Nenhum dispositivo de áudio do sistema encontrado</p>
          )}

          {/* Backend Selection - available on all platforms */}
          {!disabled && (
            <div className="pt-3 border-t border-border">
              <AudioBackendSelector disabled={disabled} />
            </div>
          )}
        </div>
      </div>

      {/* Info text */}
      <div className="text-xs text-muted-foreground space-y-1">
        <p>• <strong>Microfone:</strong> grava sua voz e o som ambiente</p>
        <p>• <strong>Áudio do sistema:</strong> grava o som do computador (música, chamadas etc.)</p>
      </div>
    </div>
  );
}
