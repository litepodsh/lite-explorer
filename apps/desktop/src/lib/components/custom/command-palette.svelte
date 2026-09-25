<script lang="ts" module>
	export type PaletteCommand = { id: string; title: string; keywords?: string[]; shortcut: string; run: () => void };
</script>

<script lang="ts">
	import "./command-surface.css";
	import { tick, untrack } from "svelte";
	import Fuse, { type FuseResult } from "fuse.js";
	import { invoke } from "@tauri-apps/api/core";
	import * as Command from "$lib/components/ui/command/index.js";
	import * as Kbd from "$lib/components/ui/kbd/index.js";
	import { highlightMatches } from "$lib/utils/highlight-matches.js";
	import { commandPaletteState, closeCommandPalette } from "$lib/state/command-palette.svelte";
	import { openTarget } from "$lib/file-ops/open.js";
	import type { DirectoryEntry } from "$lib/components/custom/file-list/list-item.svelte";
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
	import TerminalIcon from "@lucide/svelte/icons/square-terminal";
	import { openShortcuts } from "$lib/state/shortcuts-dialog.svelte.js";

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
		commands = [],
		onNavigate,
	}: {
		favorites?: Location[];
		locations?: Location[];
		recents?: Recent[];
		commands?: PaletteCommand[];
		onNavigate?: (location: Location, opts: { newTab: boolean }) => void;
	} = $props();

	let query = $state("");
	let input: HTMLInputElement | null = $state(null);
	let newTabHeld = $state(false);
	let selectedValue = $state("");
	let pathEntries = $state<DirectoryEntry[]>([]);

	type TabCycle = {
		generated: string;
		prefix: string;
		names: string[];
		dirs: boolean[];
		index: number;
	};
	let tabCycle: TabCycle | null = null;

	$effect(() => {
		if (!commandPaletteState.open) {
			query = "";
			selectedValue = "";
			pathEntries = [];
			tabCycle = null;
		}
	});

	// A query passed on open (Go to Path) fills the input once.
	$effect(() => {
		if (!commandPaletteState.open) return;
		const initial = commandPaletteState.query;
		if (initial === null) return;
		untrack(() => {
			query = initial;
			commandPaletteState.query = null;
			void tick().then(() => input?.select());
		});
	});

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

	// `?` switches to the shortcuts dialog with the rest of the text as its search.
	$effect(() => {
		if (!query.startsWith("?")) return;
		const rest = query.slice(1);
		untrack(() => {
			closeCommandPalette();
			openShortcuts(rest);
		});
	});

	const commandFuse = $derived(new Fuse(commands, { threshold: 0.4, ignoreLocation: true, keys: ["title", "keywords"] }));
	const commandResults = $derived(trimmed ? commandFuse.search(trimmed).map((result) => result.item) : commands);

	function runCommand(command: PaletteCommand) {
		closeCommandPalette();
		command.run();
	}
	const isPathQuery = $derived(
		trimmed.length > 0 &&
			(trimmed.startsWith("/") ||
				trimmed.startsWith("~") ||
				trimmed.includes("/") ||
				trimmed.includes("\\")),
	);

	const pathParts = $derived.by(() => {
		if (!isPathQuery) return { dirPart: "", fragment: "" };
		const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
		if (cut < 0) return { dirPart: trimmed, fragment: "" };
		return {
			dirPart: trimmed.slice(0, cut) || "/",
			fragment: trimmed.slice(cut + 1),
		};
	});

	let pathToken = 0;
	let pathTimer: ReturnType<typeof setTimeout> | undefined;

	$effect(() => {
		const { dirPart } = pathParts;
		if (!isPathQuery || !dirPart) {
			pathEntries = [];
			return;
		}
		const token = ++pathToken;
		clearTimeout(pathTimer);
		pathTimer = setTimeout(async () => {
			try {
				const entries = await invoke<DirectoryEntry[]>("read_directory", { path: dirPart });
				if (token === pathToken) pathEntries = entries;
			} catch {
				if (token === pathToken) pathEntries = [];
			}
		}, 120);
		return () => clearTimeout(pathTimer);
	});

	const pathSuggestions = $derived.by(() => {
		if (!isPathQuery) return [] as DirectoryEntry[];
		const fragment = pathParts.fragment.toLowerCase();
		const matches = fragment
			? pathEntries.filter((entry) => entry.name.toLowerCase().startsWith(fragment))
			: pathEntries.slice();
		matches.sort(
			(a, b) =>
				Number(b.is_directory) - Number(a.is_directory) ||
				a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
		);
		return matches.slice(0, 50);
	});

	const exactEntry = $derived(
		isPathQuery && pathParts.fragment
			? pathEntries.find((entry) => entry.name === pathParts.fragment)
			: undefined,
	);

	const pathJumpItem = $derived<PaletteItem | null>(
		isPathQuery
			? {
					id: "path-jump",
					name: trimmed,
					location: {
						name: trimmed,
						path: trimmed,
						kind: exactEntry && !exactEntry.is_directory ? "file" : "folder",
					},
					group: "path",
					icon: exactEntry && !exactEntry.is_directory ? FileIcon : FolderInputIcon,
				}
			: null,
	);

	const pathEntryItems = $derived<PaletteItem[]>(
		pathSuggestions.map((entry) => ({
			id: `path-entry-${entry.path}`,
			name: entry.is_directory ? `${entry.name}/` : entry.name,
			subtitle: entry.path,
			location: {
				name: entry.name,
				path: entry.path,
				kind: entry.is_directory ? "folder" : "file",
			},
			group: "path",
			icon: entry.is_directory ? FolderIcon : FileIcon,
		})),
	);

	const results = $derived<FuseResult<PaletteItem>[]>(
		trimmed ? fuse.search(trimmed) : [],
	);

	const pathResults = $derived([
		...(pathJumpItem ? [pathJumpItem] : []),
		...pathEntryItems,
	]);
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

	async function select(item: PaletteItem) {
		const newTab = newTabHeld;
		let path = item.location.path;
		const isFile = item.location.kind === "file";
		if (path.startsWith("~")) {
			try {
				path = await invoke<string>("resolve_path", { path });
			} catch {
				// keep the literal path; the pane will surface the error
			}
		}
		if (isFile) {
			void openTarget(path).catch(() => {});
			closeCommandPalette();
			return;
		}
		onNavigate?.({ ...item.location, path }, { newTab });
		closeCommandPalette();
	}

	function cycleQuery(cycle: TabCycle, index: number): string {
		return `${cycle.prefix}${cycle.names[index]}${cycle.dirs[index] ? "/" : ""}`;
	}

	/** Tab completes the highlighted folder, or cycles through the matching paths. */
	function completeSelected(reverse: boolean) {
		const selected = pathResults.find((candidate) => candidate.id === selectedValue);
		const selectedEntry =
			selected?.id.startsWith("path-entry-") && selected.location.kind === "folder"
				? selected.location.name
				: null;

		if (tabCycle && tabCycle.generated === query) {
			const length = tabCycle.names.length;
			const index = (tabCycle.index + (reverse ? -1 : 1) + length) % length;
			tabCycle = { ...tabCycle, index };
			query = cycleQuery(tabCycle, index);
			return;
		}

		if (!pathSuggestions.length) return;
		const base = pathParts.dirPart;
		const separator = base.endsWith("/") || base.endsWith("\\") ? "" : "/";
		const names = pathSuggestions.map((entry) => entry.name);
		const dirs = pathSuggestions.map((entry) => entry.is_directory);
		let index = selectedEntry ? Math.max(0, names.indexOf(selectedEntry)) : 0;
		if (!selectedEntry && reverse) index = names.length - 1;
		const cycle: TabCycle = { generated: "", prefix: `${base}${separator}`, names, dirs, index };
		cycle.generated = cycleQuery(cycle, index);
		tabCycle = cycle;
		query = cycle.generated;
	}

	function onInputKeydown(event: KeyboardEvent) {
		newTabHeld = event.metaKey || event.ctrlKey;
		if (event.key === "Tab") {
			event.preventDefault();
			completeSelected(event.shiftKey);
		}
	}

	function matchIndicesFor(item: PaletteItem): readonly (readonly [number, number])[] | undefined {
		if (!trimmed) return undefined;
		const result = results.find((r) => r.item.id === item.id);
		return result ? nameIndices(result) : undefined;
	}
</script>

<Command.Dialog
	bind:open={commandPaletteState.open}
	bind:value={selectedValue}
	shouldFilter={false}
	class="command-surface top-[16vh]! w-[min(39.2rem,calc(100%-2rem))]! max-w-none! rounded-[14px]! p-0 shadow-2xl ring-0! bg-transparent!"
	title="Command Palette"
	description="Search commands, favorites, locations and paths">
	<Command.Input
		bind:ref={input}
		bind:value={query}
		autofocus
		placeholder="Go to a folder, location or command…"
		onkeydown={onInputKeydown}
		onkeyup={(e: KeyboardEvent) => (newTabHeld = e.metaKey || e.ctrlKey)} />
	<Command.List>
		<Command.Empty>No results found.</Command.Empty>

		{#if pathResults.length}
			<Command.Group heading="Go to Folder">
				{#each pathResults as item (item.id)}
					<Command.Item value={item.id} onSelect={() => select(item)}>
						<item.icon class="size-4 text-muted-foreground" />
						<span class="min-w-0 flex-1 truncate">{item.name}</span>
						{#if item.subtitle}
							<span class="ml-auto truncate text-xs text-muted-foreground">{item.subtitle}</span>
						{/if}
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

		{#if commandResults.length && !isPathQuery}
			<Command.Group heading="Commands">
				{#each commandResults as command (command.id)}
					<Command.Item value={command.id} onSelect={() => runCommand(command)}>
						<TerminalIcon class="size-4 text-muted-foreground" />
						<span class="min-w-0 flex-1 truncate">{command.title}</span>
						{#if command.shortcut}
							<span class="ml-auto text-xs text-muted-foreground">{command.shortcut}</span>
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
	<footer class="command-surface-footer">
		<span><Kbd.Kbd>↑</Kbd.Kbd><Kbd.Kbd>↓</Kbd.Kbd><span class="command-surface-footer-label">navigate</span></span>
		<span><Kbd.Kbd>Tab</Kbd.Kbd><span class="command-surface-footer-label">complete / cycle</span></span>
		<span><Kbd.Kbd>↵</Kbd.Kbd><span class="command-surface-footer-label">open</span></span>
		<span><Kbd.Kbd>⌘</Kbd.Kbd><Kbd.Kbd>↵</Kbd.Kbd><span class="command-surface-footer-label">new tab</span></span>
		<span><Kbd.Kbd>?</Kbd.Kbd><span class="command-surface-footer-label">shortcuts</span></span>
		<span><Kbd.Kbd>Esc</Kbd.Kbd><span class="command-surface-footer-label">close</span></span>
	</footer>
</Command.Dialog>
