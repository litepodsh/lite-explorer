/** `query` is taken by the palette when it opens, then reset to null. */
export const commandPaletteState = $state<{ open: boolean; query: string | null }>({
  open: false,
  query: null,
});

export function openCommandPalette() {
  commandPaletteState.open = true;
}

export function openCommandPaletteWith(query: string) {
  commandPaletteState.query = query;
  commandPaletteState.open = true;
}

export function closeCommandPalette() {
  commandPaletteState.open = false;
}
