<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { activity } from "$lib/transfers/jobs.js";
  import DialogShell from "$lib/components/custom/dialog/dialog-shell.svelte";
  import DialogButton from "$lib/components/custom/dialog/dialog-button.svelte";
  import EntryIcon from "$lib/file-icons/entry-icon.svelte";
  import type { DirectoryEntry } from "$lib/components/custom/file-list/list-item.svelte";
  import { entryType } from "$lib/components/custom/file-list/sort.js";
  import { fetchDefaultApp, type OpenWithApp } from "$lib/file-ops/open.js";
  import { formatSize } from "./format.js";

  type FileInfo = {
    size: number | null; created: number | null; modified: number | null; accessed: number | null;
    readonly: boolean; details: [string, string][]; permissions: [string, string][];
    sections: { title: string; rows: [string, string][] }[];
  };
  type ExifInfo = { fields: { tag: string; description: string; group: string; value: string }[]; warning: string | null; can_remove: boolean; removal_reason: string | null; revision: string };
  let { open = $bindable(false), entry, onChanged }: { open?: boolean; entry: DirectoryEntry | null; onChanged?: () => void } = $props();
  let exif = $state<ExifInfo | null>(null);
  let exifPending = $state(false);
  let exifError = $state("");
  let confirmRemoval = $state(false);
  let removing = $state(false);
  let removed = $state(false);
  let query = $state("");
  const supportsExif = $derived(!!entry && !entry.is_directory && /\.(jpe?g|jpe|png|webp|tiff?|heic|heif|avif|dng|cr2|nef|arw|orf|rw2)$/i.test(entry.name));
  const filteredExif = $derived((exif?.fields ?? []).filter(field => `${field.tag} ${field.description} ${field.group} ${field.value}`.toLowerCase().includes(query.toLowerCase())));
  $effect(() => {
    if (!open || !entry) return;
    const path = entry.path;
    exif = null; exifError = ""; confirmRemoval = false; removed = false; query = "";
    const shouldRead = supportsExif && !remote;
    exifPending = shouldRead;
    let cancelled = false;
    if (shouldRead) invoke<ExifInfo>("read_file_exif", { path })
      .then(value => { if (!cancelled) exif = value; })
      .catch(reason => { if (!cancelled) exifError = String(reason); })
      .finally(() => { if (!cancelled) exifPending = false; });
    return () => { cancelled = true; };
  });
  async function removeExif() {
    if (!entry || !exif?.can_remove || removing) return;
    const path = entry.path, revision = exif.revision;
    removing = true; exifError = "";
    try {
      await activity.action("Remove metadata", path, () => invoke("remove_file_exif", { path, revision }));
      onChanged?.();
      const [nextInfo, nextExif] = await Promise.all([
        invoke<FileInfo>("read_file_info", { path }), invoke<ExifInfo>("read_file_exif", { path }),
      ]);
      if (open && entry?.path === path) { info = nextInfo; exif = nextExif; removed = true; query = ""; }
    } catch (reason) {
      if (open && entry?.path === path) exifError = String(reason);
    } finally { removing = false; confirmRemoval = false; }
  }
  let info = $state<FileInfo | null>(null);
  let app = $state<OpenWithApp | null>(null);
  let error = $state("");
  let pending = $state(false);
  let appPending = $state(false);
  const remote = $derived(!!entry && /^[a-z][a-z0-9+.-]*:\/\//i.test(entry.path));
  const timestamp = (value: number | null | undefined) => value == null ? "Unavailable" : new Date(value).toLocaleString(undefined, { dateStyle: "full", timeStyle: "medium" });
  const size = $derived(info ? info.size : entry?.size);
  const extension = $derived(entry && !entry.is_directory && entry.name.includes(".") ? entry.name.split(".").pop() : null);

  $effect(() => {
    if (!open || !entry) return;
    const path = entry.path;
    info = null; app = null; error = ""; pending = !remote; appPending = !remote && !entry.is_directory;
    let cancelled = false;
    if (!remote) {
      invoke<FileInfo>("read_file_info", { path })
        .then(value => { if (!cancelled) info = value; })
        .catch(reason => { if (!cancelled) error = String(reason); })
        .finally(() => { if (!cancelled) pending = false; });
      if (!entry.is_directory) fetchDefaultApp(path)
        .then(value => { if (!cancelled) app = value; })
        .catch(() => {})
        .finally(() => { if (!cancelled) appPending = false; });
    }
    return () => { cancelled = true; };
  });
</script>

<DialogShell bind:open title="Get Info" width={460}>
  {#if entry}
    <div class="identity">
      <div class="file-emblem">
        {#if entry.is_directory}
          <svg viewBox="0 0 32 32" fill="none" stroke="currentColor" stroke-width="1.2" aria-hidden="true"><path d="M3 10V7a2 2 0 0 1 2-2h8l3 4h11a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V10Z" /></svg>
        {:else}<EntryIcon path={entry.path} name={entry.name} native={!remote} />{/if}
      </div>
      <div class="identity-text"><h2>{entry.name}</h2><p>{entryType(entry)}{size != null ? ` · ${formatSize(size)}` : ""}</p></div>
    </div>
    <div class="sections" aria-busy={pending}>
      {#if pending}<p class="notice" role="status">Reading information…</p>{/if}
      {#if error}<p class="notice error" role="alert">Could not read file information: {error}</p>{/if}
      <details open>
        <summary>General</summary>
        <dl>
          <dt>Kind</dt><dd>{entry.is_directory ? entryType(entry) : extension ? `${extension.toUpperCase()} file` : "File"}</dd>
          <dt>Size</dt><dd>{size == null ? entry.is_directory ? "Not calculated" : "Unavailable" : `${formatSize(size)} (${size.toLocaleString()} bytes)`}</dd>
          <dt>Where</dt><dd>{entry.path}</dd>
          <dt>Created</dt><dd>{timestamp(info ? info.created : entry.created)}</dd>
          <dt>Modified</dt><dd>{timestamp(info ? info.modified : entry.modified)}</dd>
          {#if info?.accessed != null}<dt>Last accessed</dt><dd>{timestamp(info.accessed)}</dd>{/if}
        </dl>
      </details>
      <details open>
        <summary>More Information</summary>
        <dl>
          <dt>Name</dt><dd>{entry.name}</dd>
          {#if extension}<dt>Extension</dt><dd>.{extension}</dd>{/if}
          <dt>Hidden</dt><dd>{entry.is_hidden ? "Yes" : "No"}</dd>
          <dt>Location</dt><dd>{remote ? "Remote" : "Local filesystem"}</dd>
          {#each info?.details ?? [] as [label, value]}<dt>{label}</dt><dd>{value}</dd>{/each}
        </dl>
      </details>
      {#each info?.sections ?? [] as section (section.title)}
        <details open>
          <summary>{section.title}</summary>
          <dl>{#each section.rows as [label, value]}<dt>{label}</dt><dd>{value}</dd>{/each}</dl>
        </details>
      {/each}
      {#if supportsExif}
        <details open>
          <summary>EXIF Metadata{exif?.fields.length ? ` · ${exif.fields.length} fields` : ""}</summary>
          {#if exifPending}<p class="notice" role="status">Reading EXIF metadata…</p>{/if}
          {#if exifError}<p class="notice error" role="alert">{exifError}</p>{/if}
          {#if exif?.warning}<p class="notice">{exif.warning}</p>{/if}
          {#if removed}<p class="notice" role="status">EXIF metadata removed.</p>{/if}
          {#if remote}<p class="notice">Download this image to inspect or remove its EXIF metadata.</p>
          {:else if exif}
            {#if exif.fields.length}
              <input class="metadata-search" aria-label="Filter EXIF metadata" placeholder="Find a tag or value…" bind:value={query} />
              <dl class="exif-fields">
                {#each filteredExif as field}
                  <dt title={field.description}>{field.tag}<small>{field.group}</small></dt><dd>{field.value}</dd>
                {/each}
              </dl>
              {#if !filteredExif.length}<p class="notice">No matching EXIF fields.</p>{/if}
            {:else if !exif.warning}<p class="notice">This image has no EXIF metadata.</p>{/if}
            <div class="exif-actions">
              {#if confirmRemoval}
                <p class="notice">Remove EXIF from the original file? Camera, GPS, capture date and orientation tags will be deleted. Displayed rotation may change. Image pixels are not recompressed. Other metadata, such as XMP or IPTC, is kept.</p>
                <div class="confirm-actions"><DialogButton disabled={removing} onclick={() => confirmRemoval = false}>Cancel</DialogButton><DialogButton disabled={removing} onclick={removeExif}>{removing ? "Removing…" : "Remove EXIF from original"}</DialogButton></div>
              {:else}
                <DialogButton disabled={!exif.can_remove || removing} onclick={() => confirmRemoval = true}>Remove EXIF metadata…</DialogButton>
                {#if exif.removal_reason}<p class="notice reason">{exif.removal_reason}</p>{/if}
              {/if}
            </div>
          {/if}
        </details>
      {/if}
      {#if !entry.is_directory && !remote}
        <details open><summary>Open With</summary>
          <div class="application">{#if app?.icon}<img src={app.icon} alt="" />{/if}<span>{appPending ? "Looking up application…" : app?.name ?? "No default application available"}</span></div>
        </details>
      {/if}
      <details>
        <summary>Permissions</summary>
        {#if info}<dl>
          <dt>Read-only flag</dt><dd>{info.readonly ? "Yes" : "No"}</dd>
          {#each info.permissions as [label, value]}<dt>{label}</dt><dd>{value}</dd>{/each}
        </dl>
        {:else}<p class="notice">{pending ? "Reading permissions…" : "Permissions are unavailable for this item."}</p>{/if}
      </details>
    </div>
  {/if}
  {#snippet footer()}<DialogButton disabled={removing} onclick={() => open = false}>Done</DialogButton>{/snippet}
</DialogShell>

<style>
  .metadata-search { width: 100%; margin: 0 0 14px; padding: 8px 10px; border: 0; border-radius: 7px; background: rgb(0 0 0 / 14%); color: inherit; font: inherit; font-size: 12px; box-shadow: inset 0 0 0 1px rgb(255 255 255 / 8%); }
  .metadata-search:focus-visible { outline: 2px solid #70b7ff; }
  .exif-fields { grid-template-columns: minmax(0, 1fr) minmax(0, 1.2fr); }
  .exif-fields dt { overflow-wrap: anywhere; color: #dedbd8; }
  small { display: block; color: #928d89; font-size: 10px; }
  .exif-actions { padding: 0 0 14px 14px; }
  .confirm-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
  .reason { margin-top: 7px; padding-bottom: 0; font-size: 11px; }
  .identity { display: flex; align-items: center; gap: 14px; padding: 12px 0 18px; }
  .file-emblem { display: grid; place-items: center; width: 48px; height: 48px; flex-shrink: 0; border-radius: 12px; background: rgb(255 255 255 / 4%); box-shadow: inset 0 1px rgb(255 255 255 / 8%); color: #8ac4ff; }
  .file-emblem svg { width: 30px; height: 30px; }
  .identity-text { min-width: 0; }
  h2 { margin: 0; font-size: 17px; font-weight: 600; overflow-wrap: anywhere; user-select: text; }
  p { margin: 4px 0 0; color: #aaa6a2; font-size: 12px; }
  .sections { max-height: min(60dvh, 580px); overflow: auto; scrollbar-gutter: stable; }
  details { box-shadow: inset 0 1px rgb(255 255 255 / 7%); }
  summary { cursor: pointer; padding: 12px 0; font-size: 12px; font-weight: 600; }
  summary::marker { color: #aaa6a2; font-size: 10px; }
  summary:focus-visible { outline: 2px solid #70b7ff; outline-offset: -2px; border-radius: 4px; }
  dl { display: grid; grid-template-columns: 110px minmax(0, 1fr); gap: 9px 14px; margin: 0; padding: 0 0 16px 14px; font-size: 12px; line-height: 1.5; user-select: text; }
  dt { color: #aaa6a2; } dd { margin: 0; overflow-wrap: anywhere; }
  details[open] dl, details[open] .application { animation: reveal 180ms cubic-bezier(.32,.72,0,1); }
  .notice { margin: 0; padding: 0 0 12px; line-height: 1.5; } .error { color: #f5b5aa; }
  .application { display: flex; align-items: center; gap: 8px; padding: 0 0 16px 14px; font-size: 12px; } .application img { width: 24px; height: 24px; }
  @keyframes reveal { from { opacity: 0; transform: translateY(-3px); } to { opacity: 1; transform: translateY(0); } }
  @media (prefers-reduced-motion: reduce) { details[open] dl, details[open] .application { animation: none; } }
</style>
