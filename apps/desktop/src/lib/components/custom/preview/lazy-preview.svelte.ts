import type { Component } from "svelte";
import type { DirectoryEntry } from "#lib/components/custom/file-list/list-item.svelte";

type PreviewPanelComponent = Component<{ entry: DirectoryEntry }>;

/** The preview panel and its ~40 viewers stay out of the startup bundle; the list shows
 *  first and the panel arrives on the first preview, or earlier when the app goes idle. */
export const lazyPreview = $state<{ component: PreviewPanelComponent | null }>({ component: null });

let loading: Promise<void> | null = null;

export function loadPreviewPanel(): Promise<void> {
  loading ??= import("./preview-panel.svelte").then(
    ({ default: panel }) => {
      lazyPreview.component = panel as PreviewPanelComponent;
    },
    (error) => {
      loading = null;
      console.error("Couldn’t load the preview panel", error);
    },
  );
  return loading;
}

/** Loads the panel once the first frame is painted and the main thread is free. */
export function preloadPreviewPanelWhenIdle() {
  const idle = window.requestIdleCallback ?? ((run: () => void) => setTimeout(run, 500));
  idle(() => void loadPreviewPanel());
}
