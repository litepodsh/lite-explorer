import * as ops from "./tabs.js";
import type { Location, Tab, TabsState } from "./tabs.js";

export class TabsStore {
  #state = $state.raw<TabsState>(ops.createTabsState(crypto.randomUUID()));

  tabs = $derived(this.#state.tabs);
  activeId = $derived(this.#state.activeId);
  active = $derived(ops.activeTab(this.#state));

  /** Stable key for the active tab's location: changes only when the tab or its location changes,
   *  not on selection/view updates. Used to decide when to reload a pane's directory. */
  locationKey = $derived(
    (() => {
      const tab = ops.activeTab(this.#state);
      return `${tab.id}\u0000${tab.location.kind}\u0000${tab.location.path}\u0000${tab.location.name}`;
    })(),
  );

  get snapshot(): TabsState {
    return this.#state;
  }

  replace(next: TabsState) {
    this.#state = next;
  }

  open(location?: Location) {
    this.#state = ops.openTab(this.#state, crypto.randomUUID(), location);
  }

  navigate(location: Location) {
    this.#state = ops.navigate(this.#state, location);
  }

  back() {
    this.#state = ops.goBack(this.#state);
  }

  forward() {
    this.#state = ops.goForward(this.#state);
  }

  select(id: string) {
    this.#state = ops.selectTab(this.#state, id);
  }

  /** Returns true when the window should close (last tab, already on Overview). */
  close(id: string): boolean {
    const result = ops.closeTab(this.#state, id);
    this.#state = result.state;
    return result.closeWindow;
  }

  move(from: number, to: number) {
    this.#state = ops.moveTab(this.#state, from, to);
  }

  cycle(delta: 1 | -1) {
    this.#state = ops.cycleTab(this.#state, delta);
  }

  selectDigit(digit: number): boolean {
    const index = ops.tabIndexFromDigit(digit, this.#state.tabs.length);
    if (index === null) return false;
    this.select(this.#state.tabs[index].id);
    return true;
  }

  update(patch: Partial<Pick<Tab, "viewMode" | "selectedEntryPath">>) {
    this.#state = ops.updateActive(this.#state, patch);
  }
}
