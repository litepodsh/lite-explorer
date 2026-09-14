<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { appMenu } from "$lib/menu/app-menu.svelte.js";
</script>

<Dialog.Root
  open={appMenu.about !== null}
  onOpenChange={(open) => {
    if (!open) appMenu.about = null;
  }}>
  <Dialog.Content class="about-dialog">
    {#if appMenu.about}
      <img src="/app-icon.png" alt="" width="64" height="64" />
      <Dialog.Title>{appMenu.about.name}</Dialog.Title>
      <p class="about-version">Version {appMenu.about.version}</p>
      <Dialog.Description>{appMenu.about.description}</Dialog.Description>
      {#if appMenu.about.copyright}<p class="about-copyright">{appMenu.about.copyright}</p>{/if}
    {/if}
  </Dialog.Content>
</Dialog.Root>
