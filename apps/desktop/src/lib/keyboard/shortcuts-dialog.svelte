<script lang="ts">
  import Fuse from "fuse.js";
  import KeyboardIcon from "@lucide/svelte/icons/keyboard";
  import * as Command from "$lib/components/ui/command/index.js";
  import * as Kbd from "$lib/components/ui/kbd/index.js";
  import "$lib/components/custom/command-surface.css";
  import { shortcutsDialogState } from "$lib/state/shortcuts-dialog.svelte.js";
  import { highlightMatches } from "$lib/utils/highlight-matches.js";
  import { eventToken, formatToken, normalizeToken, type KeyPlatform } from "./keys.js";
  import type { Binding, Mode } from "./keymap.js";
  import type { Scope } from "./scope.js";
  import { CATEGORY_ORDER, descMatch, filterShortcuts, shortcutRows, type ShortcutRow } from "./shortcuts-filter.js";

  let {
    bindings,
    platform,
    mode,
    available,
    onToggleMode,
  }: {
    bindings: Binding[];
    platform: KeyPlatform;
    mode: Mode;
    /** Whether the shortcut's command can run where the dialog was opened. */
    available: (row: ShortcutRow) => boolean;
    onToggleMode: () => void;
  } = $props();

  type Filter = "all" | "standard" | "yazi";
  const FILTERS: { value: Filter; label: string }[] = [
    { value: "all", label: "All" },
    { value: "standard", label: "Standard" },
    { value: "yazi", label: "Yazi" },
  ];
  const SCOPE_LABELS: Record<Scope, string> = {
    global: "Global",
    list: "List",
    sidebar: "Sidebar",
    preview: "Preview",
    dialog: "Dialog",
    input: "Text field",
    monaco: "Code",
  };

  let filter = $state<Filter>("all");
  let recording = $state(false);
  let input = $state<HTMLInputElement | null>(null);

  const recordLabel = $derived(formatToken(normalizeToken("Mod+K", platform), platform));
  const rows = $derived(shortcutRows(bindings, platform));
  const filtered = $derived(filter === "all" ? rows : rows.filter((row) => row.mode === filter));
  const trimmed = $derived(shortcutsDialogState.query.trim());
  const results = $derived(
    filterShortcuts(filtered, shortcutsDialogState.query, platform, (rest, query) =>
      new Fuse(rest, { keys: ["desc"], threshold: 0.3, ignoreLocation: true }).search(query).map((result) => result.item),
    ),
  );
  const groups = $derived(
    CATEGORY_ORDER.map((category) => ({ category, rows: results.filter((row) => row.category === category) })).filter(
      (group) => group.rows.length > 0,
    ),
  );

  $effect(() => {
    if (shortcutsDialogState.open) return;
    recording = false;
    filter = "all";
  });

  function handleInputKeydown(event: KeyboardEvent) {
    if (recording) {
      const token = eventToken(event);
      if (!token) return;
      event.preventDefault();
      event.stopPropagation();
      recording = false;
      if (token !== "<Esc>") shortcutsDialogState.query = formatToken(token, platform);
      return;
    }
    const primary = platform === "macos" ? event.metaKey : event.ctrlKey;
    if (primary && !event.shiftKey && !event.altKey) {
      if (event.code === "KeyK") {
        event.preventDefault();
        recording = true;
        return;
      }
      const digit = ["Digit1", "Digit2", "Digit3"].indexOf(event.code);
      if (digit >= 0) {
        event.preventDefault();
        filter = FILTERS[digit].value;
        return;
      }
    }
    if (event.key === "Escape" && shortcutsDialogState.query) {
      event.preventDefault();
      event.stopPropagation();
      shortcutsDialogState.query = "";
    }
  }

  function toggleRecording() {
    recording = !recording;
    input?.focus();
  }
</script>

<Command.Dialog
  bind:open={shortcutsDialogState.open}
  shouldFilter={false}
  class="command-surface top-[12vh]! w-[min(52rem,calc(100%-2rem))] max-w-none rounded-[14px]! p-0 shadow-2xl ring-0! bg-transparent!"
  title="Keyboard Shortcuts"
  description="Search shortcuts by key or description">
  <div class="command-surface-header">
    <Command.Input
      bind:ref={input}
      bind:value={shortcutsDialogState.query}
      autofocus
      placeholder={recording ? "Press a key combination…" : "Search by key or description"}
      onkeydown={handleInputKeydown} />
    <div class="shortcut-tools">
      <button
        type="button"
        class="shortcut-record"
        aria-pressed={recording}
        aria-label="Record a key combination"
        title={`Record keys (${recordLabel})`}
        onclick={toggleRecording}><KeyboardIcon class="size-4" /></button>
      <div class="shortcut-filter" role="radiogroup" aria-label="Keyboard mode">
        {#each FILTERS as option (option.value)}
          <button type="button" role="radio" aria-checked={filter === option.value} onclick={() => (filter = option.value)}
            >{option.label}</button>
        {/each}
      </div>
    </div>
  </div>
  <Command.List>
    <Command.Empty>No shortcuts found.</Command.Empty>
    {#if trimmed}
      <Command.Group heading="Results">
        {#each results as row (row.id)}{@render shortcutRow(row)}{/each}
      </Command.Group>
    {:else}
      {#each groups as group (group.category)}
        <Command.Group heading={group.category}>
          {#each group.rows as row (row.id)}{@render shortcutRow(row)}{/each}
        </Command.Group>
      {/each}
    {/if}
  </Command.List>
  <footer class="command-surface-footer">
    <button type="button" class="shortcut-mode-toggle" onclick={onToggleMode}
      >{mode === "yazi" ? "Turn off Yazi mode" : "Turn on Yazi mode"}</button>
    <span><Kbd.Kbd>↑</Kbd.Kbd><Kbd.Kbd>↓</Kbd.Kbd> move</span>
    <span><Kbd.Kbd>{recordLabel}</Kbd.Kbd> record keys</span>
    <span><Kbd.Kbd>Esc</Kbd.Kbd> clear, close</span>
  </footer>
</Command.Dialog>

{#snippet shortcutRow(row: ShortcutRow)}
  <Command.Item value={row.id} class="shortcut-row" data-dimmed={row.mode === "yazi" && mode !== "yazi" ? "" : undefined}>
    <span class="min-w-0 flex-1 truncate">
      {#each highlightMatches(row.desc, descMatch(row.desc, trimmed)) as segment, index (index)}
        {#if segment.highlighted}<mark class="rounded-sm bg-primary/20 text-foreground">{segment.text}</mark>{:else}{segment.text}{/if}
      {/each}
    </span>
    {#if row.mode === "yazi" && mode !== "yazi"}<span class="shortcut-badge">Yazi mode</span>{/if}
    {#if !available(row)}<span class="shortcut-note">not available here</span>{/if}
    <span class="shortcut-scope">{SCOPE_LABELS[row.scope]}</span>
    <span class="shortcut-keys">
      {#each row.sequences as sequence, sequenceIndex (sequenceIndex)}
        {#if sequenceIndex > 0}<span class="shortcut-or">or</span>{/if}
        {#each sequence as token, position (position)}
          {#if position > 0}<span class="shortcut-then">›</span>{/if}
          <Kbd.Kbd>{formatToken(token, platform)}</Kbd.Kbd>
        {/each}
      {/each}
    </span>
  </Command.Item>
{/snippet}
