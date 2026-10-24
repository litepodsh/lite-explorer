<script lang="ts">
  type Props = { content: string };
  let { content }: Props = $props();

  type Line = { text: string; className: string };

  const lines = $derived.by<Line[]>(() =>
    content.split("\n").map((text) => {
      if (text.startsWith("@@")) return { text, className: "text-[var(--app-info)] bg-[var(--app-info)]/10" };
      if (text.startsWith("+++") || text.startsWith("---"))
        return { text, className: "text-[var(--app-fg)] font-semibold" };
      if (text.startsWith("+")) return { text, className: "text-[var(--app-success)] bg-[var(--app-success)]/10" };
      if (text.startsWith("-")) return { text, className: "text-[var(--app-danger-fg)] bg-[var(--app-danger)]/10" };
      if (text.startsWith("diff ") || text.startsWith("index "))
        return { text, className: "text-[var(--app-fg-muted)]" };
      return { text, className: "text-[var(--app-fg-muted)]" };
    }),
  );
</script>

<div class="h-full min-h-0 overflow-auto bg-[var(--app-input)]" aria-label="Diff preview">
  <pre class="min-w-full py-2 font-mono text-[13px] leading-[1.55]">{#each lines as line, index (index)}<span
      class="block px-3 {line.className}">{line.text || " "}</span>{/each}</pre>
</div>
