<script lang="ts">
  type Props = { source: string; zoom?: number };
  let { source, zoom = 1 }: Props = $props();

  const document = $derived(`${source}<style>html { zoom: ${zoom}; }</style>`);

  let frame: HTMLIFrameElement;
</script>

<iframe
  bind:this={frame}
  title="HTML preview"
  sandbox="allow-scripts allow-popups"
  srcdoc={document}
  class="h-full w-full border-0 bg-white"></iframe>
