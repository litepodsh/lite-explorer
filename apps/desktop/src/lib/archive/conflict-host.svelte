<script lang="ts">
  import { ConflictDialog } from "$lib/components/custom/dialog/index.js";
  import { extraction } from "./extraction.svelte.js";
</script>

{#if extraction.conflicts[0]}
  {@const conflict = extraction.conflicts[0]}
  {#key conflict}
    <ConflictDialog
      name={conflict.target.name}
      isDirectory={conflict.target.isDirectory}
      destination={conflict.destination}
      existingModified={conflict.target.existingModified}
      onresolve={conflict.resolve} />
  {/key}
{/if}
