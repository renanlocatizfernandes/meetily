'use client';

import React from 'react';
import { cn } from '@/lib/utils';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';

/**
 * Peças comuns das telas de configurações, para todas as seções terem o
 * mesmo cartão, o mesmo título e a mesma linha de opção.
 */

interface SettingsCardProps {
  title?: string;
  description?: React.ReactNode;
  /** Botão ou estado à direita do título. */
  action?: React.ReactNode;
  children?: React.ReactNode;
  className?: string;
}

export function SettingsCard({ title, description, action, children, className }: SettingsCardProps) {
  return (
    <section className={cn('rounded-xl border bg-card p-5', className)}>
      {(title || action) && (
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            {title && <h2 className="text-sm font-medium">{title}</h2>}
            {description && <p className="mt-0.5 text-[13px] text-muted-foreground">{description}</p>}
          </div>
          {action && <div className="shrink-0">{action}</div>}
        </div>
      )}
      {children && <div className={title || action ? 'mt-4' : undefined}>{children}</div>}
    </section>
  );
}

/** Empilha os cartões de uma seção com o espaçamento padrão. */
export function SettingsStack({ children, className }: { children: React.ReactNode; className?: string }) {
  return <div className={cn('space-y-4', className)}>{children}</div>;
}

interface SettingRowProps {
  id: string;
  label: string;
  hint?: React.ReactNode;
  /** Controle à direita (seletor, botão…). */
  children: React.ReactNode;
}

/** Linha "rótulo e explicação à esquerda, controle à direita". */
export function SettingRow({ id, label, hint, children }: SettingRowProps) {
  return (
    <div className="flex items-center justify-between gap-6 py-2.5 first:pt-0 last:pb-0">
      <div className="min-w-0">
        <Label htmlFor={id} className="text-sm font-medium">{label}</Label>
        {hint && <p className="mt-0.5 text-[13px] text-muted-foreground">{hint}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

interface SettingSwitchProps {
  id: string;
  label: string;
  hint?: React.ReactNode;
  checked: boolean;
  disabled?: boolean;
  onCheckedChange: (checked: boolean) => void;
}

export function SettingSwitch({ id, label, hint, checked, disabled, onCheckedChange }: SettingSwitchProps) {
  return (
    <SettingRow id={id} label={label} hint={hint}>
      <Switch id={id} checked={checked} disabled={disabled} onCheckedChange={onCheckedChange} />
    </SettingRow>
  );
}
