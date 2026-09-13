<script lang="ts">
  import { formatDate, formatSize } from "./format.js";
  import { kindLabel } from "./languages.js";
  import type { FilePreview } from "./types.js";
  import type { OpenWithApp } from "$lib/file-ops/open.js";

  // While `pending`, `preview` belongs to the previous file: its rows stay in the
  // layout (hidden) so the panel height does not jump between selections.
  type Props = { name: string; preview: FilePreview | null; defaultApp?: OpenWithApp | null; pending?: boolean };
  let { name, preview, defaultApp = null, pending = false }: Props = $props();
</script>

<section class="shrink-0 px-4 pt-3 pb-4" aria-label="File information">
  <h2 class="truncate text-[15px] font-semibold text-[#e8e5e2]" title={name}>{name}</h2>
  {#if preview}
    <div class={pending ? "invisible" : ""} aria-hidden={pending}>
      <p class="mt-0.5 text-[13px] text-[#9c9895]">
        {kindLabel(preview.name, preview.kind)}{preview.kind === "directory" ? "" : ` - ${formatSize(preview.size)}`}
      </p>
      {#if preview.created !== null || preview.modified !== null || defaultApp}
        <h3 class="mt-4 mb-1.5 text-[13px] font-semibold text-[#e8e5e2]">Information</h3>
        <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-[12px]">
          {#if preview.created !== null}
            <dt class="text-[#9c9895]">Created</dt>
            <dd class="m-0 text-right text-[#e8e5e2]">{formatDate(preview.created)}</dd>
          {/if}
          {#if preview.modified !== null}
            <dt class="text-[#9c9895]">Modified</dt>
            <dd class="m-0 text-right text-[#e8e5e2]">{formatDate(preview.modified)}</dd>
          {/if}
          {#if defaultApp}
            <dt class="text-[#9c9895]">Opens with</dt>
            <dd class="m-0 flex min-w-0 items-center justify-end gap-1.5 text-[#e8e5e2]" title={defaultApp.path}>
              {#if defaultApp.icon}<img src={defaultApp.icon} alt="" class="size-4 shrink-0" draggable="false" />{/if}
              <span class="truncate">{defaultApp.name}</span>
            </dd>
          {/if}
        </dl>
      {/if}
    </div>
  {/if}
</section>
