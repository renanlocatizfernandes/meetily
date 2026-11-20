'use client';

import { useCallback, useEffect, useState } from 'react';
import { listenAll, safeInvoke } from '@/lib/tauriSafe';
import { SmartAlert, UsageSnapshot, addAlert, parseAlert } from '@/lib/liveAssistSmart';

/**
 * Proactive alerts (`live-assist-smart-alert`) and the estimated consumption
 * (`live-assist-usage`) of the recording in progress. Each webview keeps its own list.
 */
export function useLiveAssistSmart(recording: boolean) {
  const [alerts, setAlerts] = useState<SmartAlert[]>([]);
  const [usage, setUsage] = useState<UsageSnapshot | null>(null);

  useEffect(() => {
    const stop = listenAll({
      'live-assist-smart-alert': (payload: unknown) => {
        const alert = parseAlert(payload);
        if (alert) setAlerts((current) => addAlert(current, alert));
      },
      'live-assist-usage': (payload: UsageSnapshot) => {
        if (payload) setUsage(payload);
      },
    });
    // A window opened in the middle of a recording asks for the current numbers.
    safeInvoke<UsageSnapshot | null>('get_live_assist_usage').then((result) => {
      if (result.ok && result.value) setUsage((current) => current ?? result.value);
    });
    return stop;
  }, []);

  // A new recording starts clean.
  useEffect(() => {
    if (!recording) setUsage(null);
  }, [recording]);

  const dismissAlert = useCallback((id: string) => {
    setAlerts((current) => current.filter((alert) => alert.id !== id));
  }, []);

  const clearAlerts = useCallback(() => setAlerts([]), []);

  return { alerts, usage, dismissAlert, clearAlerts };
}
