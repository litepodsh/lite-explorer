<script lang="ts">
  // Asks for a network location's password when none is saved or the saved one was rejected.
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import { DialogButton, DialogShell, DialogSwitch } from "$lib/components/custom/dialog/index.js";
  import { formatLocationUrl } from "$lib/remote/location-url.js";
  import { getNetworkLocation } from "$lib/remote/network-locations.js";
  import type { Location } from "$lib/tabs/tabs.js";
  import "./add-location.css";

  let {
    open = $bindable(false),
    location,
    onSubmit,
  }: {
    open?: boolean;
    location: Location | null;
    /** Connects with the password. Resolves to an error message, or null when connected. */
    onSubmit: (username: string, password: string, remember: boolean) => Promise<string | null>;
  } = $props();

  let field = $state<HTMLInputElement | null>(null);
  let username = $state("");
  let address = $state("");
  let password = $state("");
  let remember = $state(false);
  let error = $state("");
  let busy = $state(false);
  let attempt = $state(0);

  $effect(() => {
    const path = location?.path;
    if (!open || !path) return;
    password = "";
    remember = false;
    error = "";
    username = "";
    address = "";
    void getNetworkLocation(path)
      .then((input) => {
        username = input.username;
        address = formatLocationUrl(input);
      })
      .catch(() => {});
  });

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!password || busy) return;
    busy = true;
    error = "";
    const failure = await onSubmit(username, password, remember);
    busy = false;
    if (failure) {
      error = failure;
      password = "";
      attempt++;
      field?.focus();
    } else {
      open = false;
    }
  }
</script>

<DialogShell bind:open title={`Connect to “${location?.name ?? ""}”`} initialFocus={field}>
  {#key attempt}
    <form class="loc prompt" class:shake={attempt > 0} id="network-password-form" onsubmit={submit}>
      {#if address}<p class="loc-url">{address}</p>{/if}
      <div>
        <label class="loc-label" for="network-password-user">Username</label>
        <input
          id="network-password-user"
          class="loc-input"
          placeholder="DOMAIN\\username"
          autocomplete="username"
          bind:value={username}
          disabled={busy} />
      </div>
      <div>
        <label class="loc-label" for="network-password">Password</label>
        <input
          id="network-password"
          class="loc-input"
          type="password"
          autocomplete="off"
          bind:this={field}
          bind:value={password}
          oninput={() => (error = "")}
          disabled={busy} />
      </div>
      <DialogSwitch
        label="Remember password"
        description="Saved in this computer’s password store."
        checked={remember}
        disabled={busy}
        onchange={(value) => (remember = value)} />
      {#if error}
        <p class="loc-status error" role="alert"><CircleAlertIcon /> {error}</p>
      {/if}
    </form>
  {/key}
  {#snippet footer()}
    <DialogButton disabled={busy} onclick={() => (open = false)}>Cancel</DialogButton>
    <DialogButton type="submit" form="network-password-form" variant="primary" disabled={busy || !password}>
      {busy ? "Connecting…" : "Connect"}
    </DialogButton>
  {/snippet}
</DialogShell>

<style>
  .prompt {
    display: grid;
    gap: 12px;
    margin-top: 4px;
  }
  .shake {
    animation: prompt-shake 360ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  @keyframes prompt-shake {
    20%,
    60% {
      transform: translateX(-4px);
    }
    40%,
    80% {
      transform: translateX(4px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .shake {
      animation: none;
    }
  }
</style>
