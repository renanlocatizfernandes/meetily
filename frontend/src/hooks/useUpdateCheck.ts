import { useEffect, useState } from 'react';
import { updateService, UpdateInfo } from '@/services/updateService';
import { showUpdateNotification } from '@/components/UpdateNotification';

interface UseUpdateCheckOptions {
  checkOnMount?: boolean;
  showNotification?: boolean;
  onUpdateAvailable?: (info: UpdateInfo) => void;
}

export function useUpdateCheck(options: UseUpdateCheckOptions = {}) {
  const {
    checkOnMount = true,
    showNotification = true,
    onUpdateAvailable,
  } = options;

  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [isChecking, setIsChecking] = useState(false);

  const checkForUpdates = async (force = false) => {
    // Skip if checked recently (unless forced)
    if (!force && updateService.wasCheckedRecently()) {
      return;
    }

    setIsChecking(true);
    try {
      const info = await updateService.checkForUpdates(force);
      setUpdateInfo(info);

      if (info.available) {
        if (onUpdateAvailable) {
          onUpdateAvailable(info);
        } else if (showNotification) {
          showUpdateNotification(info, () => {
            // This will be handled by the component that uses this hook
          });
        }
      }
    } catch (error) {
      console.error('Failed to check for updates:', error);
      // Silently fail on startup checks to avoid disrupting user experience
    } finally {
      setIsChecking(false);
    }
  };

  useEffect(() => {
    if (checkOnMount) {
      // Delay the check slightly to avoid blocking app startup
      const timer = setTimeout(() => {
        checkForUpdates(false);
      }, 2000); // Check 2 seconds after mount

      // O app costuma ficar aberto por dias: confere de novo de tempos em
      // tempos. O serviço ignora a chamada se já conferiu nas últimas 24 h.
      const periodic = setInterval(() => {
        checkForUpdates(false);
      }, 6 * 60 * 60 * 1000);

      return () => {
        clearTimeout(timer);
        clearInterval(periodic);
      };
    }
  }, [checkOnMount]);

  return {
    updateInfo,
    isChecking,
    checkForUpdates,
  };
}
