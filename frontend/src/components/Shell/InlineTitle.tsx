'use client';

import React, { useEffect, useImperativeHandle, useRef, useState } from 'react';

export interface InlineTitleHandle {
  startEditing: () => void;
}

interface InlineTitleProps {
  value: string;
  /** Devolve `false` para manter a edição aberta (por exemplo, se não salvou). */
  onSave: (value: string) => Promise<boolean> | boolean;
  placeholder?: string;
}

/** Título editável no lugar: clique (ou Enter) para editar, Enter salva, Esc cancela. */
export const InlineTitle = React.forwardRef<InlineTitleHandle, InlineTitleProps>(
  ({ value, onSave, placeholder = 'Reunião sem título' }, ref) => {
    const [isEditing, setIsEditing] = useState(false);
    const [draft, setDraft] = useState(value);
    const [isSaving, setIsSaving] = useState(false);
    const inputRef = useRef<HTMLInputElement>(null);
    const cancelledRef = useRef(false);

    useImperativeHandle(ref, () => ({
      startEditing: () => {
        setDraft(value);
        setIsEditing(true);
      },
    }), [value]);

    useEffect(() => {
      if (isEditing) {
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    }, [isEditing]);

    const commit = async () => {
      if (cancelledRef.current) {
        cancelledRef.current = false;
        return;
      }
      const next = draft.trim();
      if (!next || next === value) {
        setIsEditing(false);
        return;
      }
      setIsSaving(true);
      try {
        if (await onSave(next)) setIsEditing(false);
      } finally {
        setIsSaving(false);
      }
    };

    if (isEditing) {
      return (
        <input
          ref={inputRef}
          value={draft}
          disabled={isSaving}
          aria-label="Título da reunião"
          onChange={(event) => setDraft(event.target.value)}
          onBlur={() => void commit()}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              void commit();
            } else if (event.key === 'Escape') {
              event.preventDefault();
              cancelledRef.current = true;
              setIsEditing(false);
            }
          }}
          className="h-8 w-[min(28rem,50vw)] rounded-md border border-input bg-card px-2 text-sm font-medium outline-none focus-visible:border-ring focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        />
      );
    }

    return (
      <button
        type="button"
        title="Clique para renomear"
        onClick={() => {
          setDraft(value);
          setIsEditing(true);
        }}
        className="block h-8 max-w-full truncate rounded-md px-2 text-left text-sm font-medium transition-colors duration-150 hover:bg-accent"
      >
        {value || placeholder}
      </button>
    );
  },
);
InlineTitle.displayName = 'InlineTitle';
