<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import { onDestroy } from "svelte";
  import CompareViewer from "./compare-viewer.svelte";
  import DialogButton from "./dialog-button.svelte";
  import DialogShell from "./dialog-shell.svelte";
  import DialogSwitch from "./dialog-switch.svelte";
  import Segmented from "./segmented.svelte";
  import type { DirectoryEntry } from "#lib/components/custom/file-list/index.js";
  import { Slider } from "#lib/components/ui/slider/index.js";
  import { formatBytes, percentChange } from "#lib/file-ops/convert-dialog-model.js";
  import {
    BITRATES,
    FRAME_RATES,
    GIF_FRAME_RATES,
    GIF_WIDTHS,
    HEIGHTS,
    PRESETS,
    applyPreset,
    audioSize,
    clipLength,
    defaultFrameTime,
    defaultState,
    formatTime,
    isLossless,
    isValid,
    settingsKey,
    suffixFor,
    toOptions,
    trimOf,
    warningsFor,
    type DialogState,
    type PresetId,
    type Tab,
  } from "#lib/file-ops/transcode-model.js";
  import {
    ffmpegDownload,
    ffmpegStatus,
    mediaProbe,
    missingEncoders,
    transcodeEstimate,
    transcodeFrame,
    type FfmpegStatus,
    type MediaInfo,
    type TranscodeOptions,
  } from "#lib/file-ops/transcode.js";
  import { createLatest } from "#lib/file-ops/convert-dialog-model.js";
  import type { TransferEventPayload } from "#lib/transfers/jobs.js";

  let {
    open = $bindable(false),
    targets,
    onConvert,
  }: {
    open?: boolean;
    targets: DirectoryEntry[];
    onConvert: (options: TranscodeOptions, suffix: string | null, replaceOriginal: boolean) => void;
  } = $props();

  const TABS = [
    { value: "video", label: "Video" },
    { value: "audio", label: "Audio" },
    { value: "animation", label: "GIF / WebP" },
  ] as const;
  const CONTAINERS = [
    { value: "mp4", label: "MP4" },
    { value: "webm", label: "WebM" },
    { value: "mov", label: "MOV" },
    { value: "mkv", label: "MKV" },
  ] as const;
  const SPEEDS = [
    { value: "fast", label: "Fast" },
    { value: "balanced", label: "Balanced" },
    { value: "slow", label: "Slow" },
  ] as const;
  const AUDIO_FORMATS = [
    { value: "mp3", label: "MP3" },
    { value: "m4a", label: "M4A" },
    { value: "opus", label: "Opus" },
    { value: "flac", label: "FLAC" },
    { value: "wav", label: "WAV" },
  ] as const;
  const ANIMATION_FORMATS = [
    { value: "gif", label: "GIF" },
    { value: "webp", label: "WebP" },
  ] as const;
  const PRESET_OPTIONS = PRESETS.map((preset) => ({ value: preset.id as string, label: preset.label }));
  const HEIGHT_OPTIONS = HEIGHTS.map((entry) => ({ value: String(entry.value ?? "original"), label: entry.label }));
  const FPS_OPTIONS = FRAME_RATES.map((entry) => ({ value: String(entry.value ?? "original"), label: entry.label }));
  const BITRATE_OPTIONS = BITRATES.map((rate) => ({ value: String(rate), label: `${rate}k` }));
  const WIDTH_OPTIONS = GIF_WIDTHS.map((width) => ({ value: String(width), label: `${width}px` }));
  const GIF_FPS_OPTIONS = GIF_FRAME_RATES.map((rate) => ({ value: String(rate), label: String(rate) }));

  let form = $state<DialogState>(defaultState([]));
  let status = $state<FfmpegStatus | null>(null);
  let info = $state<MediaInfo | null>(null);
  let wasOpen = $state(false);
  let downloading = $state(false);
  let downloadFraction = $state(0);
  let downloadJob = "";
  let downloadError = $state("");

  const latest = createLatest();
  const latestFrame = createLatest();
  const names = $derived(targets.map((target) => target.name));
  const single = $derived(targets.length === 1 ? targets[0] : null);
  const duration = $derived(info?.duration ?? null);
  const trimResult = $derived(trimOf(form, duration));
  const length = $derived(clipLength(duration, trimResult.trim));
  const options = $derived(toOptions(form, trimResult.trim));
  const missing = $derived(missingEncoders(status, options));
  const noStream = $derived(
    info !== null && (form.tab === "audio" ? info.audio === null : info.video === null),
  );
  const warnings = $derived(warningsFor(form, info, status, options, length));
  const size = $derived(audioSize(length, form.audioFormat, form.bitrate));
  const ready = $derived(
    targets.length > 0 &&
      status?.available === true &&
      !downloading &&
      isValid(form, duration) &&
      missing.length === 0 &&
      !noStream,
  );
  const hint = $derived(PRESETS.find((preset) => preset.id === form.preset)?.hint ?? "Custom settings");

  // Estimated size of the result, kept only while the settings that produced it are unchanged.
  let estimate = $state<{ key: string; bytes: number } | null>(null);
  let estimating = $state(false);
  let estimateJob = "";
  let estimateError = $state("");
  const key = $derived(single ? settingsKey(single.path, options) : "");
  const currentEstimate = $derived(estimate && estimate.key === key ? estimate.bytes : null);
  const canPreview = $derived(
    single !== null && form.tab !== "audio" && info !== null && info.video !== null && duration !== null,
  );
  const needsEstimate = $derived(form.tab !== "audio" || isLossless(form.audioFormat));

  // One picture before and after, for judging the quality settings by eye.
  let frameTime = $state(0);
  let frame = $state<{ key: string; original: string; converted: string } | null>(null);
  let framing = $state(false);
  let frameError = $state("");
  const frameIsStale = $derived(frame !== null && frame.key !== key);

  function dropFrame() {
    if (frame) {
      URL.revokeObjectURL(frame.original);
      URL.revokeObjectURL(frame.converted);
    }
    frame = null;
  }

  onDestroy(dropFrame);

  async function runEstimate() {
    if (!single) return;
    estimateError = "";
    estimating = true;
    estimateJob = crypto.randomUUID();
    const forKey = key;
    try {
      estimate = { key: forKey, bytes: await transcodeEstimate(single.path, options, estimateJob) };
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (message !== "Cancelled") estimateError = message;
    } finally {
      estimating = false;
    }
  }

  function cancelEstimate() {
    void invoke("cancel_transfer", { id: estimateJob }).catch(() => {});
  }

  async function showFrame() {
    if (!single) return;
    frameError = "";
    framing = true;
    const token = latestFrame.next();
    const forKey = key;
    try {
      const [original, converted] = await Promise.all([
        transcodeFrame(single.path, options, frameTime, false),
        transcodeFrame(single.path, options, frameTime, true),
      ]);
      if (!latestFrame.isCurrent(token)) return;
      dropFrame();
      frame = {
        key: forKey,
        original: URL.createObjectURL(original),
        converted: URL.createObjectURL(converted),
      };
    } catch (error) {
      if (latestFrame.isCurrent(token)) frameError = error instanceof Error ? error.message : String(error);
    } finally {
      if (latestFrame.isCurrent(token)) framing = false;
    }
  }

  async function refresh() {
    const token = latest.next();
    try {
      const next = await ffmpegStatus();
      if (!latest.isCurrent(token)) return;
      status = next;
      info = null;
      if (next.available && single) {
        const probed = await mediaProbe(single.path).catch(() => null);
        if (latest.isCurrent(token)) {
          info = probed;
          frameTime = defaultFrameTime(probed?.duration ?? null, { start: null, end: null });
        }
      }
    } catch {
      if (latest.isCurrent(token)) status = null;
    }
  }

  $effect(() => {
    if (open && !wasOpen) {
      form = defaultState(names);
      status = null;
      info = null;
      downloadError = "";
      estimate = null;
      estimateError = "";
      frameError = "";
      dropFrame();
      void refresh();
    }
    wasOpen = open;
  });

  async function download() {
    downloadError = "";
    downloading = true;
    downloadFraction = 0;
    downloadJob = crypto.randomUUID();
    const stop = await listen<TransferEventPayload>("transfer-progress", ({ payload }) => {
      if (payload.id === downloadJob && payload.bytesTotal > 0) {
        downloadFraction = payload.bytesDone / payload.bytesTotal;
      }
    });
    try {
      status = await ffmpegDownload(downloadJob);
      if (single && status.available) info = await mediaProbe(single.path).catch(() => null);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (message !== "Download cancelled") downloadError = message;
    } finally {
      stop();
      downloading = false;
    }
  }

  function cancelDownload() {
    void invoke("cancel_transfer", { id: downloadJob }).catch(() => {});
  }

  function choosePreset(id: string) {
    form = applyPreset(form, id as PresetId);
  }

  function handOverride() {
    form.preset = null;
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!ready) return;
    onConvert(options, suffixFor(form), form.replaceOriginal);
  }

  const title = $derived(
    single ? (form.tab === "audio" ? "Convert audio" : "Convert video") : `Convert ${targets.length} files`,
  );
  const summary = $derived.by(() => {
    if (!info) return "";
    const parts: string[] = [];
    if (info.duration !== null) parts.push(formatTime(info.duration));
    if (info.video) {
      parts.push(`${info.video.width} × ${info.video.height}`);
      if (info.video.fps) parts.push(`${Math.round(info.video.fps)} fps`);
      parts.push(info.video.codec);
    } else if (info.audio) {
      parts.push(info.audio.codec);
    }
    parts.push(formatBytes(info.size));
    return parts.join(" · ");
  });
</script>

<DialogShell bind:open width={580} {title}>
  <form class="form" id="transcode-form" onsubmit={submit}>
    {#if single}
      <div class="meta">
        <strong title={single.name}>{single.name}</strong>
        {#if summary}<span>{summary}</span>{/if}
      </div>
    {/if}

    {#if status === null}
      <p class="help" role="status">Checking ffmpeg…</p>
    {:else if !status.available}
      <section class="needs">
        <h3>ffmpeg is needed</h3>
        <p>
          Converting video and audio uses ffmpeg, a free tool the app doesn’t include.
          {#if status.canDownload}
            It downloads once ({status.downloadSize ? formatBytes(status.downloadSize) : "about 60 MB"}, version
            {status.downloadVersion}) and is kept with the app.
          {:else}
            Install it (for example <code>brew install ffmpeg</code>, <code>winget install ffmpeg</code> or
            <code>apt install ffmpeg</code>) and check again.
          {/if}
        </p>
        {#if downloading}
          <div class="bar" role="progressbar" aria-valuenow={Math.round(downloadFraction * 100)} aria-valuemin={0} aria-valuemax={100}>
            <span style:width={`${Math.round(downloadFraction * 100)}%`}></span>
          </div>
          <div class="row">
            <span class="help">Downloading… {Math.round(downloadFraction * 100)}%</span>
            <DialogButton onclick={cancelDownload}>Cancel download</DialogButton>
          </div>
        {:else}
          <div class="row">
            {#if status.canDownload}
              <DialogButton variant="primary" onclick={() => void download()}>
                <DownloadIcon /> Download ffmpeg
              </DialogButton>
            {/if}
            <DialogButton onclick={() => void refresh()}>Check again</DialogButton>
          </div>
        {/if}
        {#if downloadError}
          <p class="error" role="alert"><TriangleAlertIcon />{downloadError}</p>
        {/if}
      </section>
    {:else}
      <Segmented
        options={TABS}
        value={form.tab}
        label="Convert to"
        onchange={(value: Tab) => (form.tab = value)} />

      {#if form.tab === "video"}
        <span class="label">Format</span>
        <Segmented options={CONTAINERS} value={form.container} label="Format" onchange={(value) => (form.container = value)} />

        <span class="label">Preset</span>
        <Segmented options={PRESET_OPTIONS} value={form.preset ?? ""} label="Preset" onchange={choosePreset} />
        <p class="help">{hint}</p>

        <span class="label" id="transcode-quality">Quality</span>
        <div class="quality">
          <Slider
            type="single"
            value={form.quality}
            min={0}
            max={100}
            step={5}
            aria-labelledby="transcode-quality"
            onValueChange={(value) => {
              form.quality = value;
              handOverride();
            }} />
          <output>{form.quality}</output>
        </div>

        <span class="label">Speed</span>
        <Segmented
          options={SPEEDS}
          value={form.speed}
          label="Speed"
          onchange={(value) => {
            form.speed = value;
            handOverride();
          }} />
        <p class="help">Slower takes longer and makes a smaller file at the same quality.</p>

        <div class="pair">
          <div>
            <span class="label">Resolution</span>
            <Segmented
              options={HEIGHT_OPTIONS}
              value={String(form.maxHeight ?? "original")}
              label="Resolution"
              onchange={(value) => {
                form.maxHeight = value === "original" ? null : Number(value);
                handOverride();
              }} />
          </div>
          <div>
            <span class="label">Frame rate</span>
            <Segmented
              options={FPS_OPTIONS}
              value={String(form.fps ?? "original")}
              label="Frame rate"
              onchange={(value) => {
                form.fps = value === "original" ? null : Number(value);
                handOverride();
              }} />
          </div>
        </div>

        {#if status.hardware && form.container !== "webm"}
          <DialogSwitch
            label="Use hardware acceleration"
            description={`Much faster with ${status.hardware === "videotoolbox" ? "this Mac’s video engine" : "your graphics card"}, but the files come out a little larger.`}
            checked={form.hardware}
            onchange={(value) => (form.hardware = value)} />
        {/if}
        <DialogSwitch
          label="Remove the sound"
          description="The result has no audio track."
          checked={form.mute}
          onchange={(value) => (form.mute = value)} />
      {:else if form.tab === "audio"}
        <span class="label">Format</span>
        <Segmented options={AUDIO_FORMATS} value={form.audioFormat} label="Audio format" onchange={(value) => (form.audioFormat = value)} />
        {#if isLossless(form.audioFormat)}
          <p class="help">Lossless: the quality does not depend on a bitrate.</p>
        {:else}
          <span class="label">Bitrate</span>
          <Segmented
            options={BITRATE_OPTIONS}
            value={String(form.bitrate)}
            label="Bitrate"
            onchange={(value) => (form.bitrate = Number(value))} />
        {/if}
        {#if single && info?.video}
          <p class="help">The audio is taken from the video.</p>
        {/if}
        {#if size !== null}
          <p class="size">About {formatBytes(size)}</p>
        {/if}
      {:else}
        <span class="label">Format</span>
        <Segmented
          options={ANIMATION_FORMATS}
          value={form.animationFormat}
          label="Animation format"
          onchange={(value) => (form.animationFormat = value)} />
        <span class="label">Width</span>
        <Segmented
          options={WIDTH_OPTIONS}
          value={String(form.width)}
          label="Width"
          onchange={(value) => (form.width = Number(value))} />
        <span class="label">Frame rate</span>
        <Segmented
          options={GIF_FPS_OPTIONS}
          value={String(form.animationFps)}
          label="Animation frame rate"
          onchange={(value) => (form.animationFps = Number(value))} />
        {#if form.animationFormat === "webp"}
          <span class="label" id="transcode-webp-quality">Quality</span>
          <div class="quality">
            <Slider
              type="single"
              bind:value={form.animationQuality}
              min={0}
              max={100}
              step={5}
              aria-labelledby="transcode-webp-quality" />
            <output>{form.animationQuality}</output>
          </div>
        {/if}
        <DialogSwitch
          label="Repeat"
          description="The animation loops forever."
          checked={form.repeat}
          onchange={(value) => (form.repeat = value)} />
      {/if}

      <span class="label">Trim</span>
      <div class="trim">
        <label>
          Start
          <input bind:value={form.start} placeholder="0:00" spellcheck="false" autocomplete="off" aria-invalid={trimResult.error !== null} />
        </label>
        <label>
          End
          <input
            bind:value={form.end}
            placeholder={duration !== null ? formatTime(duration) : "End"}
            spellcheck="false"
            autocomplete="off"
            aria-invalid={trimResult.error !== null} />
        </label>
        <span class="length">{length !== null ? `Result: ${formatTime(length)}` : ""}</span>
      </div>
      {#if trimResult.error}
        <p class="error" role="alert"><TriangleAlertIcon />{trimResult.error}</p>
      {/if}

      {#if single && (needsEstimate || canPreview)}
        <details class="more">
          <summary>Preview and size</summary>
          {#if needsEstimate}
            <div class="estimate">
              {#if estimating}
                <span class="help" role="status">Estimating… it encodes a few short samples.</span>
                <DialogButton onclick={cancelEstimate}>Cancel</DialogButton>
              {:else}
                <DialogButton onclick={() => void runEstimate()} disabled={!ready}>Estimate size</DialogButton>
                {#if currentEstimate !== null}
                  <span class="size">
                    About {formatBytes(currentEstimate)}
                    {#if info && info.size > 0 && form.tab === "video"}
                      ({percentChange(info.size, currentEstimate)})
                    {/if}
                  </span>
                {:else if estimate}
                  <span class="help">Settings changed. Estimate again.</span>
                {/if}
              {/if}
            </div>
            {#if estimateError}
              <p class="error" role="alert"><TriangleAlertIcon />{estimateError}</p>
            {/if}
          {/if}

          {#if canPreview}
            <span class="label" id="transcode-frame-time">Compare a frame at {formatTime(frameTime)}</span>
            <div class="quality">
              <Slider
                type="single"
                bind:value={frameTime}
                min={0}
                max={Math.max(1, Math.floor(duration ?? 1))}
                step={1}
                aria-labelledby="transcode-frame-time" />
              <DialogButton onclick={() => void showFrame()} disabled={!ready || framing}>
                {framing ? "Working…" : "Compare"}
              </DialogButton>
            </div>
            {#if frameError}
              <p class="error" role="alert"><TriangleAlertIcon />{frameError}</p>
            {/if}
            {#if frame}
              <div class="frame">
                <CompareViewer
                  originalSrc={frame.original}
                  convertedSrc={frame.converted}
                  originalLabel="Original"
                  convertedLabel="Converted"
                  loading={framing} />
              </div>
              {#if frameIsStale}
                <p class="help">Settings changed. Compare again to see the new result.</p>
              {/if}
            {/if}
          {/if}
        </details>
      {/if}

      <DialogSwitch
        label="Replace original"
        description="The original goes to the Trash after the new file is checked."
        checked={form.replaceOriginal}
        onchange={(value) => (form.replaceOriginal = value)} />

      {#each warnings as warning (warning)}
        <p class="warning" role="status"><TriangleAlertIcon />{warning}</p>
      {/each}
    {/if}
  </form>
  {#snippet footer()}
    <DialogButton onclick={() => (open = false)}>Cancel</DialogButton>
    <DialogButton type="submit" form="transcode-form" variant="primary" disabled={!ready}>
      {targets.length > 1 ? `Convert ${targets.length}` : "Convert"}
    </DialogButton>
  {/snippet}
</DialogShell>

<style>
  .form {
    display: grid;
    gap: 8px;
    margin-top: 6px;
    max-height: min(72vh, 640px);
    overflow-y: auto;
    padding-right: 4px;
  }
  .meta {
    display: grid;
    gap: 2px;
    min-width: 0;
    padding: 10px;
    border-radius: calc(var(--app-radius) - 4px);
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
    color: var(--app-fg-muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .meta strong {
    overflow: hidden;
    color: var(--app-fg);
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .label {
    margin-top: 6px;
    color: var(--app-fg-muted);
    font-size: 12px;
    font-weight: 500;
  }
  .help {
    margin: 0;
    color: var(--app-fg-faint);
    font-size: 11px;
    line-height: 1.4;
  }
  .size {
    margin: 0;
    color: var(--app-fg);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
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
  .more {
    display: grid;
    gap: 8px;
    padding: 10px 12px;
    border-radius: calc(var(--app-radius) - 4px);
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
  }
  .more > summary {
    cursor: pointer;
    color: var(--app-fg-muted);
    font-size: 12px;
    font-weight: 500;
  }
  .more[open] > summary {
    margin-bottom: 8px;
  }
  .estimate {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .frame {
    --compare-height: 240px;
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .pair > div {
    display: grid;
    gap: 6px;
    align-content: start;
    min-width: 0;
  }
  .trim {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    align-items: end;
    gap: 8px;
  }
  .trim label {
    display: grid;
    gap: 4px;
    color: var(--app-fg-muted);
    font-size: 11px;
  }
  .trim input {
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: calc(var(--app-radius) - 5px);
    outline: none;
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
    color: var(--app-fg);
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 12px;
  }
  .trim input:focus {
    box-shadow:
      inset 0 0 0 1px var(--app-accent),
      0 0 0 3px color-mix(in srgb, var(--app-accent) 18%, transparent);
  }
  .trim input[aria-invalid="true"] {
    box-shadow: inset 0 0 0 1px var(--app-danger);
  }
  .length {
    padding-bottom: 6px;
    color: var(--app-fg-muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .needs {
    display: grid;
    gap: 10px;
    padding: 14px;
    border-radius: calc(var(--app-radius) - 4px);
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
  }
  .needs h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  .needs p {
    margin: 0;
    color: var(--app-fg-muted);
    font-size: 12px;
    line-height: 1.5;
  }
  .needs code {
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--app-surface-raised);
    font-size: 11px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .row :global(svg) {
    width: 14px;
    height: 14px;
  }
  .bar {
    height: 6px;
    overflow: hidden;
    border-radius: 999px;
    background: var(--app-surface-raised);
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--app-accent);
    transition: width 160ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .warning,
  .error {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 0;
    font-size: 12px;
    line-height: 1.4;
  }
  .warning {
    color: var(--app-warning);
  }
  .error {
    color: var(--app-danger-fg);
  }
  .warning :global(svg),
  .error :global(svg) {
    flex-shrink: 0;
    width: 14px;
    height: 14px;
    margin-top: 1px;
  }
  @media (prefers-reduced-motion: reduce) {
    .bar span {
      transition: none;
    }
  }
</style>
