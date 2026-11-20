'use client';

import { Copy, Pin, PinOff, X, AppWindow, ChevronRight, Eraser } from 'lucide-react';
import { toast } from 'sonner';
import { LiveAssistItem, LiveAssistSection, useLiveAssist } from '@/contexts/LiveAssistContext';
import { RecursosBar } from '@/components/Recursos/RecursosBar';
import { ProvisionalItem } from '@/components/LiveAssist/ProvisionalItem';
import { OriginTag } from '@/components/LiveAssist/OriginTag';
import { ContextInUse } from '@/components/LiveAssist/ContextInUse';
import { SmartAlerts } from '@/components/LiveAssist/SmartAlerts';
import { RecapCard } from '@/components/LiveAssist/RecapCard';
import { UsageBadge } from '@/components/LiveAssist/UsageBadge';
import { useLiveAssistSmart } from '@/hooks/useLiveAssistSmart';
import { itemKey } from '@/lib/liveAssistSync';
import { shortcutText } from '@/lib/liveAssistShortcut';

// Contexto comes first and is highlighted: it is what anchors the other two.
const SECTIONS: Array<{ id: LiveAssistSection; title: string; empty: string; hint?: string }> = [
  {
    id: 'contexto',
    title: 'Contexto',
    empty: 'Nada para explicar ainda. Termos, pessoas, números e combinados anteriores aparecem aqui.',
    hint: 'Com a origem de cada informação',
  },
  { id: 'perguntas', title: 'Perguntas', empty: 'Nenhuma pergunta sugerida ainda.' },
  { id: 'respostas', title: 'Respostas', empty: 'Nenhuma resposta sugerida ainda.' },
];

function sectionItems(items: LiveAssistItem[], section: LiveAssistSection) {
  return items
    .filter((item) => item.section === section)
    .sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.createdAt - a.createdAt);
}

function ItemCard({
  item,
  fade,
  onCopy,
  onPin,
  onDismiss,
}: {
  item: LiveAssistItem;
  fade: number;
  onCopy: () => void;
  onPin: () => void;
  onDismiss: () => void;
}) {
  return (
    <article
      className="rounded-lg border border-gray-200 bg-white p-3 shadow-sm"
      style={{ opacity: item.pinned ? 1 : fade }}
    >
      {item.origin && <OriginTag origin={item.origin} />}
      <p className="text-sm text-gray-800 leading-relaxed">{item.text}</p>
      <div className="mt-2 flex gap-1">
        <button type="button" className="inline-flex items-center gap-1 rounded px-2 py-1 text-xs text-gray-600 hover:bg-gray-100" onClick={onCopy}>
          <Copy className="h-3.5 w-3.5" /> Copiar
        </button>
        <button type="button" className="inline-flex items-center gap-1 rounded px-2 py-1 text-xs text-gray-600 hover:bg-gray-100" onClick={onPin}>
          {item.pinned ? <PinOff className="h-3.5 w-3.5" /> : <Pin className="h-3.5 w-3.5" />}
          {item.pinned ? 'Desafixar' : 'Fixar'}
        </button>
        <button type="button" className="inline-flex items-center gap-1 rounded px-2 py-1 text-xs text-gray-600 hover:bg-gray-100" onClick={onDismiss}>
          <X className="h-3.5 w-3.5" /> Descartar
        </button>
      </div>
    </article>
  );
}

export function LiveAssistPanel({ floating = false }: { floating?: boolean }) {
  const { items, pinItem, dismissItem, panelOpen, setPanelOpen, status, openAlwaysOnTop, settings } = useLiveAssist();
  const { partials, shortcut, clearItems, contextSummary } = useLiveAssist();
  const shortcutHint = shortcut.status && !shortcut.status.registered
    ? 'Atalho indisponível. Ajuste nas configurações.'
    : `Atalho: ${shortcutText(shortcut.accelerator)}`;
  const { alerts, usage, dismissAlert, clearAlerts } = useLiveAssistSmart(status.recording);
  const cloud = Boolean(contextSummary?.localOnlyBlocked) || Boolean(usage?.paid);
  const active = status.recording && settings.enabled;
  const itemKeys = new Set(items.map(itemKey));
  const hasUnpinned = items.some((item) => !item.pinned);

  if (!floating && !panelOpen) {
    return null;
  }

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success('Copiado');
    } catch {
      toast.error('Não foi possível copiar');
    }
  };

  return (
    <aside className={floating ? 'flex h-screen w-full flex-col bg-gray-50' : 'flex h-full w-[340px] shrink-0 flex-col border-l border-gray-200 bg-gray-50'}>
      <header className="border-b border-gray-200 bg-white px-4 py-3">
        <div className="flex items-start justify-between gap-2">
          <div>
            <h2 className="text-base font-semibold text-gray-900">Ajuda ao vivo</h2>
            <p className="text-xs text-gray-500">{shortcutHint}</p>
          </div>
          {!floating && (
            <button type="button" className="rounded p-1 text-gray-500 hover:bg-gray-100" onClick={() => setPanelOpen(false)} title="Recolher">
              <ChevronRight className="h-4 w-4" />
            </button>
          )}
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
          {status.recording && settings.enabled && (
            <span className="inline-flex items-center gap-1 rounded-full bg-emerald-50 px-2 py-1 font-medium text-emerald-700">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
              Gravando · Ajuda ao vivo
            </span>
          )}
          {status.busy && <span className="text-gray-500">Preparando sugestões…</span>}
          {active && <UsageBadge usage={usage} />}
        </div>
        {status.note && <p className="mt-2 text-xs text-amber-700">{status.note}</p>}
        <div className="mt-3"><RecursosBar /></div>
        {status.recording && settings.enabled && <ContextInUse summary={contextSummary} />}
        {!floating && (
          <button type="button" className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-blue-700 hover:underline" onClick={openAlwaysOnTop}>
            <AppWindow className="h-3.5 w-3.5" /> Abrir em janela sempre visível
          </button>
        )}
        {hasUnpinned && (
          <button
            type="button"
            className="ml-auto mt-3 flex items-center gap-1 text-xs font-medium text-muted-foreground hover:text-foreground hover:underline"
            onClick={clearItems}
            title="Remove as sugestões que não estão fixadas"
          >
            <Eraser className="h-3.5 w-3.5" /> Limpar sugestões
          </button>
        )}
      </header>
      <div className="flex-1 space-y-5 overflow-y-auto px-4 py-4">
        {!status.recording && (
          <p className="text-sm text-gray-500">Comece uma gravação para receber sugestões.</p>
        )}
        {active && <RecapCard disabled={false} paid={cloud} />}
        <SmartAlerts alerts={alerts} onDismiss={dismissAlert} onClear={clearAlerts} canAnswer={active} />
        {SECTIONS.map((section) => {
          const sectionList = sectionItems(items, section.id);
          // Still being written by the model; hidden once the same text arrives as a final item.
          const provisional = partials.filter((partial) => partial.section === section.id && !itemKeys.has(itemKey(partial)));
          let unpinnedIndex = 0;
          const highlighted = section.id === 'contexto';
          return (
            <section
              key={section.id}
              className={highlighted ? 'rounded-xl border border-primary/30 bg-primary/5 p-3' : undefined}
            >
              {highlighted ? (
                <div className="mb-2 flex items-baseline justify-between gap-2">
                  <h3 className="text-sm font-semibold text-foreground">
                    {section.title}
                    {sectionList.length > 0 && (
                      <span className="ml-2 rounded-full bg-primary/15 px-1.5 py-0.5 text-[11px] font-medium text-foreground">
                        {sectionList.length}
                      </span>
                    )}
                  </h3>
                  <span className="truncate text-[11px] text-muted-foreground">{section.hint}</span>
                </div>
              ) : (
                <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-gray-500">{section.title}</h3>
              )}
              {sectionList.length === 0 && provisional.length === 0 ? (
                <p className="text-sm text-gray-400">{section.empty}</p>
              ) : (
                <div className="space-y-2">
                  {provisional.map((partial, index) => (
                    <ProvisionalItem key={`provisional-${index}`} text={partial.text} origin={partial.origin} />
                  ))}
                  {sectionList.map((item) => {
                    const fade = item.pinned ? 1 : Math.max(0.4, 1 - unpinnedIndex * 0.18);
                    if (!item.pinned) unpinnedIndex += 1;
                    return (
                      <ItemCard
                        key={item.id}
                        item={item}
                        fade={fade}
                        onCopy={() => copy(item.text)}
                        onPin={() => pinItem(item.id, !item.pinned)}
                        onDismiss={() => dismissItem(item.id)}
                      />
                    );
                  })}
                </div>
              )}
            </section>
          );
        })}
      </div>
    </aside>
  );
}
