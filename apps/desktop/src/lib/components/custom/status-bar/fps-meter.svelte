<script lang="ts">
  const SAMPLE_MS = 500;

  let fps = $state(0);
  let worstMs = $state(0);

  $effect(() => {
    let raf = 0;
    let frames = 0;
    let windowStart = performance.now();
    let last = windowStart;
    let worst = 0;

    const tick = (now: number) => {
      const delta = now - last;
      last = now;
      frames++;
      if (delta > worst) worst = delta;

      const elapsed = now - windowStart;
      if (elapsed >= SAMPLE_MS) {
        fps = Math.round((frames * 1000) / elapsed);
        worstMs = worst;
        frames = 0;
        worst = 0;
        windowStart = now;
      }
      raf = requestAnimationFrame(tick);
    };

    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  const color = $derived(fps >= 55 ? "text-green-400" : fps >= 30 ? "text-yellow-400" : "text-red-400");
</script>

<span
  class="shrink-0 rounded bg-[#1b1a18] px-1.5 font-mono tabular-nums text-[10px] text-[#5c5854]"
  title="Frames per second (requestAnimationFrame) · worst frame time in last {SAMPLE_MS}ms">
  <span class={color}>{fps}</span> fps<span class="px-1">·</span>{worstMs.toFixed(1)}ms
</span>
