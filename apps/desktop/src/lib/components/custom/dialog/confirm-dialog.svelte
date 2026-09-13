<script lang="ts">
  // Confirmation sheet with a hold-to-confirm action. Stays open while `onconfirm`
  // runs, closes when it resolves and shows the error inline when it throws.
  import type { Component } from "svelte";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import DialogButton from "./dialog-button.svelte";
  import DialogShell from "./dialog-shell.svelte";
  import HoldToConfirm from "./hold-to-confirm.svelte";

  let {
    open = $bindable(false),
    title,
    description,
    confirmLabel = "Delete",
    pendingLabel = "Deleting…",
    variant = "destructive",
    icon,
    globalHoldKeys = [],
    onconfirm,
  }: {
    open?: boolean;
    title: string;
    description?: string;
    /** Verb for the action; the button reads "Hold to <verb>". */
    confirmLabel?: string;
    pendingLabel?: string;
    variant?: "destructive" | "primary";
    icon?: Component;
    /** Physical keys that drive the hold-to-confirm button while the sheet is open. */
    globalHoldKeys?: string[];
    onconfirm: () => void | Promise<void>;
  } = $props();

  let pending = $state(false);
  let error = $state("");
  // Remounts the hold button so it can be held again after a failure.
  let attempt = $state(0);

  $effect(() => {
    if (open) return;
    pending = false;
    error = "";
  });

  async function confirm() {
    if (pending) return;
    pending = true;
    error = "";
    try {
      await onconfirm();
      open = false;
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
      attempt += 1;
    } finally {
      pending = false;
    }
  }
</script>

<DialogShell bind:open {title} {description}>
  {#if error}
    <p class="error" role="alert"><CircleAlertIcon /> {error}</p>
  {/if}
  {#snippet footer()}
    <DialogButton disabled={pending} onclick={() => (open = false)}>Cancel</DialogButton>
    {#key attempt}
      <HoldToConfirm
        label={`Hold to ${confirmLabel.toLowerCase()}`}
        doneLabel={pendingLabel}
        {variant}
        {icon}
        {globalHoldKeys}
        disabled={pending}
        onconfirm={() => void confirm()} />
    {/key}
  {/snippet}
</DialogShell>

<style>
  .error {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 4px 0 0;
    color: #ff6b5e;
    font-size: 11.5px;
    line-height: 1.4;
  }
  .error :global(svg) {
    flex-shrink: 0;
    width: 13px;
    height: 13px;
    margin-top: 1px;
  }
</style>
