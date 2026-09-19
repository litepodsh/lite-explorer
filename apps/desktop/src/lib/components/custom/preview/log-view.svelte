<script lang="ts">
  type Props = { content: string };
  let { content }: Props = $props();

  type Segment = { text: string; color: string; bold: boolean };

  const COLORS: Record<number, string> = {
    30: "#3b4048",
    31: "#f7768e",
    32: "#a3c9a8",
    33: "#e0af68",
    34: "#7aa2f7",
    35: "#bb9af7",
    36: "#7dcfff",
    37: "#c0caf5",
    90: "#67635f",
    91: "#ff9e9e",
    92: "#b9f6c1",
    93: "#f6d089",
    94: "#9fb8e8",
    95: "#d0b0ff",
    96: "#a4e4ff",
    97: "#ffffff",
  };

  function styleFor(segment: Segment): string {
    return `${segment.color ? `color:${segment.color};` : ""}${segment.bold ? "font-weight:600;" : ""}`;
  }

  function parseLine(line: string): Segment[] {
    const segments: Segment[] = [];
    let color = "";
    let bold = false;
    let index = 0;
    // eslint-disable-next-line no-control-regex
    const pattern = /\x1b\[([0-9;]*)m/g;
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(line))) {
      if (match.index > index) {
        segments.push({ text: line.slice(index, match.index), color, bold });
      }
      for (const code of match[1].split(";")) {
        const value = Number(code || "0");
        if (value === 0) {
          color = "";
          bold = false;
        } else if (value === 1) {
          bold = true;
        } else if (value === 22) {
          bold = false;
        } else if (COLORS[value]) {
          color = COLORS[value];
        }
      }
      index = match.index + match[0].length;
    }
    if (index < line.length) segments.push({ text: line.slice(index), color, bold });
    return segments;
  }

  const lines = $derived(content.split("\n").map(parseLine));
</script>

<div class="h-full min-h-0 overflow-auto bg-[#1f1d1b]" aria-label="Log preview">
  <pre class="min-w-full py-2 font-mono text-[12.5px] leading-[1.55]">{#each lines as segments, index (index)}<span
      class="block px-3">{#each segments as segment}<span style={styleFor(segment)}>{segment.text}</span>{/each}{#if segments.length === 0}{" "}{/if}</span
    >{/each}</pre>
</div>
