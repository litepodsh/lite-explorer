<script lang="ts">
  import AlertCircleIcon from "@lucide/svelte/icons/circle-alert";
  import CheckIcon from "@lucide/svelte/icons/check";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";
  import MinusIcon from "@lucide/svelte/icons/minus";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import XIcon from "@lucide/svelte/icons/x";
  import DialogButton from "./dialog-button.svelte";
  import DialogShell from "./dialog-shell.svelte";
  import Segmented from "./segmented.svelte";
  import type { DirectoryEntry } from "#lib/components/custom/file-list/index.js";
  import { Switch } from "#lib/components/ui/switch/index.js";
  import { createLatest } from "#lib/file-ops/convert-dialog-model.js";
  import { renamePlan } from "#lib/file-ops/rename.js";
  import {
    STEP_TYPES,
    canApply,
    diffNames,
    moveStep,
    newStep,
    removeStep,
    stepLabel,
    summarize,
    summaryText,
    toWire,
    type Position,
    type RenamePlan,
    type RenameStep,
    type StepType,
  } from "#lib/file-ops/rename-steps.js";

  let {
    open = $bindable(false),
    targets,
    onRename,
  }: {
    open?: boolean;
    targets: DirectoryEntry[];
    onRename: (steps: RenameStep[]) => void;
  } = $props();

  let steps = $state<RenameStep[]>([]);
  let plan = $state<RenamePlan | null>(null);
  let planning = $state(false);
  let failure = $state("");
  let wasOpen = $state(false);

  const latest = createLatest();
  const summary = $derived(plan ? summarize(plan.items) : null);
  const applicable = $derived(canApply(plan) && !planning);
  const stepErrors = $derived(new Map((plan?.stepErrors ?? []).map((error) => [error.index, error.message])));

  const TARGETS = [
    { value: "name", label: "Name" },
    { value: "extension", label: "Extension" },
    { value: "both", label: "Both" },
  ] as const;
  const POSITIONS = [
    { value: "start", label: "Start" },
    { value: "end", label: "End" },
    { value: "index", label: "At…" },
  ] as const;
  const CASES = [
    { value: "upper", label: "ABC" },
    { value: "lower", label: "abc" },
    { value: "title", label: "Title" },
    { value: "sentence", label: "Sentence" },
  ] as const;
  const SPACES = [
    { value: "keep", label: "Keep" },
    { value: "dash", label: "Dash -" },
    { value: "underscore", label: "Under _" },
    { value: "remove", label: "Remove" },
  ] as const;
  const SOURCES = [
    { value: "created", label: "Created" },
    { value: "modified", label: "Modified" },
    { value: "exif", label: "Photo" },
    { value: "today", label: "Today" },
  ] as const;

  $effect(() => {
    if (open && !wasOpen) {
      steps = [newStep("replace")];
      plan = null;
      failure = "";
    }
    wasOpen = open;
  });

  // The preview, debounced; only the newest answer is used.
  $effect(() => {
    toWire(steps); // Read every field so edits re-run this effect.
    const current = $state.snapshot(steps) as RenameStep[];
    if (!open || targets.length === 0) {
      planning = false;
      return;
    }
    planning = true;
    const token = latest.next();
    const timer = setTimeout(() => {
      renamePlan(
        targets.map((target) => target.path),
        current,
      )
        .then((result) => {
          if (!latest.isCurrent(token)) return;
          plan = result;
          failure = "";
        })
        .catch((error) => {
          if (latest.isCurrent(token)) failure = error instanceof Error ? error.message : String(error);
        })
        .finally(() => {
          if (latest.isCurrent(token)) planning = false;
        });
    }, 150);
    return () => clearTimeout(timer);
  });

  function add(type: StepType) {
    steps = [...steps, newStep(type)];
  }

  function setPosition(index: number, kind: Position["kind"]) {
    const step = steps[index];
    if (step.type !== "insert" && step.type !== "number" && step.type !== "date") return;
    step.position = kind === "index" ? { kind, at: 0 } : { kind };
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!applicable) return;
    onRename($state.snapshot(steps) as RenameStep[]);
  }
</script>

{#snippet positionField(index: number, position: Position)}
  <Segmented
    options={POSITIONS}
    value={position.kind}
    label="Position"
    onchange={(kind) => setPosition(index, kind)} />
  {#if position.kind === "index"}
    <label class="inline">
      After character
      <input
        type="number"
        min="0"
        step="1"
        value={position.at}
        oninput={(event) => {
          const step = steps[index];
          if (step.type === "insert" || step.type === "number" || step.type === "date") {
            step.position = { kind: "index", at: Number(event.currentTarget.value) };
          }
        }} />
    </label>
  {/if}
{/snippet}

<DialogShell bind:open width={1040} title={`Rename ${targets.length} items`}>
  <form class="layout" id="rename-form" onsubmit={submit}>
    <div class="steps">
      <div class="adders" role="group" aria-label="Add a step">
        {#each STEP_TYPES as entry (entry.type)}
          <button type="button" class="chip" onclick={() => add(entry.type)}>
            <PlusIcon />
            {entry.label}
          </button>
        {/each}
      </div>

      {#each steps as step, index (step.id)}
        <section class="card" class:off={!step.enabled} class:bad={stepErrors.has(index)}>
          <header>
            <Switch
              size="sm"
              checked={step.enabled}
              aria-label={`Use ${stepLabel(step.type)}`}
              onCheckedChange={(value) => (step.enabled = value)} />
            <h3>{stepLabel(step.type)}</h3>
            <span class="tools">
              <button
                type="button"
                class="icon"
                aria-label="Move up"
                disabled={index === 0}
                onclick={() => (steps = moveStep(steps, index, -1))}>
                <ChevronUpIcon />
              </button>
              <button
                type="button"
                class="icon"
                aria-label="Move down"
                disabled={index === steps.length - 1}
                onclick={() => (steps = moveStep(steps, index, 1))}>
                <ChevronDownIcon />
              </button>
              <button
                type="button"
                class="icon"
                aria-label="Remove step"
                onclick={() => (steps = removeStep(steps, index))}>
                <XIcon />
              </button>
            </span>
          </header>

          {#if step.type === "replace"}
            <div class="two">
              <label>Find<input bind:value={step.find} spellcheck="false" autocomplete="off" /></label>
              <label>Replace with<input bind:value={step.with} spellcheck="false" autocomplete="off" /></label>
            </div>
            <Segmented
              options={TARGETS}
              value={step.target}
              label="Apply to"
              onchange={(value) => (step.target = value)} />
            <div class="checks">
              <label class="check"><input type="checkbox" bind:checked={step.regex} /> Regular expression</label>
              <label class="check"><input type="checkbox" bind:checked={step.caseSensitive} /> Match case</label>
            </div>
          {:else if step.type === "insert"}
            <label>Text<input bind:value={step.text} spellcheck="false" autocomplete="off" /></label>
            {@render positionField(index, step.position)}
          {:else if step.type === "number"}
            <div class="three">
              <label>Start<input type="number" step="1" bind:value={step.start} /></label>
              <label>Step<input type="number" step="1" bind:value={step.step} /></label>
              <label>Digits<input type="number" min="0" max="12" step="1" bind:value={step.padding} /></label>
            </div>
            <label>Separator<input bind:value={step.separator} spellcheck="false" autocomplete="off" /></label>
            {@render positionField(index, step.position)}
          {:else if step.type === "case"}
            <Segmented
              options={CASES}
              value={step.mode}
              label="Case"
              onchange={(value) => (step.mode = value)} />
          {:else if step.type === "clean"}
            <div class="checks">
              <label class="check"><input type="checkbox" bind:checked={step.trim} /> Trim ends</label>
              <label class="check"><input type="checkbox" bind:checked={step.collapseSpaces} /> Collapse spaces</label>
              <label class="check"><input type="checkbox" bind:checked={step.stripAccents} /> Remove accents</label>
            </div>
            <Segmented
              options={SPACES}
              value={step.spaces}
              label="Spaces"
              onchange={(value) => (step.spaces = value)} />
          {:else if step.type === "date"}
            <Segmented
              options={SOURCES}
              value={step.source}
              label="Date from"
              onchange={(value) => (step.source = value)} />
            <div class="two">
              <label>
                Format
                <input bind:value={step.format} spellcheck="false" autocomplete="off" />
              </label>
              <label>Separator<input bind:value={step.separator} spellcheck="false" autocomplete="off" /></label>
            </div>
            <p class="help">YYYY year · MM month · DD day · HH hour · mm minute · ss second</p>
            {@render positionField(index, step.position)}
          {/if}

          {#if stepErrors.has(index)}
            <p class="error" role="alert"><AlertCircleIcon />{stepErrors.get(index)}</p>
          {/if}
        </section>
      {/each}

      {#if steps.length === 0}
        <p class="help">Add a step to start renaming.</p>
      {/if}
    </div>

    <div class="preview">
      <p class="summary" role="status">
        {#if failure}
          <span class="bad-text">{failure}</span>
        {:else if summary}
          {summaryText(summary)}
          {#if planning}<span class="muted"> · updating…</span>{/if}
        {:else}
          Calculating…
        {/if}
      </p>
      <ul class="rows" aria-label="Preview">
        {#each plan?.items ?? [] as row (row.path)}
          {@const diff = diffNames(row.oldName, row.newName)}
          <li class="row {row.status}">
            <span class="mark" aria-hidden="true">
              {#if row.status === "ok"}
                <CheckIcon />
              {:else if row.status === "unchanged"}
                <MinusIcon />
              {:else}
                <AlertCircleIcon />
              {/if}
            </span>
            <span class="names">
              {#if row.status === "unchanged"}
                <span class="old" title={row.oldName}>{row.oldName}</span>
              {:else}
                <span class="old" title={row.oldName}>{row.oldName}</span>
                <span class="new" title={row.newName}>
                  {diff.prefix}<mark>{diff.newMiddle}</mark>{diff.suffix}
                </span>
              {/if}
              {#if row.reason}
                <span class="reason">{row.reason}</span>
              {/if}
            </span>
          </li>
        {/each}
      </ul>
    </div>
  </form>
  {#snippet footer()}
    <DialogButton onclick={() => (open = false)}>Cancel</DialogButton>
    <DialogButton type="submit" form="rename-form" variant="primary" disabled={!applicable}>
      {summary && summary.change > 0 ? `Rename ${summary.change}` : "Rename"}
    </DialogButton>
  {/snippet}
</DialogShell>

<style>
  .layout {
    display: grid;
    gap: 14px;
    margin-top: 6px;
  }
  @media (min-width: 900px) {
    .layout {
      grid-template-columns: 400px minmax(0, 1fr);
      gap: 18px;
      align-items: start;
    }
  }
  .steps,
  .preview {
    display: grid;
    gap: 8px;
    min-width: 0;
    align-content: start;
  }
  .adders {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 9px 0 6px;
    border: 0;
    border-radius: calc(var(--app-radius) - 5px);
    background: var(--app-surface-raised);
    box-shadow: inset 0 0 0 1px var(--app-border-strong);
    color: var(--app-fg);
    font: inherit;
    font-size: 12px;
  }
  .chip :global(svg) {
    width: 13px;
    height: 13px;
  }
  .chip:hover {
    background: var(--app-hover-strong);
  }
  .chip:focus-visible,
  .icon:focus-visible {
    outline: 2px solid var(--app-accent);
    outline-offset: 1px;
  }
  .steps {
    max-height: min(66vh, 560px);
    overflow-y: auto;
    padding-right: 4px;
  }
  .card {
    display: grid;
    gap: 8px;
    padding: 10px;
    border-radius: calc(var(--app-radius) - 4px);
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
  }
  .card.off > :not(header) {
    opacity: 0.45;
    pointer-events: none;
  }
  .card.bad {
    box-shadow: inset 0 0 0 1px var(--app-danger);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h3 {
    flex: 1;
    margin: 0;
    font-size: 12px;
    font-weight: 600;
  }
  .tools {
    display: flex;
    gap: 2px;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--app-fg-muted);
  }
  .icon :global(svg) {
    width: 14px;
    height: 14px;
  }
  .icon:hover:not(:disabled) {
    background: var(--app-hover-strong);
    color: var(--app-fg);
  }
  .icon:disabled {
    opacity: 0.3;
  }
  label {
    display: grid;
    gap: 4px;
    color: var(--app-fg-muted);
    font-size: 11px;
    font-weight: 500;
  }
  label.inline {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  label.check {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--app-fg);
    font-size: 12px;
    font-weight: 400;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .three {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .checks {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
  }
  input:not([type="checkbox"]) {
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: calc(var(--app-radius) - 5px);
    outline: none;
    background: var(--app-surface);
    box-shadow: inset 0 0 0 1px var(--app-border);
    color: var(--app-fg);
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  label.inline input {
    width: 72px;
  }
  input:not([type="checkbox"]):focus {
    box-shadow:
      inset 0 0 0 1px var(--app-accent),
      0 0 0 3px color-mix(in srgb, var(--app-accent) 18%, transparent);
  }
  .help {
    margin: 0;
    color: var(--app-fg-faint);
    font-size: 11px;
  }
  .error {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 0;
    color: var(--app-danger-fg);
    font-size: 12px;
  }
  .error :global(svg) {
    flex-shrink: 0;
    width: 14px;
    height: 14px;
    margin-top: 1px;
  }
  .summary {
    margin: 0;
    color: var(--app-fg);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .muted {
    color: var(--app-fg-faint);
  }
  .bad-text {
    color: var(--app-danger-fg);
  }
  .rows {
    display: grid;
    gap: 2px;
    max-height: min(62vh, 520px);
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    list-style: none;
    border-radius: calc(var(--app-radius) - 4px);
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
  }
  .row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 5px 6px;
    border-radius: 6px;
    font-size: 12px;
  }
  .row.conflict,
  .row.invalid {
    background: color-mix(in srgb, var(--app-danger) 12%, transparent);
  }
  .mark {
    flex-shrink: 0;
    margin-top: 1px;
    color: var(--app-fg-faint);
  }
  .mark :global(svg) {
    width: 14px;
    height: 14px;
  }
  .row.ok .mark {
    color: var(--app-success);
  }
  .row.conflict .mark,
  .row.invalid .mark {
    color: var(--app-danger-fg);
  }
  .names {
    display: grid;
    min-width: 0;
    gap: 1px;
  }
  .old,
  .new {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .old {
    color: var(--app-fg-muted);
  }
  .row.unchanged .old {
    color: var(--app-fg-faint);
  }
  .new {
    color: var(--app-fg);
    font-weight: 500;
  }
  mark {
    padding: 0 1px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--app-accent) 28%, transparent);
    color: inherit;
  }
  .reason {
    color: var(--app-danger-fg);
    font-size: 11px;
  }
</style>
