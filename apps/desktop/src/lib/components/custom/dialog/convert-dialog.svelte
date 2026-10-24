<script lang="ts">
  import FlipHorizontalIcon from "@lucide/svelte/icons/flip-horizontal-2";
  import FlipVerticalIcon from "@lucide/svelte/icons/flip-vertical-2";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
  import RotateCwIcon from "@lucide/svelte/icons/rotate-cw";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import { onDestroy } from "svelte";
  import CompareViewer from "./compare-viewer.svelte";
  import DialogButton from "./dialog-button.svelte";
  import DialogShell from "./dialog-shell.svelte";
  import DialogSwitch from "./dialog-switch.svelte";
  import type { DirectoryEntry } from "#lib/components/custom/file-list/index.js";
  import { fetchMediaUrl } from "#lib/components/custom/preview/media.js";
  import { Slider } from "#lib/components/ui/slider/index.js";
  import {
    convertImageInfo,
    estimateConvert,
    previewConvert,
    type ConvertOptions,
    type ImageFormat,
    type ImageInfo,
  } from "#lib/file-ops/convert.js";
  import {
    PERCENT_CHOICES,
    SIZE_PRESETS,
    VECTOR_SCALES,
    canPreview,
    createLatest,
    defaultState,
    formatBytes,
    formatLabel,
    hasQuality,
    hasRaster,
    hasVector,
    isValid,
    isVectorName,
    mimeType,
    percentChange,
    rotateBy,
    scaleLabel,
    sizeLine,
    toOptions,
    vectorOutputSize,
    warningsFor,
    type DialogState,
    type SizeMode,
  } from "#lib/file-ops/convert-dialog-model.js";

  let {
    open = $bindable(false),
    targets,
    writable,
    onConvert,
  }: {
    open?: boolean;
    targets: DirectoryEntry[];
    writable: ImageFormat[];
    onConvert: (options: ConvertOptions, replaceOriginal: boolean) => void;
  } = $props();

  const SIZE_MODES: { id: SizeMode; label: string }[] = [
    { id: "original", label: "Original" },
    { id: "percent", label: "Percent" },
    { id: "fit", label: "Max size" },
  ];

  let form = $state<DialogState>(defaultState([], []));
  let info = $state<ImageInfo | null>(null);
  let previewUrl = $state("");
  let convertedUrl = $state("");
  // Mirror of `convertedUrl` that effects can read without subscribing to it.
  let convertedObjectUrl = "";
  let estimate = $state<number | null>(null);
  let estimating = $state(false);
  let wasOpen = $state(false);
  let convertedFailed = $state(false);

  const latestEstimate = createLatest();
  const latestInfo = createLatest();

  const single = $derived(targets.length === 1 ? targets[0] : null);
  const valid = $derived(targets.length > 0 && isValid(form));
  const warnings = $derived(warningsFor(form, info));
  const names = $derived(targets.map((target) => target.name));
  const showScale = $derived(hasVector(names));
  const showSize = $derived(hasRaster(names));
  const vectorOutput = $derived(
    single && showScale && info ? vectorOutputSize(info.width, info.height, form.vectorScale) : null,
  );
  const originalLabel = $derived(info ? `Original · ${formatBytes(info.bytes)}` : "Original");
  const convertedLabel = $derived.by(() => {
    if (estimate === null || !info) return "Converted · calculating…";
    // An SVG is text and the result is pixels, so a percentage between them means nothing.
    const change = single && isVectorName(single.name) ? "" : percentChange(info.bytes, estimate);
    return `Converted · ${canPreview(form.format) ? "" : "≈"}${formatBytes(estimate)}${change ? ` (${change})` : ""}`;
  });

  function setConverted(url: string) {
    if (convertedObjectUrl) URL.revokeObjectURL(convertedObjectUrl);
    convertedObjectUrl = url;
    convertedUrl = url;
    convertedFailed = false;
  }

  onDestroy(() => setConverted(""));

  $effect(() => {
    if (open && !wasOpen) {
      form = defaultState(
        targets.map((target) => target.name),
        writable,
      );
      info = null;
      previewUrl = "";
      estimate = null;
    }
    if (!open && wasOpen) setConverted("");
    wasOpen = open;
  });

  // Facts about the single selected image, and its preview URL.
  $effect(() => {
    const target = open ? single : null;
    const token = latestInfo.next();
    if (!target) {
      info = null;
      previewUrl = "";
      return;
    }
    void convertImageInfo(target.path)
      .then((value) => {
        if (latestInfo.isCurrent(token)) info = value;
      })
      .catch(() => {
        if (latestInfo.isCurrent(token)) info = null;
      });
    void fetchMediaUrl(target.path)
      .then((media) => {
        if (latestInfo.isCurrent(token)) previewUrl = media.url;
      })
      .catch(() => {
        if (latestInfo.isCurrent(token)) previewUrl = "";
      });
  });

  // The converted image itself (so its size is exact), debounced; only the newest answer is used.
  // Formats the web view cannot display only get a size estimate.
  $effect(() => {
    const options = toOptions(form);
    const format = form.format;
    const target = open && valid ? single : null;
    if (!target) {
      estimate = null;
      estimating = false;
      return;
    }
    estimating = true;
    const token = latestEstimate.next();
    const timer = setTimeout(async () => {
      try {
        if (canPreview(format)) {
          const blob = await previewConvert(target.path, options, mimeType(format));
          if (!latestEstimate.isCurrent(token)) return;
          estimate = blob.size;
          setConverted(URL.createObjectURL(blob));
        } else {
          const value = await estimateConvert(target.path, options);
          if (!latestEstimate.isCurrent(token)) return;
          estimate = value;
          setConverted("");
        }
      } catch {
        if (latestEstimate.isCurrent(token)) estimate = null;
      } finally {
        if (latestEstimate.isCurrent(token)) estimating = false;
      }
    }, 300);
    return () => clearTimeout(timer);
  });

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!valid) return;
    onConvert(toOptions(form), form.replaceOriginal);
  }
</script>

<DialogShell
  bind:open
  width={single ? 980 : 460}
  title={targets.length === 1 ? "Convert image" : `Convert ${targets.length} images`}>
  <form class="form" class:split={!!single} id="convert-form" onsubmit={submit}>
    <div class="controls">
    {#if single}
      <div class="meta">
        <strong title={single.name}>{single.name}</strong>
        {#if info}
          <span>{info.width} × {info.height} px</span>
        {/if}
      </div>
    {/if}

    <span class="label" id="convert-format-label">Format</span>
    <div class="seg" role="radiogroup" aria-labelledby="convert-format-label">
      {#each writable as format (format)}
        <button
          type="button"
          role="radio"
          aria-checked={form.format === format}
          class:active={form.format === format}
          onclick={() => (form.format = format)}>
          {formatLabel(format)}
        </button>
      {/each}
    </div>

    <span class="label" id="convert-quality-label">Quality</span>
    {#if hasQuality(form.format)}
      <div class="quality">
        <Slider
          type="single"
          bind:value={form.quality}
          min={5}
          max={100}
          step={5}
          aria-labelledby="convert-quality-label" />
        <output>{form.quality}</output>
      </div>
      {#if form.format === "webp" && form.quality === 100}
        <p class="help">At 100 WebP is lossless.</p>
      {/if}
    {:else}
      <p class="help">Lossless: quality does not change the result.</p>
    {/if}

    {#if showScale}
      <span class="label" id="convert-scale-label">Scale</span>
      <div class="row wrap" role="group" aria-labelledby="convert-scale-label">
        {#each VECTOR_SCALES as scale (scale)}
          <button
            type="button"
            class="chip"
            class:active={form.vectorScale === scale}
            onclick={() => (form.vectorScale = scale)}>
            {scaleLabel(scale)}
          </button>
        {/each}
        <input
          type="number"
          min="1"
          max="6400"
          step="1"
          aria-label="Scale in percent"
          value={Math.round(form.vectorScale * 100)}
          oninput={(event) => (form.vectorScale = Number(event.currentTarget.value) / 100)} />
        <span class="unit">%</span>
      </div>
      <p class="help">
        {#if vectorOutput}
          Exports at {vectorOutput.width} × {vectorOutput.height} px.
        {/if}
        Vector art stays sharp at any scale.
      </p>
    {/if}

    {#if showSize}
    <span class="label" id="convert-size-label">Size</span>
    <div class="seg" role="radiogroup" aria-labelledby="convert-size-label">
      {#each SIZE_MODES as mode (mode.id)}
        <button
          type="button"
          role="radio"
          aria-checked={form.sizeMode === mode.id}
          class:active={form.sizeMode === mode.id}
          onclick={() => (form.sizeMode = mode.id)}>
          {mode.label}
        </button>
      {/each}
    </div>
    {#if form.sizeMode === "percent"}
      <div class="row">
        {#each PERCENT_CHOICES as choice (choice)}
          <button
            type="button"
            class="chip"
            class:active={form.percent === choice}
            onclick={() => (form.percent = choice)}>
            {choice}%
          </button>
        {/each}
        <input
          type="number"
          min="1"
          max="100"
          step="1"
          aria-label="Percent"
          bind:value={form.percent} />
        <span class="unit">%</span>
      </div>
    {:else if form.sizeMode === "fit"}
      <div class="row">
        <input type="number" min="0" step="1" aria-label="Max width" bind:value={form.fitWidth} />
        <span class="unit">×</span>
        <input type="number" min="0" step="1" aria-label="Max height" bind:value={form.fitHeight} />
        <span class="unit">px</span>
      </div>
      <div class="row">
        {#each SIZE_PRESETS as preset (preset.label)}
          <button
            type="button"
            class="chip"
            onclick={() => {
              form.fitWidth = preset.width;
              form.fitHeight = preset.height;
            }}>
            {preset.label}
          </button>
        {/each}
      </div>
      <p class="help">Never enlarges. A side of 0 is unlimited.</p>
    {/if}
    {/if}

    <span class="label">Transform</span>
    <div class="row">
      <button
        type="button"
        class="icon"
        aria-label="Rotate left"
        onclick={() => (form.rotate = rotateBy(form.rotate, -90))}>
        <RotateCcwIcon />
      </button>
      <button
        type="button"
        class="icon"
        aria-label="Rotate right"
        onclick={() => (form.rotate = rotateBy(form.rotate, 90))}>
        <RotateCwIcon />
      </button>
      <button
        type="button"
        class="icon"
        class:active={form.flipH}
        aria-pressed={form.flipH}
        aria-label="Flip horizontally"
        onclick={() => (form.flipH = !form.flipH)}>
        <FlipHorizontalIcon />
      </button>
      <button
        type="button"
        class="icon"
        class:active={form.flipV}
        aria-pressed={form.flipV}
        aria-label="Flip vertically"
        onclick={() => (form.flipV = !form.flipV)}>
        <FlipVerticalIcon />
      </button>
    </div>

    <div class="switches">
      <DialogSwitch
        label="Remove EXIF and location"
        description="Deletes camera, date and GPS data from the new file."
        checked={form.stripMetadata}
        onchange={(value) => (form.stripMetadata = value)} />
      <DialogSwitch
        label="Replace original"
        description="The original goes to the Trash after the new file is checked."
        checked={form.replaceOriginal}
        onchange={(value) => (form.replaceOriginal = value)} />
    </div>

    {#each warnings as warning (warning)}
      <p class="warning" role="status">
        <TriangleAlertIcon />
        {warning}
      </p>
    {/each}
    </div>
    {#if single}
      <div class="side">
        <CompareViewer
          originalSrc={previewUrl}
          convertedSrc={convertedUrl}
          rotate={form.rotate}
          flipH={form.flipH}
          flipV={form.flipV}
          {originalLabel}
          {convertedLabel}
          loading={estimating}
          onConvertedError={() => (convertedFailed = true)} />
        {#if !canPreview(form.format)}
          <p class="help">TIFF can't be shown here, so only the estimated size is available.</p>
        {:else if convertedFailed}
          <p class="help">
            This system can't display {formatLabel(form.format)}. The size shown is still exact.
          </p>
        {/if}
      </div>
    {/if}
  </form>
  {#snippet footer()}
    <DialogButton onclick={() => (open = false)}>Cancel</DialogButton>
    <DialogButton type="submit" form="convert-form" variant="primary" disabled={!valid}>
      {targets.length > 1 ? `Convert ${targets.length}` : "Convert"}
    </DialogButton>
  {/snippet}
</DialogShell>

<style>
  .form {
    display: grid;
    margin-top: 6px;
  }
  /* Narrow windows: the viewer goes above the controls. Wide ones: it sits on the right. */
  .form.split {
    gap: 14px;
  }
  .form.split .side {
    order: -1;
    --compare-height: min(34vh, 280px);
  }
  @media (min-width: 880px) {
    .form.split {
      grid-template-columns: 380px minmax(0, 1fr);
      gap: 18px;
      align-items: start;
    }
    .form.split .side {
      order: 0;
      --compare-height: min(62vh, 460px);
    }
  }
  .controls {
    display: grid;
    gap: 8px;
    min-width: 0;
  }
  .side {
    display: grid;
    gap: 8px;
    min-width: 0;
  }
  .label {
    margin-top: 6px;
    color: var(--app-fg-muted);
    font-size: 12px;
    font-weight: 500;
  }
  .meta {
    display: grid;
    min-width: 0;
    gap: 2px;
    font-size: 12px;
    color: var(--app-fg-muted);
  }
  .meta strong {
    overflow: hidden;
    color: var(--app-fg);
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .seg {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    padding: 2px;
    border-radius: calc(var(--app-radius) - 4px);
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
  }
  .seg button {
    flex: 1;
    min-width: 44px;
    height: 26px;
    padding: 0 8px;
    border: 0;
    border-radius: calc(var(--app-radius) - 6px);
    background: transparent;
    color: var(--app-fg-muted);
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    transition:
      background-color 160ms cubic-bezier(0.32, 0.72, 0, 1),
      color 160ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .seg button:hover {
    color: var(--app-fg);
  }
  .seg button.active {
    background: var(--app-surface-raised);
    box-shadow: inset 0 0 0 1px var(--app-border-strong);
    color: var(--app-fg);
  }
  .seg button:focus-visible,
  .chip:focus-visible,
  .icon:focus-visible {
    outline: 2px solid var(--app-accent);
    outline-offset: 1px;
  }
  .quality {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .quality output {
    min-width: 28px;
    color: var(--app-fg);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .row.wrap {
    flex-wrap: wrap;
  }
  .row input {
    width: 76px;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: calc(var(--app-radius) - 5px);
    outline: none;
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
    color: var(--app-fg);
    font: inherit;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .row input:focus {
    box-shadow:
      inset 0 0 0 1px var(--app-accent),
      0 0 0 3px color-mix(in srgb, var(--app-accent) 18%, transparent);
  }
  .unit {
    color: var(--app-fg-muted);
    font-size: 12px;
  }
  .chip,
  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 28px;
    border: 0;
    border-radius: calc(var(--app-radius) - 5px);
    background: var(--app-surface-raised);
    box-shadow: inset 0 0 0 1px var(--app-border-strong);
    color: var(--app-fg);
    font: inherit;
    font-size: 12px;
    transition: background-color 160ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .chip {
    padding: 0 10px;
  }
  .icon {
    width: 32px;
  }
  .icon :global(svg) {
    width: 15px;
    height: 15px;
  }
  .chip:hover,
  .icon:hover {
    background: var(--app-hover-strong);
  }
  .chip.active,
  .icon.active {
    box-shadow: inset 0 0 0 1px var(--app-accent);
    color: var(--app-accent);
  }
  .switches {
    display: grid;
    gap: 12px;
    margin-top: 8px;
  }
  .help {
    margin: 0;
    color: var(--app-fg-muted);
    font-size: 11px;
    line-height: 1.4;
  }
  .warning {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 0;
    color: var(--app-warning);
    font-size: 12px;
    line-height: 1.4;
  }
  .warning :global(svg) {
    flex-shrink: 0;
    width: 14px;
    height: 14px;
    margin-top: 1px;
  }
</style>
