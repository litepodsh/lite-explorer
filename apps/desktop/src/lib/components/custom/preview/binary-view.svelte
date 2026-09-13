<script lang="ts">
  type Props = { name: string };
  let { name }: Props = $props();

  const COLUMNS = 80;
  const LINES = 400;

  function hash(str: string): number {
    let h = 2166136261;
    for (let i = 0; i < str.length; i++) {
      h ^= str.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    return h >>> 0;
  }

  function mulberry32(seed: number): () => number {
    let a = seed;
    return () => {
      a |= 0;
      a = (a + 0x6d2b79f5) | 0;
      let t = Math.imul(a ^ (a >>> 15), 1 | a);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  const bits = $derived.by(() => {
    const rand = mulberry32(hash(name));
    let out = "";
    for (let i = 0; i < LINES; i++) {
      for (let j = 0; j < COLUMNS; j++) {
        out += rand() < 0.5 ? "0" : "1";
      }
      out += "\n";
    }
    return out;
  });
</script>

<div class="relative h-full w-full overflow-auto bg-black">
  <div class="sticky top-0 z-10 border-b border-[#0a3d1f] bg-black/80 px-3 py-1">
    <p class="text-[11px] text-[#1f8f4d]">binary &mdash; {name}</p>
  </div>
  <pre class="select-none whitespace-pre break-all p-3 text-[12px] leading-[1.15] tracking-tight text-[#22c55e]">{bits}</pre>
</div>
