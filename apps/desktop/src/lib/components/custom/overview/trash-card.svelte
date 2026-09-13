<script lang="ts">
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import FolderOpenIcon from "@lucide/svelte/icons/folder-open";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { formatSize } from "$lib/components/custom/preview/format.js";

  let {
    trashBytes,
    scanning,
    os,
    onEmptied,
    onDetected,
  }: {
    /** Size of the OS trash, or null when it can’t be measured on this platform. */
    trashBytes: number | null;
    /** True while the home folder scan is running and the size is still warming up. */
    scanning: boolean;
    /** Current platform from `os_detection`, used to word the permission hint. */
    os: string | null;
    /** Called with the fresh trash size after a successful empty. */
    onEmptied: (bytes: number | null) => void;
    /** Called with a freshly measured size (e.g. after access is granted). */
    onDetected: (bytes: number | null) => void;
  } = $props();

  let emptying = $state(false);
  let detecting = $state(false);
  let opening = $state(false);
  let error = $state("");
  // Guards the direct probe below so it runs a single time instead of re-firing every time
  // `detecting` toggles (which would otherwise loop once a denied trash keeps reporting `null`).
  let probed = false;

  const size = $derived(trashBytes === null ? null : trashBytes);

  // When the size can’t be read, point macOS users at Full Disk Access, which the trash
  // (especially trash on mounted volumes) lives behind.
  const unavailableHint = $derived(
    os === "macos"
      ? "Grant Full Disk Access in System Settings › Privacy & Security, then refresh the overview, so liteexplorer can read the Trash."
      : "Trash size isn’t available on this system.",
  );

  const showPermissionButton = $derived(size === null && os === "macos");

  // Once the macOS scan settles with no size, probe the trash directly — if Full Disk Access was
  // just granted (or never needed) this fills in the real number without a full home re-scan.
  $effect(() => {
    if (!showPermissionButton || scanning || detecting || probed) return;
    probed = true;
    detecting = true;
    invoke<number | null>("trash_usage")
      .then((bytes) => onDetected(bytes))
      .catch(() => {})
      .finally(() => (detecting = false));
  });

  async function openTrash() {
    if (opening) return;
    opening = true;
    error = "";
    try {
      await invoke("open_trash");
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
    } finally {
      opening = false;
    }
  }

  async function requestAccess() {
    if (detecting) return;
    detecting = true;
    error = "";
    try {
      // Touching the protected locations makes macOS show its native prompt and register the app.
      await invoke("request_full_disk_access");
      const bytes = await invoke<number | null>("trash_usage");
      onDetected(bytes);
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
    } finally {
      detecting = false;
    }
  }

  async function openSettings() {
    try {
      await invoke("open_system_settings");
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
    }
  }

  async function empty() {
    if (emptying) return;
    const confirmed = await ask("Empty the trash? This deletes everything in it and can’t be undone.", {
      title: "Empty trash",
      kind: "warning",
    });
    if (!confirmed) return;
    emptying = true;
    error = "";
    try {
      const bytes = await invoke<number | null>("empty_trash");
      onEmptied(bytes);
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
    } finally {
      emptying = false;
    }
  }
</script>

<article class="overflow-hidden rounded-xl border border-[#3a3734] bg-[#2d2a28]">
  <header class="flex items-center gap-3 border-b border-[#3a3734] px-4 py-3.5">
    <Trash2Icon class="size-7 shrink-0 stroke-[1.4] text-amber-400" />
    <div class="min-w-0">
      <h2 class="text-[15px] font-semibold text-[#f2f1f0]">Trash</h2>
      <p class="mt-0.5 truncate text-xs text-[#9c9895]">Files you’ve deleted</p>
    </div>
    <button
      class="ml-auto flex shrink-0 items-center gap-1.5 rounded-md border border-[#3a3734] bg-transparent px-2.5 py-1.5 text-xs font-medium text-[#d9d6d3] hover:bg-[#353230] hover:text-[#f2f1f0] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[#0a9bff] disabled:cursor-not-allowed disabled:opacity-40"
      onclick={openTrash}
      disabled={opening || (size !== null && size === 0)}
      title="Open the trash so you can empty it manually">
      {#if opening}
        <LoaderCircleIcon class="size-3.5 animate-spin text-[#0a9bff] motion-reduce:animate-none" />
        <span>Opening…</span>
      {:else}
        <FolderOpenIcon class="size-3.5" />
        <span>Open</span>
      {/if}
    </button>
    <button
      class="flex shrink-0 items-center gap-1.5 rounded-md border border-[#3a3734] bg-transparent px-2.5 py-1.5 text-xs font-medium text-[#d9d6d3] hover:bg-[#353230] hover:text-[#f2f1f0] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[#0a9bff] disabled:cursor-not-allowed disabled:opacity-40"
      onclick={empty}
      disabled={emptying || size === null || size === 0}
      title={size === 0 ? "Trash is already empty" : "Empty the trash"}>
      {#if emptying}
        <LoaderCircleIcon class="size-3.5 animate-spin text-[#0a9bff] motion-reduce:animate-none" />
        <span>Emptying…</span>
      {:else}
        <Trash2Icon class="size-3.5" />
        <span>Empty</span>
      {/if}
    </button>
  </header>

  <div class="grid p-1.5">
    {#if error}
      <p class="px-2.5 py-2 text-xs text-[#ff8a80]">{error}</p>
    {:else if size === null}
      <div class="grid gap-2 px-2.5 py-3">
        <p class="text-xs text-[#9c9895]">
          {scanning ? "Measuring your trash…" : unavailableHint}
        </p>
        {#if showPermissionButton}
          <div class="flex flex-wrap items-center gap-2">
            <button
              class="flex w-fit items-center gap-1.5 rounded-md border border-[#0a9bff] bg-[#0a9bff]/10 px-2.5 py-1.5 text-xs font-medium text-[#7fd0ff] hover:bg-[#0a9bff]/20 focus-visible:outline focus-visible:outline-2 focus-visible:outline-[#0a9bff] disabled:opacity-40"
              onclick={requestAccess}
              disabled={detecting}>
              {detecting ? "Checking…" : "Request access"}
            </button>
            <button
              class="flex w-fit items-center gap-1.5 rounded-md border border-[#3a3734] bg-transparent px-2.5 py-1.5 text-xs font-medium text-[#d9d6d3] hover:bg-[#353230] hover:text-[#f2f1f0] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[#0a9bff]"
              onclick={openSettings}>
              Open System Settings
            </button>
          </div>
        {/if}
      </div>
    {:else}
      <div class="grid grid-cols-[18px_minmax(0,1fr)_76px] items-center gap-x-2.5 px-2.5 py-3">
        <Trash2Icon class="size-4 stroke-[1.8] {size > 0 ? 'text-amber-400' : 'text-[#67635f]'}" />
        <span class="truncate text-[13px] text-[#d9d6d3]">
          {size > 0 ? "Taking up space" : "Empty"}
        </span>
        <span class="text-right font-mono text-xs tabular-nums text-[#d9d6d3]">{formatSize(size)}</span>
      </div>
    {/if}
  </div>
</article>
