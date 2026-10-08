<script lang="ts">
  import { formatToken, type KeyPlatform } from "./keys.js";
  import type { ChordState } from "./resolve.js";

  let { pending, enabled, platform }: { pending: ChordState | null; enabled: boolean; platform: KeyPlatform } = $props();

  /** Fast typists finish chords before the popup would flash. */
  const DELAY_MS = 300;
  let shown = $state(false);

  $effect(() => {
    if (!pending || !enabled) {
      shown = false;
      return;
    }
    const timer = setTimeout(() => (shown = true), DELAY_MS);
    return () => clearTimeout(timer);
  });

  const prefix = $derived(pending ? pending.typed.map((token) => formatToken(token, platform)).join(" ") : "");
  const rows = $derived.by(() => {
    const chord = pending;
    if (!chord) return [];
    return chord.candidates
      .map((binding) => ({ key: formatToken(binding.tokens[chord.typed.length], platform), desc: binding.desc }))
      .sort((a, b) => a.key.localeCompare(b.key));
  });
</script>

{#if shown && pending}
  <div class="which-key" role="status" aria-live="polite">
    <div class="which-key-prefix"><kbd>{prefix}</kbd><span>…</span></div>
    <ul>
      {#each rows as row, index (index)}
        <li><kbd>{row.key}</kbd><span>{row.desc}</span></li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  .which-key {
    position: fixed;
    bottom: 40px;
    left: 50%;
    z-index: 60;
    width: min(34rem, calc(100% - 2rem));
    padding: 10px 12px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--app-surface) 94%, transparent);
    box-shadow:
      0 0 0 1px var(--app-border-strong),
      0 18px 40px -18px rgb(0 0 0 / 55%);
    color: var(--app-fg);
    transform: translateX(-50%);
    backdrop-filter: blur(20px) saturate(125%);
    animation: which-key-in 160ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .which-key-prefix {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    color: var(--app-fg-muted);
    font-size: 11px;
  }
  ul {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(10rem, 1fr));
    gap: 4px 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  li span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  kbd {
    min-width: 20px;
    padding: 1px 6px;
    border-radius: 6px;
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
    color: #ffc76b;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11px;
    text-align: center;
  }
  @keyframes which-key-in {
    from {
      opacity: 0;
      transform: translate(-50%, 8px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .which-key {
      animation: which-key-fade 160ms linear;
    }
    @keyframes which-key-fade {
      from {
        opacity: 0;
      }
    }
  }
</style>
