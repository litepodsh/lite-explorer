export const shortcutsDialogState = $state({ open: false, query: "" });

export function openShortcuts(query = "") {
  shortcutsDialogState.query = query;
  shortcutsDialogState.open = true;
}
