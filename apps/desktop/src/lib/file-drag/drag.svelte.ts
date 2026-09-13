class DragState {
  entry = $state<{ path: string; paneId: string } | null>(null);
  tab = $state<{ paneId: string; id: string } | null>(null);
}

export const drag = new DragState();
