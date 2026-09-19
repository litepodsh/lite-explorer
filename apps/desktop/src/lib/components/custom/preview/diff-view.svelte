<script lang="ts">
  type Props = { content: string };
  let { content }: Props = $props();

  type Line = { text: string; className: string };

  const lines = $derived.by<Line[]>(() =>
    content.split("\n").map((text) => {
      if (text.startsWith("@@")) return { text, className: "text-[#7aa2f7] bg-[#7aa2f7]/10" };
      if (text.startsWith("+++") || text.startsWith("---"))
        return { text, className: "text-[#e8e5e2] font-semibold" };
      if (text.startsWith("+")) return { text, className: "text-[#a3c9a8] bg-[#a3c9a8]/10" };
      if (text.startsWith("-")) return { text, className: "text-[#f7768e] bg-[#f7768e]/10" };
      if (text.startsWith("diff ") || text.startsWith("index "))
        return { text, className: "text-[#9c9895]" };
      return { text, className: "text-[#c0bbb5]" };
    }),
  );
</script>

<div class="h-full min-h-0 overflow-auto bg-[#1f1d1b]" aria-label="Diff preview">
  <pre class="min-w-full py-2 font-mono text-[12.5px] leading-[1.55]">{#each lines as line, index (index)}<span
      class="block px-3 {line.className}">{line.text || " "}</span>{/each}</pre>
</div>
