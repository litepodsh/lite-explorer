import * as ops from "./panes.js";
import type { PaneLayout } from "./panes.js";
import { TabsStore } from "../tabs/tabs.svelte.js";
import { settings } from "../settings/settings.svelte.js";

type ReactivePane = { id: string; tabs: TabsStore; previewOpen: boolean };

export class PanesStore {
  #reactive = $state<ReactivePane[]>([this.#makePane(crypto.randomUUID())]);
  #activeId = $state(this.#reactive[0].id);
  #layout = $derived<PaneLayout>(settings.current.panesLayout);
  #pure = $derived<ops.PanesState>({
    panes: this.#reactive.map((pane) => ({ id: pane.id, tabs: pane.tabs.snapshot })),
    activeId: this.#activeId,
    layout: this.#layout,
  });

  panes = $derived(this.#reactive);
  activeId = $derived(this.#activeId);
  layout = $derived(this.#layout);
  activePane = $derived(
    this.#reactive.find((pane) => pane.id === this.#activeId) ?? this.#reactive[0],
  );
  activeTabs = $derived(this.activePane.tabs);

  #makePane(id: string): ReactivePane {
    return { id, tabs: new TabsStore(() => settings.current.defaultViewMode), previewOpen: true };
  }

  #sync(state: ops.PanesState) {
    this.#activeId = state.activeId;
    if (state.layout !== this.#layout) settings.set("panesLayout", state.layout);
    const sameShape =
      state.panes.length === this.#reactive.length &&
      state.panes.every((pane, index) => pane.id === this.#reactive[index].id);
    if (sameShape) {
      // Only tab state changed (or focus/layout): keep the pane array identity so the
      // directory load effect isn't retriggered.
      for (const pane of state.panes) {
        this.#reactive.find((candidate) => candidate.id === pane.id)?.tabs.replace(pane.tabs);
      }
      return;
    }
    this.#reactive = state.panes.map((pane) => {
      const existing = this.#reactive.find((candidate) => candidate.id === pane.id);
      if (existing) {
        existing.tabs.replace(pane.tabs);
        return existing;
      }
      const created = this.#makePane(pane.id);
      created.tabs.replace(pane.tabs);
      return created;
    });
  }

  addPane(focus = true) {
    this.#sync(ops.addPane(this.#pure, crypto.randomUUID(), crypto.randomUUID(), focus));
  }

  removePane() {
    this.#sync(ops.removePane(this.#pure));
  }

  closeActivePane() {
    this.#sync(ops.removePane(this.#pure, this.#activeId));
  }

  togglePane() {
    if (this.#reactive.length >= 2) this.removePane();
    else this.addPane();
  }

  setActive(id: string) {
    this.#sync(ops.setActive(this.#pure, id));
  }

  setLayout(layout: PaneLayout) {
    this.#sync(ops.setLayout(this.#pure, layout));
  }

  toggleLayout() {
    this.setLayout(this.#layout === "row" ? "column" : "row");
  }

  newTab() {
    this.activeTabs.open();
  }

  newTabInOtherPane() {
    const other = ops.otherPane(this.#pure);
    if (!other) {
      // No second pane yet: create it with a fresh tab, keeping focus where it is.
      this.addPane(false);
      return;
    }
    const store = this.#reactive.find((pane) => pane.id === other.id);
    store?.tabs.open();
  }

  moveTab(fromPaneId: string, toPaneId: string, fromIndex: number, toIndex: number) {
    this.#sync(
      ops.moveTabBetweenPanes(
        this.#pure,
        fromPaneId,
        toPaneId,
        fromIndex,
        toIndex,
        crypto.randomUUID(),
      ),
    );
  }

  setPreviewOpen(paneId: string, open: boolean) {
    const pane = this.#reactive.find((candidate) => candidate.id === paneId);
    if (pane) pane.previewOpen = open;
  }
}
