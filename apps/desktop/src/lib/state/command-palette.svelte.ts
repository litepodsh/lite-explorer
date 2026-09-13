export const commandPaletteState = $state({ open: false });

export function openCommandPalette() {
  commandPaletteState.open = true;
}

export function closeCommandPalette() {
  commandPaletteState.open = false;
}
