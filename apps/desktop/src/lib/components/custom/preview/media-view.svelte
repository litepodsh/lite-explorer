<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import PlayIcon from "@lucide/svelte/icons/play";
  import PauseIcon from "@lucide/svelte/icons/pause";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
  import RotateCwIcon from "@lucide/svelte/icons/rotate-cw";
  import Volume2Icon from "@lucide/svelte/icons/volume-2";
  import Volume1Icon from "@lucide/svelte/icons/volume-1";
  import VolumeXIcon from "@lucide/svelte/icons/volume-x";
  import ExpandIcon from "@lucide/svelte/icons/expand";
  import ShrinkIcon from "@lucide/svelte/icons/shrink";
  import MaximizeIcon from "@lucide/svelte/icons/maximize-2";
  import MusicIcon from "@lucide/svelte/icons/music";
  import AlertTriangleIcon from "@lucide/svelte/icons/triangle-alert";
  import ExternalLinkIcon from "@lucide/svelte/icons/external-link";
  import * as Tooltip from "$lib/components/ui/tooltip/index.js";
  import MediaTool from "./media-tool.svelte";
  import { openTarget } from "$lib/file-ops/open.js";

  type Props = {
    src: string;
    name: string;
    kind: "video" | "audio";
    /** Local/remote path, used by the “Open with default app” fallback. */
    path?: string;
    /** Shows the “open in a separate window” button when provided. */
    onMaximize?: () => void;
    /** Focuses the player on mount so keyboard shortcuts work immediately. */
    autofocus?: boolean;
  };
  let { src, name, kind, path, onMaximize, autofocus = false }: Props = $props();

  const SKIP_SECONDS = 10;
  const VOLUME_STEP = 0.05;
  const HIDE_DELAY = 2600;
  const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];

  let containerEl = $state<HTMLElement | null>(null);
  let mediaEl = $state<HTMLVideoElement | null>(null);
  let trackEl = $state<HTMLElement | null>(null);

  let playing = $state(false);
  let current = $state(0);
  let duration = $state(0);
  let buffered = $state(0);
  let volume = $state(1);
  let muted = $state(false);
  let rate = $state(1);
  let loading = $state(true);
  let error = $state("");
  let scrubbing = $state(false);
  let fullscreen = $state(false);
  let controlsVisible = $state(true);
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  // Tooltips render into the player while fullscreen (a portal to <body> would be
  // invisible outside the fullscreen subtree); otherwise they use the default body.
  const portalTarget = $derived(fullscreen ? containerEl : null);
  const hasDuration = $derived(Number.isFinite(duration) && duration > 0);
  const playedPercent = $derived(hasDuration ? (current / duration) * 100 : 0);
  const bufferedPercent = $derived(hasDuration ? (buffered / duration) * 100 : 0);
  const VolumeIcon = $derived(muted || volume === 0 ? VolumeXIcon : volume < 0.5 ? Volume1Icon : Volume2Icon);

  type FullscreenDocument = Document & {
    webkitFullscreenElement?: Element | null;
    webkitExitFullscreen?: () => void;
  };
  type FullscreenNode = HTMLElement & {
    webkitRequestFullscreen?: () => void;
  };

  function fullscreenElement(): Element | null {
    return document.fullscreenElement ?? (document as FullscreenDocument).webkitFullscreenElement ?? null;
  }

  async function exitFullscreen(): Promise<void> {
    const doc = document as FullscreenDocument;
    if (typeof doc.exitFullscreen === "function") return doc.exitFullscreen();
    doc.webkitExitFullscreen?.();
  }

  onMount(() => {
    // Fullscreen targets the player element (so the custom controls stay on screen);
    // OS window fullscreen is only a fallback for webviews without the API.
    const sync = () => {
      if (fullscreenElement() === containerEl) {
        fullscreen = true;
        return;
      }
      void getCurrentWindow()
        .isFullscreen()
        .then((value) => (fullscreen = value))
        .catch(() => (fullscreen = false));
    };
    sync();
    document.addEventListener("fullscreenchange", sync);
    document.addEventListener("webkitfullscreenchange", sync);
    return () => {
      document.removeEventListener("fullscreenchange", sync);
      document.removeEventListener("webkitfullscreenchange", sync);
    };
  });

  $effect(() => {
    // Controls never hide while paused or scrubbing.
    if (!playing || scrubbing) {
      controlsVisible = true;
      clearTimeout(hideTimer);
    }
  });

  function poke() {
    controlsVisible = true;
    clearTimeout(hideTimer);
    if (playing && !scrubbing) {
      hideTimer = setTimeout(() => (controlsVisible = false), HIDE_DELAY);
    }
  }

  function formatTime(seconds: number): string {
    if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
    const total = Math.floor(seconds);
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const secs = total % 60;
    const mm = hours > 0 ? String(minutes).padStart(2, "0") : String(minutes);
    return `${hours > 0 ? `${hours}:` : ""}${mm}:${String(secs).padStart(2, "0")}`;
  }

  function togglePlay() {
    const el = mediaEl;
    if (!el || error) return;
    if (el.paused) void el.play().catch((reason) => (error = String(reason)));
    else el.pause();
  }

  function seekBy(delta: number) {
    const el = mediaEl;
    if (!el || !hasDuration) return;
    el.currentTime = Math.min(duration, Math.max(0, el.currentTime + delta));
    poke();
  }

  function setVolume(next: number) {
    const el = mediaEl;
    if (!el) return;
    volume = Math.min(1, Math.max(0, next));
    el.volume = volume;
    el.muted = volume === 0;
    poke();
  }

  function toggleMute() {
    const el = mediaEl;
    if (!el) return;
    el.muted = !el.muted;
    poke();
  }

  function cycleSpeed() {
    const el = mediaEl;
    if (!el) return;
    const index = SPEEDS.indexOf(rate);
    rate = SPEEDS[(index + 1) % SPEEDS.length];
    el.playbackRate = rate;
    poke();
  }

  async function toggleFullscreen() {
    if (fullscreenElement()) {
      try {
        await exitFullscreen();
      } catch {
        /* already leaving fullscreen */
      }
      return;
    }
    const node = containerEl as FullscreenNode | null;
    if (typeof node?.requestFullscreen === "function") {
      try {
        await node.requestFullscreen();
        return;
      } catch {
        /* fall through to window fullscreen */
      }
    }
    if (typeof node?.webkitRequestFullscreen === "function") {
      node.webkitRequestFullscreen();
      return;
    }
    try {
      await getCurrentWindow().setFullscreen(!fullscreen);
    } catch {
      /* fullscreen unavailable */
    }
  }

  function ratioFromEvent(event: PointerEvent): number {
    const rect = trackEl?.getBoundingClientRect();
    if (!rect || rect.width === 0) return 0;
    return Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
  }

  function scrubTo(ratio: number) {
    current = ratio * duration;
    if (mediaEl) mediaEl.currentTime = current;
  }

  function startScrub(event: PointerEvent) {
    if (!mediaEl || !hasDuration) return;
    scrubbing = true;
    trackEl?.setPointerCapture(event.pointerId);
    scrubTo(ratioFromEvent(event));
    poke();
  }

  function moveScrub(event: PointerEvent) {
    if (!scrubbing) return;
    scrubTo(ratioFromEvent(event));
  }

  function endScrub(event: PointerEvent) {
    if (!scrubbing) return;
    scrubbing = false;
    try {
      trackEl?.releasePointerCapture(event.pointerId);
    } catch {
      /* pointer already released */
    }
    poke();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.target instanceof HTMLInputElement) return;
    const key = event.key.toLowerCase();
    if (key === " " || key === "k") {
      event.preventDefault();
      togglePlay();
    } else if (key === "arrowright") {
      event.preventDefault();
      seekBy(5);
    } else if (key === "arrowleft") {
      event.preventDefault();
      seekBy(-5);
    } else if (key === "l") {
      seekBy(SKIP_SECONDS);
    } else if (key === "j") {
      seekBy(-SKIP_SECONDS);
    } else if (key === "arrowup") {
      event.preventDefault();
      setVolume(volume + VOLUME_STEP);
    } else if (key === "arrowdown") {
      event.preventDefault();
      setVolume(volume - VOLUME_STEP);
    } else if (key === "m") {
      toggleMute();
    } else if (key === "f") {
      event.preventDefault();
      void toggleFullscreen();
    } else if (/^[0-9]$/.test(key) && hasDuration) {
      scrubTo(Number(key) / 10);
    }
  }

  /** Pauses playback, e.g. when the file opens in the separate viewer window. */
  export function pause() {
    mediaEl?.pause();
  }

  function reportError() {
    const code = mediaEl?.error?.code;
    error =
      code === 4
        ? "This file uses a format or codec the preview can’t play."
        : (mediaEl?.error?.message || "This media couldn’t be played.");
  }

  /** Focuses the player when `autofocus` is true, so keys work without a click. */
  function focusSelf(node: HTMLElement, enabled: boolean) {
    if (enabled) queueMicrotask(() => node.focus({ preventScroll: true }));
  }
</script>

<svelte:window onpointerup={endScrub} onpointermove={moveScrub} />

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions, a11y_no_static_element_interactions -->
<div
  bind:this={containerEl}
  class="media-stage @container relative flex h-full w-full items-center justify-center overflow-hidden bg-black outline-none"
  role="group"
  aria-label={`${kind === "audio" ? "Audio" : "Video"} preview of ${name}`}
  tabindex="0"
  onkeydown={handleKeydown}
  onpointermove={poke}
  onpointerdown={poke}
  onmouseleave={() => playing && (controlsVisible = false)}
  ondblclick={() => void toggleFullscreen()}
  use:focusSelf={autofocus}
>
  <!-- svelte-ignore a11y_media_has_caption -->
  <video
    bind:this={mediaEl}
    {src}
    preload="metadata"
    playsinline
    class={kind === "audio"
      ? "pointer-events-none absolute h-px w-px opacity-0"
      : "max-h-full max-w-full"}
    onclick={togglePlay}
    onplay={() => ((playing = true), poke())}
    onpause={() => (playing = false)}
    ontimeupdate={() => !scrubbing && (current = mediaEl?.currentTime ?? 0)}
    ondurationchange={() => ((duration = mediaEl?.duration ?? 0), (loading = false))}
    onloadedmetadata={() => ((duration = mediaEl?.duration ?? 0), (loading = false))}
    onprogress={() => {
      const el = mediaEl;
      try {
        if (el && el.buffered.length) buffered = el.buffered.end(el.buffered.length - 1);
      } catch {
        /* no buffered ranges yet */
      }
    }}
    onwaiting={() => (loading = true)}
    onplaying={() => ((loading = false), poke())}
    oncanplay={() => (loading = false)}
    onvolumechange={() => ((volume = mediaEl?.volume ?? 1), (muted = mediaEl?.muted ?? false))}
    onratechange={() => (rate = mediaEl?.playbackRate ?? 1)}
    onended={() => (playing = false)}
    onerror={reportError}
  ></video>

  {#if kind === "audio" && !error}
    <div class="flex select-none flex-col items-center gap-4 px-8 text-center" aria-hidden="true">
      <span class="grid size-24 place-items-center rounded-full bg-[linear-gradient(180deg,rgb(255_255_255/0.08),rgb(255_255_255/0.02))] text-[#c9c4c0] shadow-[inset_0_1px_0_rgb(255_255_255/0.1)]">
        <MusicIcon class="size-10 stroke-[1.4]" />
      </span>
      <p class="max-w-[240px] truncate text-[13px] font-medium text-[#9c9895]" title={name}>{name}</p>
    </div>
  {/if}

  {#if loading && !error}
    <div class="pointer-events-none absolute inset-0 grid place-items-center">
      <span class="size-8 animate-spin rounded-full border-2 border-white/20 border-t-white/80"></span>
    </div>
  {/if}

  {#if !playing && !loading && !error}
    <button
      type="button"
      class="absolute grid size-16 place-items-center rounded-full border-0 bg-[rgb(20_18_17/0.55)] text-white shadow-[inset_0_1px_0_rgb(255_255_255/0.15),0_20px_50px_-20px_rgb(0_0_0/0.9)] backdrop-blur-md transition-transform duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] hover:scale-105 active:scale-95"
      aria-label="Play"
      onclick={togglePlay}><PlayIcon class="ml-0.5 size-7 fill-current stroke-none" /></button
    >
  {/if}

  {#if error}
    <div class="grid max-w-[80%] place-content-center justify-items-center gap-2 text-center">
      <AlertTriangleIcon class="size-10 stroke-[1.4] text-[#d8a24a]" />
      <p class="text-[13.5px] font-semibold text-[#e8e5e2]">Can’t play this file</p>
      <p class="text-[12px] text-[#9c9895]">{error}</p>
      {#if path}
        <button
          type="button"
          class="mt-1 flex items-center gap-1.5 rounded-full border-0 bg-white/[0.06] px-3.5 py-1.5 text-[12px] font-medium text-[#e8e5e2] transition-[background-color,transform] duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/[0.12] active:scale-[0.97]"
          onclick={() => path && openTarget(path)}
        >
          <ExternalLinkIcon class="size-3.5 stroke-[1.8]" /> Open with default app
        </button>
      {/if}
    </div>
  {/if}

  {#if !error}
    <div
      class="media-toolbar absolute inset-x-2 bottom-2 z-20 flex items-center gap-0.5 rounded-full px-1.5 py-1 transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] {controlsVisible
        ? 'opacity-100'
        : 'pointer-events-none translate-y-2 opacity-0'}"
      role="toolbar"
      aria-label="Playback controls"
    >
      <MediaTool label={playing ? "Pause" : "Play"} {portalTarget} onclick={togglePlay}>
        {#if playing}
          <PauseIcon class="size-[15px] stroke-[1.7]" />
        {:else}
          <PlayIcon class="size-[15px] stroke-[1.7]" />
        {/if}
      </MediaTool>

      <span class="hidden items-center gap-0.5 @[20rem]:flex">
        <span class="mx-0.5 h-4 w-px shrink-0 bg-white/10"></span>
        <MediaTool
          label={`Back ${SKIP_SECONDS} seconds`}
          {portalTarget}
          disabled={!hasDuration}
          onclick={() => seekBy(-SKIP_SECONDS)}>
          <RotateCcwIcon class="size-[15px] stroke-[1.7]" />
        </MediaTool>
        <MediaTool
          label={`Forward ${SKIP_SECONDS} seconds`}
          {portalTarget}
          disabled={!hasDuration}
          onclick={() => seekBy(SKIP_SECONDS)}>
          <RotateCwIcon class="size-[15px] stroke-[1.7]" />
        </MediaTool>
      </span>

      <span class="hidden shrink-0 px-1 text-[11px] tabular-nums text-[#c9c4c0] @[23rem]:inline" aria-live="off">
        {formatTime(current)}<span class="text-[#8d8986]"> / {formatTime(duration)}</span>
      </span>

      <Tooltip.Root delayDuration={400}>
        <Tooltip.Trigger>
          {#snippet child({ props })}
            <div
              {...props}
              bind:this={trackEl}
              class="group/track relative mx-1 h-1 min-w-6 flex-1 cursor-pointer rounded-full bg-white/15"
              role="slider"
              aria-label="Seek"
              aria-valuemin={0}
              aria-valuemax={Math.round(duration)}
              aria-valuenow={Math.round(current)}
              tabindex="-1"
              onpointerdown={startScrub}
            >
              <div class="pointer-events-none absolute inset-y-0 left-0 rounded-full bg-white/20" style="width: {bufferedPercent}%"></div>
              <div class="pointer-events-none absolute inset-y-0 left-0 rounded-full bg-[#0a9bff]" style="width: {playedPercent}%"></div>
              <div
                class="pointer-events-none absolute top-1/2 size-3 -translate-y-1/2 rounded-full bg-white opacity-0 shadow-[0_2px_8px_rgb(0_0_0/0.6)] transition-opacity duration-150 group-hover/track:opacity-100 {scrubbing
                  ? 'opacity-100'
                  : ''}"
                style="left: calc({playedPercent}% - 6px);"
              ></div>
            </div>
          {/snippet}
        </Tooltip.Trigger>
        <Tooltip.Content side="top" sideOffset={10} portalProps={portalTarget ? { to: portalTarget } : undefined}>
          Seek - {formatTime(current)} of {formatTime(duration)}
        </Tooltip.Content>
      </Tooltip.Root>

      <div class="group/volume flex shrink-0 items-center gap-1 px-0.5">
        <MediaTool label={muted ? "Unmute" : "Mute"} {portalTarget} onclick={toggleMute}>
          <VolumeIcon class="size-[15px] stroke-[1.7]" />
        </MediaTool>
        <Tooltip.Root delayDuration={400}>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <input
                {...props}
                type="range"
                min="0"
                max="1"
                step="0.01"
                value={muted ? 0 : volume}
                aria-label="Volume"
                class="volume-slider hidden h-1 w-14 cursor-pointer @[28rem]:block"
                oninput={(event) => setVolume(Number(event.currentTarget.value))}
              />
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content side="top" sideOffset={10} portalProps={portalTarget ? { to: portalTarget } : undefined}>
            Volume - {Math.round((muted ? 0 : volume) * 100)}%
          </Tooltip.Content>
        </Tooltip.Root>
      </div>

      <Tooltip.Root delayDuration={400}>
        <Tooltip.Trigger>
          {#snippet child({ props })}
            <button
              {...props}
              type="button"
              class="hidden min-w-9 shrink-0 rounded-full border-0 bg-transparent px-1.5 text-center text-[11px] tabular-nums text-[#c9c4c0] transition-colors duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/[0.08] hover:text-[#f2f1f0] @[32rem]:inline-block"
              aria-label="Playback speed"
              onclick={cycleSpeed}>{rate}×</button
            >
          {/snippet}
        </Tooltip.Trigger>
        <Tooltip.Content side="top" sideOffset={10} portalProps={portalTarget ? { to: portalTarget } : undefined}>
          Playback speed - {rate}×
        </Tooltip.Content>
      </Tooltip.Root>

      {#if onMaximize}
        <MediaTool label="Open in a separate window" {portalTarget} onclick={onMaximize}>
          <MaximizeIcon class="size-[15px] stroke-[1.7]" />
        </MediaTool>
      {/if}
      <MediaTool
        label={fullscreen ? "Exit fullscreen" : "Fullscreen"}
        {portalTarget}
        onclick={() => void toggleFullscreen()}>
        {#if fullscreen}
          <ShrinkIcon class="size-[15px] stroke-[1.7]" />
        {:else}
          <ExpandIcon class="size-[15px] stroke-[1.7]" />
        {/if}
      </MediaTool>
    </div>
  {/if}
</div>

<style>
  .media-stage:fullscreen {
    width: 100vw;
    height: 100vh;
    background: #000;
  }
  .media-toolbar {
    background: rgb(45 42 40 / 0.82);
    box-shadow:
      inset 0 0 0 1px rgb(255 255 255 / 0.09),
      inset 0 1px 0 rgb(255 255 255 / 0.08),
      0 16px 40px -16px rgb(0 0 0 / 0.7);
    -webkit-backdrop-filter: blur(24px);
    backdrop-filter: blur(24px);
  }
  @media (prefers-reduced-transparency: reduce) {
    .media-toolbar {
      background: #2d2a28;
      -webkit-backdrop-filter: none;
      backdrop-filter: none;
    }
  }
  .volume-slider {
    appearance: none;
    -webkit-appearance: none;
    background: rgb(255 255 255 / 0.15);
    border-radius: 999px;
  }
  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 11px;
    height: 11px;
    border-radius: 999px;
    background: #f2f1f0;
    box-shadow: 0 2px 6px rgb(0 0 0 / 0.5);
  }
  .volume-slider::-moz-range-thumb {
    width: 11px;
    height: 11px;
    border: 0;
    border-radius: 999px;
    background: #f2f1f0;
  }
  @media (prefers-reduced-motion: reduce) {
    .media-toolbar {
      transition: none;
    }
  }
</style>
