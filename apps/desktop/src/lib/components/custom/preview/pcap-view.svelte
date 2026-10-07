<script lang="ts">
  import { formatSize } from "./format.js";
  import { openPcap, type PcapPreview } from "./pcap.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let pcap = $state.raw<PcapPreview | null>(null);
  let error = $state("");
  let token = 0;

  function time(ms: number): string {
    const date = new Date(ms);
    const pad = (value: number, size = 2) => String(value).padStart(size, "0");
    return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`;
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    pcap = null;
    error = "";
    openPcap(target)
      .then((result) => {
        if (request === token) pcap = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[var(--app-input)] text-[var(--app-fg)]" aria-label={`Capture preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[var(--app-fg-muted)]">{error}</p>
    </div>
  {:else if pcap}
    <p class="sticky top-0 z-10 border-b border-[var(--app-border)] bg-[var(--app-surface)] px-3 py-1.5 text-[11px] text-[var(--app-fg-muted)]">
      {pcap.linkType} · {pcap.packets.length} paquetes{pcap.truncated ? " (truncado)" : ""}
    </p>
    <table class="w-full text-left text-[12px]">
      <thead class="sticky top-7 z-10 bg-[var(--app-surface)] text-[10.5px] text-[var(--app-fg-faint)]">
        <tr>
          <th class="px-3 py-1.5 font-medium">#</th>
          <th class="px-3 py-1.5 font-medium">Hora</th>
          <th class="px-3 py-1.5 font-medium">Protocolo</th>
          <th class="px-3 py-1.5 text-right font-medium">Bytes</th>
        </tr>
      </thead>
      <tbody>
        {#each pcap.packets as packet (packet.index)}
          <tr class="border-t border-[var(--app-border)]">
            <td class="px-3 py-1 text-[var(--app-fg-faint)]">{packet.index}</td>
            <td class="px-3 py-1 font-mono">{time(packet.timeMs)}</td>
            <td class="px-3 py-1">
              {#if packet.protocol}
                <span class="rounded bg-white/5 px-1.5 py-0.5 text-[11px] text-[#9fb8e8]">{packet.protocol}</span>
              {/if}
            </td>
            <td class="px-3 py-1 text-right text-[var(--app-fg-muted)]">{formatSize(packet.length)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
