'use client';

import React from 'react';

interface MainContentProps {
  children: React.ReactNode;
}

/** Área principal: uma coluna calma ao lado da barra lateral. As páginas ocupam `h-full`. */
const MainContent: React.FC<MainContentProps> = ({ children }) => {
  return (
    <main className="relative h-screen min-w-0 flex-1 overflow-hidden bg-background">
      {children}
    </main>
  );
};

export default MainContent;
