interface StatusOverlaysProps {
  // Status flags
  isProcessing: boolean;      // Processing transcription after recording stops
  isSaving: boolean;          // Saving transcript to database

  // Layout
  sidebarCollapsed: boolean;  // For responsive margin calculation
}

// Internal reusable component for individual status overlays
interface StatusOverlayProps {
  show: boolean;
  message: string;
  sidebarCollapsed: boolean;
}

function StatusOverlay({ show, message, sidebarCollapsed }: StatusOverlayProps) {
  if (!show) return null;

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-5 z-10 flex justify-center px-4" role="status">
      <div className="flex h-11 items-center gap-2.5 rounded-full border bg-card px-4 shadow-[0_4px_20px_hsl(0_0%_0%/0.08)]">
        <div className="h-4 w-4 animate-spin rounded-full border-2 border-border border-t-foreground" aria-hidden="true"></div>
        <span className="text-sm text-foreground">{message}</span>
      </div>
    </div>
  );
}

// Main exported component - renders multiple status overlays
export function StatusOverlays({
  isProcessing,
  isSaving,
  sidebarCollapsed
}: StatusOverlaysProps) {
  return (
    <>
      {/* Processing status overlay - shown after recording stops while finalizing transcription */}
      <StatusOverlay
        show={isProcessing}
        message="Finalizando a transcrição…"
        sidebarCollapsed={sidebarCollapsed}
      />

      {/* Saving status overlay - shown while saving transcript to database */}
      <StatusOverlay
        show={isSaving}
        message="Salvando a transcrição…"
        sidebarCollapsed={sidebarCollapsed}
      />
    </>
  );
}
