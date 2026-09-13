<script lang="ts">
  import ConfirmDialog from "./confirm-dialog.svelte";
  import { confirmation } from "./confirmation.svelte.js";
</script>

{#if confirmation.request}
  {@const request = confirmation.request}
  <ConfirmDialog bind:open={confirmation.open} {...request} />
{/if}
