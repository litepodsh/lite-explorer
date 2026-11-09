import type { ClipboardMode } from "../transfer-clipboard/queue.js";

export type PaneTransfer = {
  mode: ClipboardMode;
  paths: string[];
  destination: string;
  /** The other pane shows a real folder, not Overview, Recents or a list of buckets or shares. */
  destinationIsFolder: boolean;
  sourceRemote: boolean;
  targetRemote: boolean;
};

function trimmed(path: string): string {
  return path.replace(/[\\/]+$/, "");
}

function parentOf(path: string): string {
  const clean = trimmed(path);
  return clean.slice(0, Math.max(clean.lastIndexOf("/"), clean.lastIndexOf("\\")));
}

function isSameOrInside(path: string, folder: string): boolean {
  const child = trimmed(path);
  const parent = trimmed(folder);
  return child === parent || child.startsWith(`${parent}/`) || child.startsWith(`${parent}\\`);
}

/** Why the selection can't go to the other pane, as tooltip text. Null when it can. */
export function paneTransferBlocked(transfer: PaneTransfer): string | null {
  const { mode, paths, destination } = transfer;
  if (!transfer.destinationIsFolder) return "Open a folder in the other pane first";
  if (mode === "move" && (transfer.sourceRemote || transfer.targetRemote)) {
    return "Move isn’t available for remote locations";
  }
  if (paths.some((path) => isSameOrInside(destination, path))) {
    return "A folder can’t go inside itself";
  }
  if (paths.some((path) => parentOf(path) === trimmed(destination))) {
    return "The other pane shows this same folder";
  }
  return null;
}
