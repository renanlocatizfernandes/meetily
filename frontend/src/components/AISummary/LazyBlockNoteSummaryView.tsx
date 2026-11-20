"use client";

import { forwardRef, type ComponentPropsWithoutRef, type Ref } from 'react';
import dynamic from 'next/dynamic';
import type { BlockNoteSummaryView, BlockNoteSummaryViewRef } from './BlockNoteSummaryView';

type Props = ComponentPropsWithoutRef<typeof BlockNoteSummaryView>;

// The BlockNote editor is the heaviest part of the meeting page. Loading it on
// demand lets the transcript and the page chrome appear first. next/dynamic does
// not forward refs, so the ref travels as a plain prop and is reattached inside.
const Inner = dynamic(
  () => import('./BlockNoteSummaryView').then(({ BlockNoteSummaryView: View }) => {
    function WithRef({ viewRef, ...props }: Props & { viewRef: Ref<BlockNoteSummaryViewRef> }) {
      return <View ref={viewRef} {...props} />;
    }
    return WithRef;
  }),
  {
    ssr: false,
    loading: () => (
      <div className="space-y-3 p-6" aria-busy="true" aria-label="Carregando o resumo">
        <div className="h-4 w-1/3 animate-pulse rounded bg-muted" />
        <div className="h-3 w-full animate-pulse rounded bg-muted" />
        <div className="h-3 w-5/6 animate-pulse rounded bg-muted" />
      </div>
    ),
  },
);

export const LazyBlockNoteSummaryView = forwardRef<BlockNoteSummaryViewRef, Props>(
  function LazyBlockNoteSummaryView(props, ref) {
    return <Inner {...props} viewRef={ref} />;
  },
);
