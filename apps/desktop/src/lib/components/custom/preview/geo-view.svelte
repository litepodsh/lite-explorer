<script lang="ts">
  import { openGeo, type GeoPreview } from "./geo.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let geo = $state.raw<GeoPreview | null>(null);
  let error = $state("");
  let token = 0;

  const view = $derived.by(() => {
    if (!geo?.bounds) return null;
    const [minLon, minLat, maxLon, maxLat] = geo.bounds;
    const spanLon = maxLon - minLon || 1e-6;
    const spanLat = maxLat - minLat || 1e-6;
    const aspect = spanLon / spanLat;
    let width = 1000;
    let height = width / aspect;
    if (height > 1000) {
      height = 1000;
      width = height * aspect;
    }
    const padLon = spanLon * 0.05;
    const padLat = spanLat * 0.05;
    const project = ([lon, lat]: [number, number]): [number, number] => [
      ((lon - minLon + padLon) / (spanLon + padLon * 2)) * width,
      ((maxLat + padLat - lat) / (spanLat + padLat * 2)) * height,
    ];
    return { width, height, project };
  });

  function pathOf(points: [number, number][], close: boolean): string {
    if (!view) return "";
    const commands = points.map((point, index) => {
      const [x, y] = view.project(point);
      return `${index === 0 ? "M" : "L"}${x.toFixed(1)} ${y.toFixed(1)}`;
    });
    return commands.join(" ") + (close ? " Z" : "");
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    geo = null;
    error = "";
    openGeo(target)
      .then((result) => {
        if (request === token) geo = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="flex h-full min-h-0 flex-col bg-[#1f1d1b]" aria-label={`Map preview of ${name}`}>
  {#if error}
    <div class="grid flex-1 place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if geo && view}
    <p class="shrink-0 border-b border-[#3a3734] px-3 py-1 text-[11px] text-[#9c9895]">
      {geo.format.toUpperCase()} · {geo.features.length} features · {geo.pointCount} puntos
    </p>
    <div class="min-h-0 flex-1">
      <svg class="h-full w-full" viewBox="0 0 {view.width} {view.height}" preserveAspectRatio="xMidYMid meet">
        <defs>
          <pattern id="geo-grid" width="50" height="50" patternUnits="userSpaceOnUse">
            <path d="M50 0H0V50" fill="none" stroke="#2c2a28" stroke-width="1" />
          </pattern>
        </defs>
        <rect width={view.width} height={view.height} fill="url(#geo-grid)" />
        {#each geo.features as feature, index (index)}
          {#if feature.kind === "Polygon"}
            <path d={pathOf(feature.points, true)} fill="#7aa2f7" fill-opacity="0.18" stroke="#7aa2f7" stroke-width="2" />
          {:else if feature.kind === "LineString"}
            <path d={pathOf(feature.points, false)} fill="none" stroke="#a3c9a8" stroke-width="2.5" stroke-linejoin="round" />
          {:else}
            {#each feature.points as point, pointIndex (pointIndex)}
              {@const projected = view.project(point)}
              <circle cx={projected[0]} cy={projected[1]} r="5" fill="#e0af68" stroke="#1f1d1b" stroke-width="1.5">
                <title>{feature.name || `${point[1].toFixed(4)}, ${point[0].toFixed(4)}`}</title>
              </circle>
            {/each}
          {/if}
        {/each}
      </svg>
    </div>
  {/if}
</div>
