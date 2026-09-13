<script lang="ts">
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import GlobeIcon from "@lucide/svelte/icons/globe";
  import { DialogButton, DialogShell, DialogSwitch, HoldToConfirm } from "$lib/components/custom/dialog/index.js";
  import {
    getBucketSettings,
    setBucketPublic,
    setBucketVersioning,
    type BucketSettings,
  } from "$lib/remote/remote-locations.js";

  let {
    open = $bindable(false),
    path,
    name,
  }: {
    open?: boolean;
    /** Remote path of the bucket, `s3://<id>/<bucket>/`. */
    path: string;
    name: string;
  } = $props();

  let settings = $state<BucketSettings | null>(null);
  let loading = $state(false);
  let saving = $state<"versioning" | "public" | null>(null);
  let confirmingPublic = $state(false);
  let error = $state("");
  // Drops responses for a bucket that is no longer shown.
  let token = 0;

  $effect(() => {
    const current = ++token;
    settings = null;
    error = "";
    confirmingPublic = false;
    if (!open) return;
    loading = true;
    const bucket = path;
    getBucketSettings(bucket)
      .then((result) => {
        if (current === token) settings = result;
      })
      .catch((reason) => {
        if (current === token) error = message(reason);
      })
      .finally(() => {
        if (current === token) loading = false;
      });
  });

  function message(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
  }

  async function save(kind: "versioning" | "public", request: () => Promise<BucketSettings>) {
    const current = token;
    saving = kind;
    error = "";
    try {
      const result = await request();
      if (current === token) settings = result;
    } catch (reason) {
      if (current === token) error = message(reason);
    } finally {
      if (current === token) saving = null;
    }
  }

  function toggleVersioning(enabled: boolean) {
    void save("versioning", () => setBucketVersioning(path, enabled));
  }

  function togglePublic(makePublic: boolean) {
    if (makePublic) {
      confirmingPublic = true;
      return;
    }
    confirmingPublic = false;
    void save("public", () => setBucketPublic(path, false));
  }

  async function confirmPublic() {
    await save("public", () => setBucketPublic(path, true));
    confirmingPublic = false;
  }

  const versioningHelp = $derived.by(() => {
    switch (settings?.versioning) {
      case "enabled":
        return "Previous versions are kept. Turning this off suspends versioning; existing versions stay.";
      case "suspended":
        return "Suspended. Existing versions are kept, new changes overwrite objects.";
      default:
        return "Keep previous versions of objects when they change or are deleted.";
    }
  });

  const publicHelp = $derived.by(() => {
    const access = settings?.publicAccess;
    if (access?.publicElsewhere) return "Public through a bucket policy statement added outside this app.";
    if (access?.public) return "Anyone with an object’s URL can download it.";
    return "Only these credentials and others you grant can read objects.";
  });
</script>

<DialogShell bind:open title={`“${name}” settings`} width={440}>
  {#if loading}
    <p class="muted" aria-live="polite">Checking bucket settings…</p>
  {:else if settings}
    <div class="rows">
      {#if settings.versioning !== null}
        <DialogSwitch
          label="Versioning"
          description={versioningHelp}
          checked={settings.versioning === "enabled"}
          disabled={saving !== null}
          onchange={toggleVersioning} />
      {/if}

      {#if settings.publicAccess !== null}
        <DialogSwitch
          label="Public read access"
          description={publicHelp}
          checked={settings.publicAccess.public || confirmingPublic}
          disabled={saving !== null || settings.publicAccess.publicElsewhere}
          onchange={togglePublic} />
        {#if confirmingPublic}
          <div class="confirm">
            <p>Every object in “{name}” becomes readable by anyone with its URL.</p>
            <div class="confirm-actions">
              <DialogButton disabled={saving !== null} onclick={() => (confirmingPublic = false)}>Keep private</DialogButton>
              <HoldToConfirm
                label="Hold to make public"
                doneLabel="Updating…"
                variant="primary"
                icon={GlobeIcon}
                disabled={saving !== null}
                onconfirm={() => void confirmPublic()} />
            </div>
          </div>
        {/if}
      {/if}

      {#if settings.provider === "r2"}
        <p class="muted">
          R2 doesn’t support versioning. Public access for R2 buckets is set up in the Cloudflare dashboard.
        </p>
      {:else if settings.versioning === null && settings.publicAccess === null}
        <p class="muted">This service doesn’t expose versioning or public access through the S3 API.</p>
      {/if}
    </div>
  {/if}

  {#if error}
    <p class="error" role="alert"><CircleAlertIcon /> {error}</p>
  {/if}

  {#snippet footer()}
    <DialogButton variant="primary" onclick={() => (open = false)}>Done</DialogButton>
  {/snippet}
</DialogShell>

<style>
  .rows {
    display: grid;
    gap: 16px;
    margin-top: 8px;
  }
  .muted {
    margin: 4px 0 0;
    color: #8f8b88;
    font-size: 11.5px;
    line-height: 1.45;
  }
  .confirm {
    display: grid;
    gap: 10px;
    margin-top: -4px;
    padding: 10px 12px;
    border-radius: 9px;
    background: rgb(10 155 255 / 0.08);
    box-shadow: inset 0 0 0 1px rgb(10 155 255 / 0.2);
    animation: confirm-in 220ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .confirm p {
    margin: 0;
    color: #cfe8ff;
    font-size: 11.5px;
    line-height: 1.4;
  }
  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
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
  @keyframes confirm-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .confirm {
      animation: none;
    }
  }
</style>
