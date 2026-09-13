<script lang="ts" generics="T">
	import { createRowVirtualizer } from "$lib/virtual/row-virtualizer.svelte.js";
	import type { Snippet } from "svelte";

	let {
		items = [],
		estimateSize = 36,
		overscan = 10,
		class: className = "",
		children,
	}: {
		items: T[];
		estimateSize?: number;
		overscan?: number;
		class?: string;
		children: Snippet<[T, { start: number; size: number; index: number }]>;
	} = $props();

	let scrollEl = $state<HTMLDivElement>();

	const rows = createRowVirtualizer({
		count: () => items.length,
		estimateSize: () => estimateSize,
		get overscan() {
			return overscan;
		},
		getScrollElement: () => scrollEl ?? null,
	});
</script>

<div bind:this={scrollEl} class="h-full min-h-0 overflow-y-auto {className}">
	<div style="height: {rows.totalSize}px; position: relative;">
		{#each rows.virtualItems as v (v.key)}
			<div
				style="position: absolute; top: 0; left: 0; right: 0; transform: translateY({v.start}px); height: {v.size}px;">
				{@render children(items[v.index], { start: v.start, size: v.size, index: v.index })}
			</div>
		{/each}
	</div>
</div>
