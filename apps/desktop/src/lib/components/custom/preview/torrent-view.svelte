<script lang="ts">
  import FileIcon from "@lucide/svelte/icons/file";
  import Lock from "@lucide/svelte/icons/lock";
  import { formatSize } from "./format.js";
  import { openTorrent, type TorrentPreview } from "./torrent.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let torrent = $state.raw<TorrentPreview | null>(null);
  let error = $state("");
  let token = 0;

  const created = $derived(
    torrent?.creationDate ? new Date(torrent.creationDate * 1000).toLocaleString() : "",
  );

  $effect(() => {
    const target = path;
    const request = ++token;
    torrent = null;
    error = "";
    openTorrent(target)
      .then((result) => {
        if (request === token) torrent = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[#1f1d1b] text-[#e8e5e2]" aria-label={`Torrent preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if torrent}
    <div class="mx-auto max-w-2xl p-4">
      <header class="flex items-start gap-3">
        <span class="grid size-11 shrink-0 place-items-center rounded-lg bg-white/5">
          <FileIcon class="size-5 text-[#c0bbb5]" />
        </span>
        <div class="min-w-0 flex-1">
          <h1 class="truncate text-[15px] font-semibold">{torrent.name}</h1>
          <p class="text-[12.5px] text-[#9c9895]">
            {formatSize(torrent.totalSize)} · {torrent.files.length} archivo{torrent.files.length === 1 ? "" : "s"}
            {#if torrent.private}<span class="ml-1 inline-flex items-center gap-1 text-[#9c9895]"><Lock class="size-3" />privado</span>{/if}
          </p>
        </div>
      </header>

      <dl class="mt-4 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-[12.5px]">
        {#if torrent.announce}
          <dt class="font-medium text-[#9c9895]">Tracker</dt>
          <dd class="min-w-0 truncate">{torrent.announce}</dd>
        {/if}
        <dt class="font-medium text-[#9c9895]">Piezas</dt>
        <dd>{torrent.pieceCount} × {formatSize(torrent.pieceLength)}</dd>
        {#if torrent.createdBy}
          <dt class="font-medium text-[#9c9895]">Creado por</dt>
          <dd class="truncate">{torrent.createdBy}</dd>
        {/if}
        {#if created}
          <dt class="font-medium text-[#9c9895]">Fecha</dt>
          <dd>{created}</dd>
        {/if}
        {#if torrent.comment}
          <dt class="font-medium text-[#9c9895]">Comentario</dt>
          <dd class="whitespace-pre-wrap">{torrent.comment}</dd>
        {/if}
      </dl>

      {#if torrent.files.length > 1}
        <table class="mt-4 w-full text-left text-[12.5px]">
          <thead class="text-[11px] text-[#67635f]">
            <tr><th class="py-1 font-medium">Archivo</th><th class="py-1 text-right font-medium">Tamaño</th></tr>
          </thead>
          <tbody>
            {#each torrent.files as file (file.path)}
              <tr class="border-t border-[#3a3734]">
                <td class="min-w-0 truncate py-1 pr-3">{file.path}</td>
                <td class="py-1 text-right whitespace-nowrap text-[#9c9895]">{formatSize(file.size)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  {/if}
</div>
