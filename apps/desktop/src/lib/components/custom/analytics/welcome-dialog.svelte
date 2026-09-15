<script lang="ts">
  // First-run notice about anonymous crash reporting. The choice persists in the Rust
  // preference file; before 1.0 the switch is locked on and explains why.
  import DialogButton from "../dialog/dialog-button.svelte";
  import DialogShell from "../dialog/dialog-shell.svelte";
  import DialogSwitch from "../dialog/dialog-switch.svelte";
  import { analytics } from "$lib/analytics/analytics.svelte.js";

  let open = $state(true);
  let enabled = $state(analytics.enabled);

  const locked = $derived(analytics.locked);
  const description = $derived(
    locked
      ? "Reports stay on while Lite Explorer is before version 1.0. They’re anonymized and only used to fix crashes on different operating systems. You can turn them off after 1.0."
      : "Anonymized, and only used to fix crashes on different operating systems.",
  );

  // Escape or the overlay closing the dialog keeps the default (on).
  $effect(() => {
    if (!open) void analytics.completeWelcome(enabled);
  });

  function confirm() {
    open = false;
  }
</script>

<DialogShell
  bind:open
  title="Help improve Lite Explorer"
  description="Lite Explorer can send anonymous crash reports for the operating system you’re using. They help us fix crashes on Windows, macOS and Linux.">
  <DialogSwitch
    label="Send anonymous crash reports"
    {description}
    checked={enabled || locked}
    disabled={locked}
    onchange={(value) => (enabled = value)} />
  {#snippet footer()}
    <DialogButton variant="primary" onclick={confirm}>Continue</DialogButton>
  {/snippet}
</DialogShell>
