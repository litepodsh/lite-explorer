<script lang="ts">
  import ServerIcon from "@lucide/svelte/icons/server";
  import { LOCATION_TYPES, type LocationKind } from "./location-types.js";

  let { kind }: { kind: LocationKind } = $props();
</script>

<span class="loc-mark {kind}" aria-hidden="true">
  {#if kind === "custom"}<ServerIcon />{:else if kind === "gdrive"}<img src="/google-drive.svg" alt="" />{:else if kind === "onedrive"}<img src="/onedrive.svg" alt="" />{:else if kind === "dropbox"}<img src="/dropbox.svg" alt="" />{:else if kind === "azblob"}<img src="/azure.svg" alt="" />{:else}{LOCATION_TYPES[kind].mark}{/if}
</span>
