<script lang="ts">
  // Centered alert-style dialog, matching the add-location dialog.
  // Content goes in `children`, buttons in `footer`.
  import { AlertDialog } from "bits-ui";
  import type { Snippet } from "svelte";

  let {
    open = $bindable(false),
    title,
    description,
    width = 420,
    initialFocus = null,
    children,
    footer,
  }: {
    open?: boolean;
    title: string;
    description?: string;
    width?: number;
    /** Element focused when the dialog opens, instead of the first focusable one. */
    initialFocus?: HTMLElement | null;
    children?: Snippet;
    footer: Snippet;
  } = $props();
</script>

<AlertDialog.Root bind:open>
  <AlertDialog.Portal>
    <AlertDialog.Overlay class="app-dialog-overlay" />
    <AlertDialog.Content
      class="app-dialog"
      style={`--app-dialog-width: ${width}px`}
      onOpenAutoFocus={(event) => {
        if (!initialFocus) return;
        event.preventDefault();
        initialFocus.focus();
      }}>
      <div class="shell">
        <div class="core">
          <AlertDialog.Title class="app-dialog-title">{title}</AlertDialog.Title>
          {#if description}
            <AlertDialog.Description class="app-dialog-description">{description}</AlertDialog.Description>
          {/if}
          {@render children?.()}
        </div>
        <footer>{@render footer()}</footer>
      </div>
    </AlertDialog.Content>
  </AlertDialog.Portal>
</AlertDialog.Root>

<style>
  :global(.app-dialog-overlay) {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: var(--app-overlay);
    animation: app-dialog-fade 220ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  :global(.app-dialog) {
    --app-dialog-ease: cubic-bezier(0.32, 0.72, 0, 1);
    position: fixed;
    top: 50%;
    left: 50%;
    z-index: 61;
    width: min(var(--app-dialog-width), calc(100vw - 32px));
    translate: -50% -50%;
    color: var(--app-fg);
    font-size: 13px;
    outline: none;
    animation: app-dialog-in 260ms var(--app-dialog-ease);
  }
  .shell {
    padding: 5px;
    border-radius: var(--app-radius);
    /* Opaque: the listing must not show through the footer. */
    background: var(--app-input);
    box-shadow:
      0 0 0 1px rgb(255 255 255 / 0.08),
      var(--app-shadow);
  }
  .core {
    display: grid;
    gap: 8px;
    padding: 18px 18px 16px;
    border-radius: calc(var(--app-radius) - 4px);
    background: var(--app-surface);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.06);
  }
  :global(.app-dialog-title) {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  :global(.app-dialog-description) {
    margin: 0;
    color: var(--app-fg-muted);
    font-size: 12.5px;
    line-height: 1.45;
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 9px 7px 4px 12px;
  }
  @keyframes app-dialog-in {
    from {
      opacity: 0;
      transform: scale(0.96);
    }
  }
  @keyframes app-dialog-fade {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.app-dialog),
    :global(.app-dialog-overlay) {
      animation: none;
    }
  }
</style>
