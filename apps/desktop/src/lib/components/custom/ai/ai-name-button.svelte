<script lang="ts">
  import AppleIntelligenceMark from "./apple-intelligence-mark.svelte";

  let {
    thinking = false,
    disabled = false,
    label = "Suggest a name with Apple Intelligence",
    // Grid tiles are themselves buttons, so the trigger renders as a span there to keep the
    // markup valid while staying keyboard-operable.
    as = "button",
    onSuggest,
  } = $props<{
    thinking?: boolean;
    disabled?: boolean;
    label?: string;
    as?: "button" | "span";
    onSuggest?: () => void;
  }>();

  function activate() {
    if (!disabled) onSuggest?.();
  }
</script>

<svelte:element
  this={as}
  role={as === "span" ? "button" : undefined}
  tabindex={as === "span" ? 0 : undefined}
  class="inline-flex size-5 flex-none items-center justify-center rounded-full border-0 bg-transparent p-0 text-inherit transition duration-150 motion-reduce:transition-none {thinking
    ? 'animate-[pulse_1.15s_ease-in-out_infinite] opacity-100 motion-reduce:animate-none'
    : 'opacity-55 hover:scale-110 hover:opacity-100 focus-visible:scale-110 focus-visible:opacity-100 focus-visible:outline-none'}"
  title={thinking ? "Asking Apple Intelligence…" : label}
  aria-label={thinking ? "Asking Apple Intelligence…" : label}
  {...(as === "button" ? { disabled } : {})}
  onclick={(event) => {
    // The field lives inside a row that opens files on click and selects on pointer-down.
    event.stopPropagation();
    event.preventDefault();
    activate();
  }}
  onmousedown={(event) => {
    // Keeps focus in the rename field: a blur would commit the old name and close the field.
    event.stopPropagation();
    event.preventDefault();
  }}
  onpointerdown={(event) => event.stopPropagation()}
  onkeydown={(event) => {
    event.stopPropagation();
    if (as === "span" && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault();
      activate();
      return;
    }
    if (event.key === "Escape") event.stopPropagation();
  }}>
  <AppleIntelligenceMark size={15} />
</svelte:element>
