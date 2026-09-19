<script lang="ts">
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import { openDicom, type DicomPreview } from "./dicom.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let dicom = $state.raw<DicomPreview | null>(null);
  let error = $state("");
  let token = 0;

  let canvas = $state<HTMLCanvasElement | null>(null);
  let raw: Uint8Array | null = null;
  let sampleMin = $state(0);
  let sampleMax = $state(255);
  let center = $state(0);
  let width = $state(0);
  let defaults = { center: 0, width: 0 };

  function decode(base64: string): Uint8Array {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes;
  }

  function sampleAt(index: number): number {
    if (!raw || !dicom) return 0;
    if (dicom.bitsAllocated > 8) {
      return raw[index * 2] | (raw[index * 2 + 1] << 8);
    }
    return raw[index];
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    dicom = null;
    raw = null;
    error = "";
    openDicom(target)
      .then((result) => {
        if (request !== token) return;
        const bytes = decode(result.pixels);
        raw = bytes;
        const samples = result.columns * result.rows;
        let min = Number.POSITIVE_INFINITY;
        let max = Number.NEGATIVE_INFINITY;
        const step = result.bitsAllocated > 8 ? 2 : 1;
        for (let index = 0; index < samples; index += 1) {
          const value =
            step === 2 ? bytes[index * 2] | (bytes[index * 2 + 1] << 8) : bytes[index];
          if (value < min) min = value;
          if (value > max) max = value;
        }
        if (!Number.isFinite(min)) {
          min = 0;
          max = 255;
        }
        sampleMin = min;
        sampleMax = max;
        defaults = {
          center: result.windowCenter ?? (min + max) / 2,
          width: result.windowWidth ?? Math.max(1, max - min),
        };
        center = defaults.center;
        width = defaults.width;
        dicom = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });

  $effect(() => {
    const element = canvas;
    const image = dicom;
    const currentCenter = center;
    const currentWidth = width;
    if (!element || !image || !raw) return;
    const context = element.getContext("2d");
    if (!context) return;
    element.width = image.columns;
    element.height = image.rows;
    const pixels = context.createImageData(image.columns, image.rows);
    const low = currentCenter - currentWidth / 2;
    const span = currentWidth || 1;
    const samples = image.columns * image.rows;
    for (let index = 0; index < samples; index += 1) {
      const value = sampleAt(index);
      const scaled = Math.max(0, Math.min(1, (value - low) / span));
      const gray = Math.round(scaled * 255);
      pixels.data[index * 4] = gray;
      pixels.data[index * 4 + 1] = gray;
      pixels.data[index * 4 + 2] = gray;
      pixels.data[index * 4 + 3] = 255;
    }
    context.putImageData(pixels, 0, 0);
  });

  function reset() {
    center = defaults.center;
    width = defaults.width;
  }
</script>

<div class="flex h-full min-h-0 bg-[#1f1d1b] text-[#e8e5e2]" aria-label={`DICOM preview of ${name}`}>
  {#if error}
    <div class="grid h-full flex-1 place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if dicom}
    <div class="flex min-h-0 min-w-0 flex-1 items-center justify-center overflow-auto p-4">
      <canvas
        bind:this={canvas}
        class="max-h-full max-w-full rounded border border-[#3a3734]"
        style="image-rendering: pixelated; background: #000;"></canvas>
    </div>
    <aside class="flex w-64 shrink-0 flex-col gap-4 overflow-auto border-l border-[#3a3734] bg-[#242220] p-4">
      <div>
        <h2 class="truncate text-[13px] font-semibold">{dicom.patient || name}</h2>
        {#if dicom.patientId}<p class="text-[11.5px] text-[#9c9895]">{dicom.patientId}</p>{/if}
      </div>
      <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[12px]">
        {#if dicom.modality}
          <dt class="text-[#9c9895]">Modalidad</dt><dd>{dicom.modality}</dd>
        {/if}
        {#if dicom.studyDate}
          <dt class="text-[#9c9895]">Fecha</dt><dd>{dicom.studyDate}</dd>
        {/if}
        {#if dicom.studyDescription}
          <dt class="text-[#9c9895]">Estudio</dt><dd class="break-words">{dicom.studyDescription}</dd>
        {/if}
        <dt class="text-[#9c9895]">Tamaño</dt><dd>{dicom.columns}×{dicom.rows}</dd>
        <dt class="text-[#9c9895]">Bits</dt><dd>{dicom.bitsAllocated}</dd>
        {#if dicom.photometric}
          <dt class="text-[#9c9895]">Fotometría</dt><dd>{dicom.photometric}</dd>
        {/if}
      </dl>
      <div class="flex flex-col gap-3 border-t border-[#3a3734] pt-3">
        <label class="text-[11.5px] text-[#9c9895]">
          Centro {Math.round(center)}
          <input
            type="range"
            class="mt-1 w-full"
            min={sampleMin}
            max={sampleMax}
            step="1"
            value={center}
            oninput={(event) => (center = Number(event.currentTarget.value))} />
        </label>
        <label class="text-[11.5px] text-[#9c9895]">
          Ancho {Math.round(width)}
          <input
            type="range"
            class="mt-1 w-full"
            min="1"
            max={Math.max(1, sampleMax - sampleMin)}
            step="1"
            value={width}
            oninput={(event) => (width = Number(event.currentTarget.value))} />
        </label>
        <button
          type="button"
          class="flex items-center justify-center gap-1.5 rounded border border-[#3a3734] bg-transparent px-2 py-1 text-[12px] text-[#c0bbb5] hover:bg-[#3b3836] hover:text-white"
          onclick={reset}>
          <RotateCcw class="size-3.5" />Reiniciar
        </button>
      </div>
    </aside>
  {/if}
</div>
