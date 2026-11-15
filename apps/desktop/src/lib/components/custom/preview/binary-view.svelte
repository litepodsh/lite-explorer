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

<div class="relative h-full w-full overflow-hidden" aria-label={`Binary file ${name}`}>
  <pre class="binary-bits select-none whitespace-pre break-all p-3 text-[11px] leading-[1.2] tracking-tight" aria-hidden="true">{bits}</pre>
  <p class="binary-label">Binary · no preview</p>
</div>

<style>
  .binary-bits {
    color: color-mix(in srgb, var(--app-fg) 14%, transparent);
    font-family: var(--app-font-mono);
    mask-image: radial-gradient(ellipse at center, #000 20%, transparent 75%);
  }
  .binary-label {
    position: absolute;
    inset: 50% 0 auto;
    color: var(--app-fg-muted);
    font-family: var(--app-font-mono);
    font-size: 10.5px;
    letter-spacing: 0.08em;
    text-align: center;
    text-transform: uppercase;
    transform: translateY(-50%);
  }
</style>
