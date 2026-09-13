<script lang="ts">
  import ServerIcon from "@lucide/svelte/icons/server";
  import { LOCATION_TYPES, type LocationKind } from "./location-types.js";

  let { kind }: { kind: LocationKind } = $props();
</script>

<span class="loc-mark {kind}" aria-hidden="true">
  {#if kind === "custom"}<ServerIcon />{:else}{LOCATION_TYPES[kind].mark}{/if}
</span>
