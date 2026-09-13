<script lang="ts">
  import type { Snippet } from "svelte";

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
  <button
    {id}
    type="button"
    role="switch"
    aria-checked={checked}
    aria-labelledby={`${id}-label`}
    class="switch"
    {disabled}
    onclick={() => onchange(!checked)}><span></span></button>
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
    color: #eceae8;
    font-size: 12.5px;
    font-weight: 500;
  }
  p {
    margin: 3px 0 0;
    color: #8f8b88;
    font-size: 11.5px;
    line-height: 1.4;
  }
  .switch {
    position: relative;
    flex-shrink: 0;
    width: 30px;
    height: 18px;
    margin-top: 1px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: #474341;
    transition: background-color 180ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .switch span {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #f2f1f0;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.3);
    transition: transform 220ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .switch[aria-checked="true"] {
    background: #0a9bff;
  }
  .switch[aria-checked="true"] span {
    transform: translateX(12px);
  }
  .switch:focus-visible {
    outline: 2px solid rgb(10 155 255 / 0.6);
    outline-offset: 2px;
  }
  .switch:disabled {
    opacity: 0.5;
  }
  .row.disabled label {
    color: #a8a4a1;
  }
  @media (prefers-reduced-motion: reduce) {
    .switch,
    .switch span {
      transition: none;
    }
  }
</style>
