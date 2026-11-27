import type { ReactNode } from 'react';
import { SettingsCard as BaseSettingsCard } from '@/components/settings/SettingsCard';

/**
 * Cartão das configurações da ajuda ao vivo: o cartão padrão das
 * configurações, com o espaçamento de quem é empilhado sem um contêiner.
 */
export function SettingsCard({
  title,
  description,
  children,
}: {
  title: string;
  description?: ReactNode;
  children: ReactNode;
}) {
  return (
    <BaseSettingsCard title={title} description={description} className="mt-4 first:mt-0">
      <div className="space-y-4">{children}</div>
    </BaseSettingsCard>
  );
}
