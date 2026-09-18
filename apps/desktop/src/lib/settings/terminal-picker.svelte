<script lang="ts" module>
  export type TerminalOption = { id: string; name: string; icon: string | null };
</script>

<script lang="ts">
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import CheckIcon from "@lucide/svelte/icons/check";
  import TerminalIcon from "@lucide/svelte/icons/terminal";

  let {
    label,
    description,
    options,
    value,
    onchange,
  }: {
    label: string;
    description?: string;
    options: TerminalOption[];
    value: string;
    onchange: (id: string) => void;
  } = $props();

  const id = $props.id();
  let open = $state(false);
  let root = $state<HTMLDivElement | null>(null);
  let trigger = $state<HTMLButtonElement | null>(null);
  let position = $state({ top: 0, left: 0, width: 0, up: false });

  const selected = $derived(options.find((option) => option.id === value) ?? options[0]);

  function place() {
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const width = rect.width;
    const up = window.innerHeight - rect.bottom < 240;
    position = { top: up ? rect.top - 4 : rect.bottom + 4, left: Math.max(8, rect.right - width), width, up };
  }

  function toggle() {
    if (open) {
      open = false;
      return;
    }
    place();
    open = true;
  }

  function choose(next: string) {
    onchange(next);
    open = false;
  }

  function onWindowPointerDown(event: PointerEvent) {
    if (open && root && !root.contains(event.target as Node)) open = false;
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (open && event.key === "Escape") open = false;
  }

  $effect(() => {
    if (!open) return;
    const close = () => (open = false);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
    };
  });
</script>

<svelte:window onpointerdown={onWindowPointerDown} onkeydown={onWindowKeydown} />

<div class="row" bind:this={root}>
  <div class="copy">
    <span id={`${id}-label`} class="label">{label}</span>
    {#if description}<p>{description}</p>{/if}
  </div>
  <div class="picker">
    <button
      bind:this={trigger}
      type="button"
      class="trigger"
      aria-haspopup="listbox"
      aria-expanded={open}
      aria-labelledby={`${id}-label`}
      onclick={toggle}>
      {#if selected?.icon}
        <img class="icon" src={selected.icon} alt="" />
      {:else}
        <TerminalIcon class="icon fallback" />
      {/if}
      <span class="name">{selected?.name ?? ""}</span>
      <ChevronDownIcon class="chevron" />
    </button>
    {#if open}
      <ul
        class="menu"
        role="listbox"
        aria-labelledby={`${id}-label`}
        style={`top:${position.top}px;left:${position.left}px;width:${position.width}px${position.up ? ";transform:translateY(-100%)" : ""}`}>
        {#each options as option (option.id)}
          <li>
            <button
              type="button"
              role="option"
              aria-selected={option.id === value}
              onclick={() => choose(option.id)}>
              {#if option.icon}
                <img class="icon" src={option.icon} alt="" />
              {:else}
                <TerminalIcon class="icon fallback" />
              {/if}
              <span class="name">{option.name}</span>
              {#if option.id === value}<CheckIcon class="check" />{/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .copy {
    min-width: 0;
  }
  .label {
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
  .picker {
    flex-shrink: 0;
  }
  .trigger {
    display: flex;
    width: 200px;
    align-items: center;
    gap: 8px;
    padding: 5px 10px;
    border: 1px solid rgb(255 255 255 / 8%);
    border-radius: 8px;
    background: #1f1d1b;
    color: #f2f1f0;
    font-size: 12px;
    text-align: left;
  }
  .trigger:focus-visible {
    outline: 2px solid rgb(10 155 255 / 0.6);
    outline-offset: 1px;
  }
  .icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    border-radius: 4px;
    object-fit: contain;
  }
  .picker :global(svg.icon) {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }
  .picker :global(.fallback) {
    color: #a8a4a1;
  }
  .name {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .trigger :global(.chevron) {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: #8f8b88;
  }
  .menu {
    position: fixed;
    z-index: 50;
    max-height: 260px;
    overflow-y: auto;
    margin: 0;
    padding: 4px;
    border-radius: 10px;
    background: #2b2927;
    box-shadow:
      0 0 0 1px rgb(255 255 255 / 8%),
      0 16px 32px -12px rgb(0 0 0 / 55%);
    list-style: none;
  }
  .menu button {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: #eceae8;
    font-size: 12px;
    text-align: left;
  }
  .menu button:hover,
  .menu button:focus-visible {
    background: rgb(255 255 255 / 6%);
    outline: none;
  }
  .menu button[aria-selected="true"] {
    background: rgb(10 155 255 / 16%);
  }
  .menu button :global(.check) {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: #0a9bff;
  }
</style>
