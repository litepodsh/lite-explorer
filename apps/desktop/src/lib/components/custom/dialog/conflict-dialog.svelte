<script lang="ts">
  // Name-collision decision for one item: Skip, Keep both or Replace. Replacing a
  // folder needs a hold-to-confirm step. Closing without a choice is Skip.
  import { formatDate } from "$lib/components/custom/preview/format.js";
  import DialogButton from "./dialog-button.svelte";
  import DialogShell from "./dialog-shell.svelte";
  import HoldToConfirm from "./hold-to-confirm.svelte";

  type Resolution = "replace" | "keepBoth" | "skip";

  let {
    name,
    isDirectory,
    destination,
    existingModified = null,
    onresolve,
  }: {
    name: string;
    isDirectory: boolean;
    destination: string;
    existingModified?: number | null;
    onresolve: (resolution: Resolution) => void;
  } = $props();

  let open = $state(true);
  let step = $state<"choose" | "confirm">("choose");
  let settled = false;

  function settle(resolution: Resolution) {
    if (settled) return;
    settled = true;
    open = false;
    onresolve(resolution);
  }

  // Escape or any other dismissal never overwrites.
  $effect(() => {
    if (!open) settle("skip");
  });

  const title = $derived(
    step === "confirm" ? `Replace “${name}” and everything inside it?` : `“${name}” already exists`,
  );
  const description = $derived(
    step === "confirm"
      ? "The existing folder is removed after the new one is fully extracted."
      : `In ${destination}${existingModified ? `, modified ${formatDate(existingModified)}` : ""}.`,
  );

  function replace() {
    if (isDirectory) step = "confirm";
    else settle("replace");
  }
</script>

<DialogShell bind:open {title} {description}>
  {#snippet footer()}
    {#if step === "choose"}
      <DialogButton onclick={() => settle("skip")}>Skip</DialogButton>
      <DialogButton onclick={replace}>Replace</DialogButton>
      <DialogButton variant="primary" onclick={() => settle("keepBoth")}>Keep both</DialogButton>
    {:else}
      <DialogButton onclick={() => (step = "choose")}>Back</DialogButton>
      <HoldToConfirm
        label="Hold to replace"
        doneLabel="Replacing…"
        variant="destructive"
        onconfirm={() => settle("replace")} />
    {/if}
  {/snippet}
</DialogShell>
