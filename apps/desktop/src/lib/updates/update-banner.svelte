<script lang="ts">
  import DownloadIcon from "@lucide/svelte/icons/download";
  import XIcon from "@lucide/svelte/icons/x";
  import { formatSize } from "$lib/components/custom/preview/format.js";
  import { updates } from "./updates.svelte.js";
  import { whatsNew } from "./whats-new.svelte.js";

  let status = $derived(updates.status);
  let visible = $derived(
    !updates.dismissed &&
      (status.state === "ready" ||
        status.state === "installing" ||
        (status.state === "downloading" && updates.manual)),
  );
  let progress = $derived(
    status.state === "downloading" && status.total
      ? `${Math.round((status.downloaded / status.total) * 100)}%`
      : status.state === "downloading"
        ? formatSize(status.downloaded)
        : "",
  );
</script>

{#if visible && status.state !== "idle" && status.state !== "checking"}
  <div class="update-banner" role="status" aria-live="polite">
    <DownloadIcon class="size-4 shrink-0 text-[#9c9895]" />
    <span class="update-text">
      {#if status.state === "downloading"}
        Downloading Lite Explorer {status.version}… <span class="tabular-nums">{progress}</span>
      {:else}
        Lite Explorer {status.version} is ready
      {/if}
    </span>
    {#if status.state !== "downloading"}
      <button
        class="update-notes"
        onclick={() => whatsNew.showForUpdate(status.version, updates.notes)}
        >What’s New</button>
      <button
        class="update-restart"
        disabled={status.state === "installing"}
        onclick={() => void updates.restart()}
        >{status.state === "installing" ? "Restarting…" : "Restart to Update"}</button>
    {/if}
    <button class="update-dismiss" aria-label="Hide update notice" onclick={() => updates.dismiss()}>
      <XIcon class="size-3.5" />
    </button>
  </div>
{/if}

<style>
  .update-banner {
    position: fixed;
    right: 16px;
    bottom: 40px;
    z-index: 40;
    display: flex;
    max-width: calc(100vw - 32px);
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 12px;
    border-radius: 10px;
    background: #2b2927;
    box-shadow:
      inset 0 0 0 1px rgb(255 255 255 / 0.08),
      0 12px 32px rgb(0 0 0 / 0.35);
    color: #e8e5e2;
    font-size: 12px;
    animation: update-in 320ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .update-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .update-notes {
    flex-shrink: 0;
    padding: 4px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.14);
    color: #cfccc9;
    font-size: 12px;
    font-weight: 500;
  }
  .update-notes:hover {
    background: #3a3734;
    color: #e8e5e2;
  }
  .update-notes:active {
    transform: scale(0.98);
  }
  .update-restart {
    flex-shrink: 0;
    padding: 4px 10px;
    border: 0;
    border-radius: 6px;
    background: #e8e5e2;
    color: #1f1d1b;
    font-size: 12px;
    font-weight: 500;
    transition: transform 120ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .update-restart:active:not(:disabled) {
    transform: scale(0.98);
  }
  .update-restart:disabled {
    opacity: 0.6;
  }
  .update-dismiss {
    display: grid;
    width: 22px;
    height: 22px;
    flex-shrink: 0;
    place-items: center;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: #9c9895;
  }
  .update-dismiss:hover {
    background: #3a3734;
    color: #e8e5e2;
  }
  .update-notes:focus-visible,
  .update-restart:focus-visible,
  .update-dismiss:focus-visible {
    outline: 2px solid rgb(255 255 255 / 0.32);
    outline-offset: 2px;
  }
  @keyframes update-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .update-banner {
      animation: none;
    }
  }
</style>
