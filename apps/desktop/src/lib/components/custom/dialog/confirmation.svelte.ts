import type { Component } from "svelte";

export type ConfirmRequest = {
  title: string;
  description?: string;
  confirmLabel?: string;
  pendingLabel?: string;
  variant?: "destructive" | "primary";
  icon?: Component;
  /** Physical keys that drive the hold-to-confirm button while the sheet is open. */
  globalHoldKeys?: string[];
  /** Runs after the hold completes. Throw to keep the sheet open with the error shown. */
  onconfirm: () => void | Promise<void>;
};

/** App-wide confirmation sheet, rendered once by `<ConfirmHost />`. */
class Confirmation {
  open = $state(false);
  request = $state.raw<ConfirmRequest | null>(null);

  ask(request: ConfirmRequest) {
    this.request = request;
    this.open = true;
  }
}

export const confirmation = new Confirmation();
