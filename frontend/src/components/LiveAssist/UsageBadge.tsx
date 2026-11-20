'use client';

import { Coins } from 'lucide-react';
import { UsageSnapshot, usageLabel, usageTitle } from '@/lib/liveAssistSmart';

/** Estimated consumption of the recording in progress. Cloud use is highlighted. */
export function UsageBadge({ usage }: { usage: UsageSnapshot | null }) {
  const label = usageLabel(usage);
  if (!label) return null;
  const paid = Boolean(usage?.paid);
  return (
    <span
      title={usageTitle(usage)}
      className={`inline-flex items-center gap-1 rounded-full px-2 py-1 text-[11px] font-medium ${
        paid ? 'bg-amber-50 text-amber-800' : 'bg-muted text-muted-foreground'
      }`}
    >
      <Coins className="h-3 w-3" aria-hidden="true" />
      {label}
    </span>
  );
}
