<script lang="ts">
  import type { Snippet } from "svelte";
  import { Switch } from "$lib/components/ui/switch/index.js";

  let {
    checked = false,
    disabled = false,
    label,
    description,
    onchange,
  }: {
    checked?: boolean;
    disabled?: boolean;
    label: string;
    description?: Snippet | string;
    onchange: (checked: boolean) => void;
  } = $props();

  const id = $props.id();
</script>

<div class="row" class:disabled>
  <div class="copy">
    <label id={`${id}-label`} for={id}>{label}</label>
    {#if typeof description === "string"}
      <p>{description}</p>
    {:else if description}
      <p>{@render description()}</p>
    {/if}
  </div>
  <Switch
    {id}
    aria-labelledby={`${id}-label`}
    size="sm"
    class="mt-px shrink-0"
    onCheckedChange={onchange}
    {checked}
    {disabled}
  />
</div>

<style>
  .row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  .copy {
    min-width: 0;
  }
  label {
    color: var(--app-fg);
    font-size: 12.5px;
    font-weight: 500;
  }
  p {
    margin: 3px 0 0;
    color: var(--app-fg-muted);
    font-size: 11.5px;
    line-height: 1.4;
  }
  .row.disabled label {
    color: var(--app-fg-muted);
  }
</style>
