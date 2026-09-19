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

<div class="h-full min-h-0 overflow-auto bg-[#1f1d1b] text-[#e8e5e2]" aria-label={`Capture preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if pcap}
    <p class="sticky top-0 z-10 border-b border-[#3a3734] bg-[#242220] px-3 py-1.5 text-[11px] text-[#9c9895]">
      {pcap.linkType} · {pcap.packets.length} paquetes{pcap.truncated ? " (truncado)" : ""}
    </p>
    <table class="w-full text-left text-[12px]">
      <thead class="sticky top-7 z-10 bg-[#242220] text-[10.5px] text-[#67635f]">
        <tr>
          <th class="px-3 py-1.5 font-medium">#</th>
          <th class="px-3 py-1.5 font-medium">Hora</th>
          <th class="px-3 py-1.5 font-medium">Protocolo</th>
          <th class="px-3 py-1.5 text-right font-medium">Bytes</th>
        </tr>
      </thead>
      <tbody>
        {#each pcap.packets as packet (packet.index)}
          <tr class="border-t border-[#2c2a28]">
            <td class="px-3 py-1 text-[#67635f]">{packet.index}</td>
            <td class="px-3 py-1 font-mono">{time(packet.timeMs)}</td>
            <td class="px-3 py-1">
              {#if packet.protocol}
                <span class="rounded bg-white/5 px-1.5 py-0.5 text-[11px] text-[#9fb8e8]">{packet.protocol}</span>
              {/if}
            </td>
            <td class="px-3 py-1 text-right text-[#9c9895]">{formatSize(packet.length)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
