<script lang="ts">
  import CheckIcon from "@lucide/svelte/icons/check";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import Clock3Icon from "@lucide/svelte/icons/clock-3";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import GaugeIcon from "@lucide/svelte/icons/gauge";
  import NetworkIcon from "@lucide/svelte/icons/network";
  import UsersIcon from "@lucide/svelte/icons/users";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import { breadcrumbSegments, collapseSegments } from "#lib/components/custom/status-bar/breadcrumb.js";
  import type { Location } from "#lib/tabs/tabs.js";

  let {
    location,
    path = "",
    display = "",
    label = "",
    network = false,
    editable = false,
    onNavigate,
    onEdit,
  }: {
    /** The active tab's location; Overview, Recents and Shared show their name instead of a path. */
    location: Location;
    /** The internal path of the listing; breadcrumb segments open folders of it. */
    path?: string;
    /** The path as shown, like an `s3://bucket` URI or a share's server address. */
    display?: string;
    /** The location's name, shown when there is no path. */
    label?: string;
    network?: boolean;
    /** Clicking the bar opens the command palette on this path. */
    editable?: boolean;
    onNavigate?: (path: string) => void;
    onEdit?: () => void;
  } = $props();

  const segments = $derived(path ? breadcrumbSegments(path, display) : []);
  const crumbs = $derived(collapseSegments(segments));
  const Icon = $derived(
    location.kind === "overview"
      ? GaugeIcon
      : location.kind === "recents"
        ? Clock3Icon
        : location.kind === "shared"
          ? UsersIcon
          : network
            ? NetworkIcon
            : FolderIcon,
  );

  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  async function copyPath(event: MouseEvent) {
    event.stopPropagation();
    // WebKit rejects clipboard writes without a fresh user activation.
    try {
      await navigator.clipboard.writeText(display || path);
    } catch {
      return;
    }
    copied = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied = false), 1500);
  }

  /** A click on the bar's empty space edits the path; clicks on a folder navigate instead. */
  function edit(event: MouseEvent) {
    if (!editable || (event.target instanceof Element && event.target.closest("button, [role='menu']"))) return;
    onEdit?.();
  }
</script>

{#snippet crumb(segment: { label: string; path: string }, current: boolean)}
  {#if current}
    <span class="path-crumb path-current" aria-current="location" title={segment.path}>{segment.label}</span>
  {:else}
    <button type="button" class="path-crumb" title={segment.path} onclick={() => onNavigate?.(segment.path)}>{segment.label}</button>
    <ChevronRightIcon class="path-separator" aria-hidden="true" />
  {/if}
{/snippet}

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="path-bar" class:editable title={editable ? "Click to type a path" : undefined} onclick={edit}>
  <Icon class="path-icon" />
  {#if segments.length}
    <nav class="path-crumbs" aria-label="Path">
      {#each crumbs.head as segment, index (segment.path)}
        {@render crumb(segment, index === segments.length - 1)}
      {/each}
      {#if crumbs.hidden.length}
        <DropdownMenu.Root>
          <DropdownMenu.Trigger class="path-crumb" aria-label="Show hidden folders">…</DropdownMenu.Trigger>
          <DropdownMenu.Content align="start" class="w-auto max-w-80">
            {#each crumbs.hidden as segment (segment.path)}
              <DropdownMenu.Item onSelect={() => onNavigate?.(segment.path)}>
                <FolderIcon /><span class="truncate">{segment.label}</span>
              </DropdownMenu.Item>
            {/each}
          </DropdownMenu.Content>
        </DropdownMenu.Root>
        <ChevronRightIcon class="path-separator" aria-hidden="true" />
      {/if}
      {#each crumbs.tail as segment, index (segment.path)}
        {@render crumb(segment, index === crumbs.tail.length - 1)}
      {/each}
    </nav>
  {:else}
    <span class="path-crumb path-current">{label}</span>
  {/if}
  {#if path}
    <button
      type="button"
      class="path-copy"
      class:copied
      aria-label={copied ? "Path copied" : "Copy path"}
      title={copied ? "Copied" : "Copy path"}
      onclick={copyPath}>
      {#if copied}<CheckIcon />{:else}<CopyIcon />{/if}
    </button>
  {/if}
</div>

<style>
  .path-bar {
    position: relative;
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    gap: 6px;
    min-width: 0;
    height: 34px;
    padding: 0 6px 0 12px;
    border-radius: 999px;
    background: var(--app-well-fill);
    box-shadow: var(--app-well);
    color: var(--app-fg-muted);
    font-family: var(--app-font-mono);
    font-size: 12px;
    white-space: nowrap;
  }
  .path-bar.editable {
    cursor: text;
  }
  .path-bar :global(.path-icon) {
    flex: none;
    width: 15px;
    height: 15px;
    color: var(--app-folder);
    stroke-width: 1.8;
  }
  .path-crumbs {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    min-width: 0;
    overflow: hidden;
  }
  /* The toolbar sizes its buttons as round icon buttons; folder names size to their text. */
  .path-bar :global(.path-crumb) {
    display: block;
    flex: none;
    width: auto;
    height: auto;
    max-width: 14rem;
    overflow: hidden;
    padding: 0 5px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: inherit;
    font: inherit;
    line-height: 22px;
    text-overflow: ellipsis;
    cursor: default;
  }
  .path-bar :global(button.path-crumb:hover),
  .path-bar :global(button.path-crumb[data-state="open"]) {
    background: var(--app-hover-strong);
    color: var(--app-fg);
  }
  .path-bar :global(.path-current) {
    flex: 0 1 auto;
    min-width: 0;
    max-width: none;
    color: var(--app-fg);
    font-weight: 500;
    cursor: inherit;
    user-select: text;
  }
  .path-bar :global(.path-separator) {
    flex: none;
    width: 12px;
    height: 12px;
    color: var(--app-fg-faint);
    stroke-width: 2;
  }
  .path-copy {
    display: grid;
    flex: none;
    width: 24px;
    height: 24px;
    margin-left: auto;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--app-fg-muted);
    opacity: 0;
    transition: opacity 140ms ease;
  }
  .path-copy :global(svg) {
    width: 13px;
    height: 13px;
  }
  .path-bar:hover .path-copy,
  .path-copy:focus-visible,
  .path-copy.copied {
    opacity: 1;
  }
  .path-copy:hover {
    background: var(--app-hover-strong);
    color: var(--app-fg);
  }
  .path-copy.copied {
    color: var(--app-success);
  }
</style>
