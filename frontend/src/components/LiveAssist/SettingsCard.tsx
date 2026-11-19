import type { ReactNode } from 'react';

/** Card used by the live assist settings. Same footprint as the existing settings cards. */
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
    <section className="mx-auto mt-8 max-w-2xl space-y-4 rounded-xl border border-border bg-card p-6 text-card-foreground">
      <div>
        <h2 className="text-lg font-semibold text-foreground">{title}</h2>
        {description && <p className="mt-1 text-sm text-muted-foreground">{description}</p>}
      </div>
      {children}
    </section>
  );
}
