<script lang="ts">
	import Fuse, { type FuseResult } from "fuse.js";
	import * as Command from "$lib/components/ui/command/index.js";
	import * as Kbd from "$lib/components/ui/kbd/index.js";
	import { highlightMatches } from "$lib/utils/highlight-matches.js";
	import { commandPaletteState, closeCommandPalette } from "$lib/state/command-palette.svelte";
	import type { Location } from "$lib/tabs/tabs.js";

	import GaugeIcon from "@lucide/svelte/icons/gauge";
	import Clock3Icon from "@lucide/svelte/icons/clock-3";
	import UsersIcon from "@lucide/svelte/icons/users";
	import FolderIcon from "@lucide/svelte/icons/folder";
	import FolderInputIcon from "@lucide/svelte/icons/folder-input";
	import CloudIcon from "@lucide/svelte/icons/cloud";
	import HouseIcon from "@lucide/svelte/icons/house";
	import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
	import FileIcon from "@lucide/svelte/icons/file";

	type Recent = { name: string; path: string; kind: string };
	type PaletteItem = {
		id: string;
		name: string;
		subtitle?: string;
		location: Location;
		group: "path" | "quick" | "favorites" | "locations" | "recents";
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		icon: any;
	};

	let {
		favorites = [],
		locations = [],
		recents = [],
		onNavigate,
	}: {
		favorites?: Location[];
		locations?: Location[];
		recents?: Recent[];
		onNavigate?: (location: Location, opts: { newTab: boolean }) => void;
	} = $props();

	let query = $state("");
	let newTabHeld = $state(false);

	$effect(() => {
		if (!commandPaletteState.open) query = "";
	});

	function handleKeydown(event: KeyboardEvent) {
		if (event.key.toLowerCase() !== "p" || !event.shiftKey || !(event.metaKey || event.ctrlKey))
			return;
		event.preventDefault();
		commandPaletteState.open = !commandPaletteState.open;
	}

	const quickItems = $derived<PaletteItem[]>([
		{
			id: "quick-overview",
			name: "Overview",
			location: { name: "Overview", path: "", kind: "overview" },
			group: "quick",
			icon: GaugeIcon,
		},
		{
			id: "quick-recents",
			name: "Recents",
			location: { name: "Recents", path: "", kind: "recents" },
			group: "quick",
			icon: Clock3Icon,
		},
		{
			id: "quick-shared",
			name: "Shared",
			location: { name: "Shared", path: "", kind: "shared" },
			group: "quick",
			icon: UsersIcon,
		},
	]);

	const favoriteItems = $derived<PaletteItem[]>(
		favorites.map((favorite) => ({
			id: `favorite-${favorite.path}`,
			name: favorite.name,
			subtitle: favorite.path,
			location: favorite,
			group: "favorites",
			icon: FolderIcon,
		})),
	);

	const locationItems = $derived<PaletteItem[]>(
		locations.map((location) => ({
			id: `location-${location.path}`,
			name: location.name,
			subtitle: location.path,
			location,
			group: "locations",
			icon:
				location.kind === "s3"
					? CloudIcon
					: location.kind === "home"
						? HouseIcon
						: HardDriveIcon,
		})),
	);

	const recentItems = $derived<PaletteItem[]>(
		recents.map((recent) => ({
			id: `recent-${recent.path}`,
			name: recent.name,
			subtitle: recent.path,
			location: {
				name: recent.name,
				path: recent.path,
				kind: recent.kind === "file" ? "file" : "folder",
			},
			group: "recents",
			icon: recent.kind === "file" ? FileIcon : FolderIcon,
		})),
	);

	const allItems = $derived([
		...quickItems,
		...favoriteItems,
		...locationItems,
		...recentItems,
	]);

	const fuse = $derived(
		new Fuse(allItems, {
			includeMatches: true,
			threshold: 0.4,
			ignoreLocation: true,
			keys: [
				{ name: "name", weight: 2 },
				{ name: "subtitle", weight: 1 },
			],
		}),
	);

	const trimmed = $derived(query.trim());
	const isPathQuery = $derived(
		trimmed.length > 0 && (trimmed.startsWith("/") || trimmed.startsWith("~/") || trimmed.includes("/")),
	);

	const pathItem = $derived<PaletteItem | null>(
		isPathQuery
			? {
					id: "path-jump",
					name: trimmed,
					location: { name: trimmed, path: trimmed, kind: "folder" },
					group: "path",
					icon: FolderInputIcon,
				}
			: null,
	);

	const results = $derived<FuseResult<PaletteItem>[]>(
		trimmed ? fuse.search(trimmed) : [],
	);

	const pathResults = $derived(pathItem ? [pathItem] : []);
	const quickResults = $derived(
		trimmed ? results.filter((r) => r.item.group === "quick").map((r) => r.item) : quickItems,
	);
	const favoriteResults = $derived(
		trimmed ? results.filter((r) => r.item.group === "favorites").map((r) => r.item) : favoriteItems,
	);
	const locationResults = $derived(
		trimmed ? results.filter((r) => r.item.group === "locations").map((r) => r.item) : locationItems,
	);
	const recentResults = $derived(
		trimmed ? results.filter((r) => r.item.group === "recents").map((r) => r.item) : recentItems,
	);

	function nameIndices(result: FuseResult<PaletteItem>) {
		return result.matches?.find((match) => match.key === "name")?.indices;
	}

	function select(item: PaletteItem) {
		onNavigate?.(item.location, { newTab: newTabHeld });
		closeCommandPalette();
	}

	function matchIndicesFor(item: PaletteItem): readonly (readonly [number, number])[] | undefined {
		if (!trimmed) return undefined;
		const result = results.find((r) => r.item.id === item.id);
		return result ? nameIndices(result) : undefined;
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<Command.Dialog
	bind:open={commandPaletteState.open}
	shouldFilter={false}
	class="command-palette top-[16vh]! w-[min(42rem,calc(100%-2rem))] max-w-none rounded-[20px]! p-0 shadow-2xl ring-0! bg-transparent!"
	title="Go to Folder"
	description="Search favorites, locations and paths">
		<div class="command-palette-header">
			<Command.Input
				bind:value={query}
				autofocus
				placeholder="Go to a folder or location…"
				onkeydown={(e: KeyboardEvent) => (newTabHeld = e.metaKey || e.ctrlKey)}
				onkeyup={(e: KeyboardEvent) => (newTabHeld = e.metaKey || e.ctrlKey)} />
		</div>
	<Command.List>
		<Command.Empty>No results found.</Command.Empty>

		{#if pathResults.length}
			<Command.Group heading="Go to Folder">
				{#each pathResults as item (item.id)}
					<Command.Item value={item.id} onSelect={() => select(item)}>
						<item.icon class="size-4 text-muted-foreground" />
						<span class="min-w-0 flex-1 truncate">{item.name}</span>
					</Command.Item>
				{/each}
			</Command.Group>
		{/if}

		{#if quickResults.length}
			<Command.Group heading="Quick Access">
				{#each quickResults as item (item.id)}
					<Command.Item value={item.id} onSelect={() => select(item)}>
						<item.icon class="size-4 text-muted-foreground" />
						<span class="min-w-0 flex-1 truncate">{item.name}</span>
					</Command.Item>
				{/each}
			</Command.Group>
		{/if}

		{#if favoriteResults.length}
			<Command.Group heading="Favorites">
				{#each favoriteResults as item (item.id)}
					<Command.Item value={item.id} onSelect={() => select(item)}>
						<item.icon class="size-4 text-muted-foreground" />
						<span class="min-w-0 flex-1 truncate">
							{#each highlightMatches(item.name, matchIndicesFor(item)) as segment}
								{#if segment.highlighted}<mark class="bg-primary/20 text-foreground rounded-sm">{segment.text}</mark>{:else}{segment.text}{/if}
							{/each}
						</span>
						{#if item.subtitle}
							<span class="ml-auto truncate text-xs text-muted-foreground">{item.subtitle}</span>
						{/if}
					</Command.Item>
				{/each}
			</Command.Group>
		{/if}

		{#if locationResults.length}
			<Command.Group heading="Locations">
				{#each locationResults as item (item.id)}
					<Command.Item value={item.id} onSelect={() => select(item)}>
						<item.icon class="size-4 text-muted-foreground" />
						<span class="min-w-0 flex-1 truncate">
							{#each highlightMatches(item.name, matchIndicesFor(item)) as segment}
								{#if segment.highlighted}<mark class="bg-primary/20 text-foreground rounded-sm">{segment.text}</mark>{:else}{segment.text}{/if}
							{/each}
						</span>
						{#if item.subtitle}
							<span class="ml-auto truncate text-xs text-muted-foreground">{item.subtitle}</span>
						{/if}
					</Command.Item>
				{/each}
			</Command.Group>
		{/if}

		{#if recentResults.length}
			<Command.Group heading="Recents">
				{#each recentResults as item (item.id)}
					<Command.Item value={item.id} onSelect={() => select(item)}>
						<item.icon class="size-4 text-muted-foreground" />
						<span class="min-w-0 flex-1 truncate">
							{#each highlightMatches(item.name, matchIndicesFor(item)) as segment}
								{#if segment.highlighted}<mark class="bg-primary/20 text-foreground rounded-sm">{segment.text}</mark>{:else}{segment.text}{/if}
							{/each}
						</span>
						{#if item.subtitle}
							<span class="ml-auto truncate text-xs text-muted-foreground">{item.subtitle}</span>
						{/if}
					</Command.Item>
				{/each}
			</Command.Group>
		{/if}
	</Command.List>
	<footer class="command-palette-footer">
		<span><Kbd.Kbd>↑</Kbd.Kbd><Kbd.Kbd>↓</Kbd.Kbd> navigate</span>
		<span><Kbd.Kbd>↵</Kbd.Kbd> open</span>
		<span><Kbd.Kbd>⌘</Kbd.Kbd><Kbd.Kbd>↵</Kbd.Kbd> new tab</span>
		<span><Kbd.Kbd>Esc</Kbd.Kbd> close</span>
	</footer>
</Command.Dialog>

<style>
	:global(.command-palette [data-slot='command']) {
		border: 1px solid #4a4642;
		border-radius: 20px;
		background: rgb(43 41 39 / 94%);
		color: #e8e5e2;
		backdrop-filter: blur(20px) saturate(125%);
		padding: 0;
	}

	:global(.command-palette [data-slot='command-input-wrapper']) {
		border-bottom: 1px solid #3a3734;
		padding: 0.75rem;
	}

	:global(.command-palette .command-palette-header) {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	:global(.command-palette .command-palette-header [data-slot='command-input-wrapper']) {
		flex: 1 1 auto;
		min-width: 0;
	}

	:global(.command-palette [data-slot='input-group']) {
		height: 2.75rem !important;
		border-radius: 0.75rem;
		background: #242220;
	}

	:global(.command-palette [data-slot='command-list']) {
		max-height: min(24rem, 55vh);
		padding: 0.35rem;
	}

	:global(.command-palette [data-slot='command-group']) {
		padding: 0.25rem;
	}

	:global(.command-palette [data-command-group-heading]) {
		padding: 0.6rem 0.65rem 0.35rem;
		color: var(--muted-foreground);
		font-size: 0.6875rem;
		font-weight: 700;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}

	:global(.command-palette [data-slot='command-item']) {
		min-height: 1.9rem;
		border-radius: 0.625rem;
		padding: 0.35rem 0.6rem;
	}

	:global(.command-palette [data-slot='command-item'][aria-selected='true']),
	:global(.command-palette [data-slot='command-item'][data-selected='true']) {
		background: #0a9bff;
		color: #fff;
	}

	:global(.command-palette [data-slot='command-item'][aria-selected='true'] svg),
	:global(.command-palette [data-slot='command-item'][data-selected='true'] svg) {
		color: #fff;
	}

	:global(.command-palette [data-slot='command-item'][aria-selected='true'] mark),
	:global(.command-palette [data-slot='command-item'][data-selected='true'] mark) {
		background: oklch(1 0 0 / 25%);
		color: inherit;
	}

	:global(.command-palette .command-palette-footer) {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 1rem;
		border-top: 1px solid #3a3734;
		padding: 0.65rem 0.9rem;
		color: var(--muted-foreground);
		font-size: 0.75rem;
	}

	:global(.command-palette .command-palette-footer span) {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
	}
</style>
