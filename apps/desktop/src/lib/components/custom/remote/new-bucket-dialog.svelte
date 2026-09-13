<script lang="ts">
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import GlobeIcon from "@lucide/svelte/icons/globe";
  import { DialogButton, DialogShell, DialogSwitch, HoldToConfirm } from "$lib/components/custom/dialog/index.js";
  import { createRemoteBucket, remoteProvider, type CreatedBucket, type Provider } from "$lib/remote/remote-locations.js";

  let {
    open = $bindable(false),
    location,
    onCreated,
  }: {
    open?: boolean;
    /** Path of the account location whose bucket list is showing. */
    location: string;
    onCreated: (result: CreatedBucket) => void;
  } = $props();

  let input = $state<HTMLInputElement | null>(null);
  let name = $state("");
  let versioning = $state(false);
  let publicRead = $state(false);
  let provider = $state<Provider | null>(null);
  let error = $state("");
  let creating = $state(false);
  // Remounts the hold button so it can be held again after a failure.
  let attempt = $state(0);
  let isR2 = $derived(provider === "r2");
  let valid = $derived(name.trim().length > 0);

  $effect(() => {
    if (!open) {
      name = "";
      versioning = false;
      publicRead = false;
      error = "";
      creating = false;
      return;
    }
    const path = location;
    void remoteProvider(path)
      .then((value) => {
        if (open && location === path) provider = value;
      })
      .catch(() => (provider = null));
  });

  async function create() {
    if (creating || !valid) return;
    creating = true;
    error = "";
    try {
      const result = await createRemoteBucket(location, name.trim(), {
        versioning: !isR2 && versioning,
        public: !isR2 && publicRead,
      });
      open = false;
      onCreated(result);
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
      attempt += 1;
    } finally {
      creating = false;
    }
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    // Public buckets are only created through the hold button.
    if (!(publicRead && !isR2)) void create();
  }
</script>

<DialogShell bind:open title="New bucket" initialFocus={input}>
  <form class="form" id="new-bucket-form" onsubmit={submit}>
    <label for="new-bucket-name">Name</label>
    <input
      id="new-bucket-name"
      bind:this={input}
      bind:value={name}
      oninput={() => (error = "")}
      placeholder="media-uploads"
      spellcheck="false"
      autocomplete="off"
      disabled={creating} />
    <p class="help">
      {isR2
        ? "Lowercase letters, numbers and hyphens, 3 to 63 characters."
        : "Lowercase letters, numbers, hyphens and dots, 3 to 63 characters."}
    </p>

    {#if provider && !isR2}
      <div class="options">
        <DialogSwitch
          label="Versioning"
          description="Keep previous versions of objects when they change or are deleted."
          checked={versioning}
          disabled={creating}
          onchange={(value) => (versioning = value)} />
        <DialogSwitch
          label="Public read access"
          description="Anyone with an object’s URL can download it."
          checked={publicRead}
          disabled={creating}
          onchange={(value) => (publicRead = value)} />
      </div>
    {:else if isR2}
      <p class="note">New R2 buckets are private. Public access is set up in the Cloudflare dashboard.</p>
    {/if}

    {#if error}
      <p class="error" role="alert"><CircleAlertIcon /> {error}</p>
    {/if}
  </form>
  {#snippet footer()}
    <DialogButton disabled={creating} onclick={() => (open = false)}>Cancel</DialogButton>
    {#if publicRead && !isR2}
      {#key attempt}
        <HoldToConfirm
          label="Hold to create public"
          doneLabel="Creating…"
          variant="primary"
          icon={GlobeIcon}
          disabled={creating || !valid}
          onconfirm={() => void create()} />
      {/key}
    {:else}
      <DialogButton type="submit" form="new-bucket-form" variant="primary" disabled={creating || !valid}>
        {creating ? "Creating…" : "Create"}
      </DialogButton>
    {/if}
  {/snippet}
</DialogShell>

<style>
  .form {
    display: grid;
    margin-top: 6px;
  }
  label {
    margin-bottom: 6px;
    color: #a8a4a1;
    font-size: 11.5px;
    font-weight: 500;
  }
  input {
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    outline: none;
    background: #201e1d;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.07);
    color: #eceae8;
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 12px;
    transition: box-shadow 180ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  input::placeholder {
    color: #8a8683;
  }
  input:focus {
    box-shadow:
      inset 0 0 0 1px rgb(10 155 255 / 0.9),
      0 0 0 3px rgb(10 155 255 / 0.18);
  }
  .help,
  .note {
    margin: 5px 0 0;
    color: #8f8b88;
    font-size: 11px;
    line-height: 1.4;
  }
  .note {
    margin-top: 14px;
  }
  .options {
    display: grid;
    gap: 14px;
    margin-top: 16px;
    padding-top: 14px;
    border-top: 1px solid rgb(255 255 255 / 0.06);
  }
  .error {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 12px 0 0;
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
