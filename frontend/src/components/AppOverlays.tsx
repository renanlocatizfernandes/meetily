'use client';

import { AutoRecordPrompt } from '@/components/AutoRecord/AutoRecordPrompt';
import { MeetingEndPrompt } from '@/components/AutoRecord/MeetingEndPrompt';
import { PrivacyChoicesDialog } from '@/components/Privacy/PrivacyChoicesDialog';

/**
 * Overlays of the main window that must work on every page (not mounted in the
 * always-on-top window nor during the first-run setup).
 */
export function AppOverlays({ onboardingCompleted }: { onboardingCompleted: boolean }) {
  return (
    <>
      <AutoRecordPrompt />
      <MeetingEndPrompt />
      {onboardingCompleted && <PrivacyChoicesDialog />}
    </>
  );
}
