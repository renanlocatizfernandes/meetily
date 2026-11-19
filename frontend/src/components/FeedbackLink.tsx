'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { MessagesSquare } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';

export default function FeedbackLink({ isCollapsed }: { isCollapsed: boolean }) {
  const pathname = usePathname();
  const isActive = pathname === '/feedback';
  const link = (
    <Link
      href="/feedback"
      replace={pathname === '/settings'}
      aria-label="Enviar feedback"
      aria-current={isActive ? 'page' : undefined}
      className={`flex items-center justify-center rounded-lg transition-colors active:bg-border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 ${
        isCollapsed
          ? 'p-2'
          : 'w-full px-3 py-1.5 mt-1 mb-2 text-sm font-medium shadow-sm'
      } ${
        isCollapsed
          ? `text-muted-foreground ${isActive ? 'bg-muted' : 'hover:bg-muted'}`
          : 'bg-accent text-foreground hover:bg-border'
      }`}
    >
      <MessagesSquare aria-hidden="true" className={isCollapsed ? 'w-5 h-5' : 'w-4 h-4'} />
      {!isCollapsed && <span className="ml-2">Enviar feedback</span>}
    </Link>
  );

  if (!isCollapsed) return link;

  return (
    <Tooltip>
      <TooltipTrigger asChild>{link}</TooltipTrigger>
      <TooltipContent side="right">Enviar feedback</TooltipContent>
    </Tooltip>
  );
}