<script lang="ts">
  import { formatDate, formatSize } from "./format.js";
  import { kindLabel } from "./languages.js";
  import type { FilePreview } from "./types.js";
  import type { OpenWithApp } from "#lib/file-ops/open.js";

  // While `pending`, `preview` belongs to the previous file: its rows stay in the
  // layout (hidden) so the panel height does not jump between selections.
  type Props = { preview: FilePreview | null; defaultApp?: OpenWithApp | null; pending?: boolean };
  let { preview, defaultApp = null, pending = false }: Props = $props();
</script>

{#if preview}
  <section class="preview-info" class:invisible={pending} aria-hidden={pending} aria-label="File information">
    <span class="preview-kicker">Information</span>
    <dl>
      <div class="preview-info-row">
        <dt>Kind</dt>
        <dd>{kindLabel(preview.name, preview.kind)}</dd>
      </div>
      {#if preview.kind !== "directory"}
        <div class="preview-info-row">
          <dt>Size</dt>
          <dd>{formatSize(preview.size)}</dd>
        </div>
      {/if}
      {#if preview.created !== null}
        <div class="preview-info-row">
          <dt>Created</dt>
          <dd>{formatDate(preview.created)}</dd>
        </div>
      {/if}
      {#if preview.modified !== null}
        <div class="preview-info-row">
          <dt>Modified</dt>
          <dd>{formatDate(preview.modified)}</dd>
        </div>
      {/if}
      {#if defaultApp}
        <div class="preview-info-row">
          <dt>Opens with</dt>
          <dd class="preview-info-app" title={defaultApp.path}>
            {#if defaultApp.icon}<img src={defaultApp.icon} alt="" draggable="false" />{/if}
            <span>{defaultApp.name}</span>
          </dd>
        </div>
      {/if}
    </dl>
  </section>
{/if}
