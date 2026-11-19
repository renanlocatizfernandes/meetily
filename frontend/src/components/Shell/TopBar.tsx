'use client';

import React from 'react';

interface TopBarProps {
  /** Título (texto ou um editor no lugar). */
  title: React.ReactNode;
  /** Etiquetas ao lado do título (cliente, categoria…). */
  chips?: React.ReactNode;
  /** Ações contextuais, à direita. */
  actions?: React.ReactNode;
}

/** Barra superior fina da área principal: título, etiquetas e ações contextuais. */
export function TopBar({ title, chips, actions }: TopBarProps) {
  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b bg-background px-3">
      <div className="flex min-w-0 flex-1 items-center gap-2 pl-1">
        <div className="min-w-0 shrink text-sm font-medium">{title}</div>
        {chips && <div className="flex min-w-0 shrink-0 items-center gap-1.5">{chips}</div>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-1">{actions}</div>}
    </header>
  );
}
