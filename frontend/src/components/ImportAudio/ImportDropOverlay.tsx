import React from 'react';
import { Upload } from 'lucide-react';
import { getAudioFormatsDisplayList } from '@/constants/audioFormats';

interface ImportDropOverlayProps {
  visible: boolean;
}

export function ImportDropOverlay({ visible }: ImportDropOverlayProps) {
  if (!visible) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm
                 flex items-center justify-center pointer-events-none
                 transition-opacity duration-150"
    >
      <div className="border-2 border-dashed border-primary rounded-2xl
                      p-12 text-center bg-primary/50 shadow-md
                      transform scale-100 transition-transform">
        <Upload className="h-16 w-16 text-brand mx-auto mb-4" />
        <p className="text-xl font-medium text-white">Solte o arquivo de áudio para importar</p>
        <p className="text-sm text-primary-foreground/80 mt-2">{getAudioFormatsDisplayList()}</p>
      </div>
    </div>
  );
}
