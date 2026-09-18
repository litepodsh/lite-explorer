<script lang="ts">
  // The Apple Intelligence glyph: a rosette of rounded lobes in the system's spectrum gradient.
  // Drawn rather than imported so it stays crisp at every size and needs no asset.
  let { size = 16, class: className = "" } = $props<{ size?: number; class?: string }>();

  const gradientId = $props.id();
  const lobes = Array.from({ length: 8 }, (_, index) => index * 45);
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  class={className}
  aria-hidden="true"
  focusable="false">
  <defs>
    <linearGradient id={gradientId} x1="2" y1="22" x2="22" y2="2" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#4f9bff" />
      <stop offset="0.38" stop-color="#a66bff" />
      <stop offset="0.72" stop-color="#ff6bb2" />
      <stop offset="1" stop-color="#ffb86b" />
    </linearGradient>
  </defs>
  {#each lobes as angle (angle)}
    <ellipse
      cx="12"
      cy="5.5"
      rx="2.45"
      ry="4.5"
      fill="url(#{gradientId})"
      transform="rotate({angle} 12 12)" />
  {/each}
</svg>
