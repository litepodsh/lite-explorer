import { getVersion } from "@tauri-apps/api/app";
import { noteLines, releaseFor } from "./changelog.js";

const SEEN_KEY = "whats-new.seen-version";

function readSeen(): string | null {
  try {
    return localStorage.getItem(SEEN_KEY);
  } catch {
    return null;
  }
}

function writeSeen(version: string) {
  try {
    localStorage.setItem(SEEN_KEY, version);
  } catch {
    // Private windows can refuse writes; the notes just show again next launch.
  }
}

/**
 * Release notes dialog. Opens from the Help menu and by itself once after an update, when the
 * running version differs from the last one the user saw.
 */
class WhatsNew {
  open = $state(false);
  /** Version to scroll to and title the dialog with; null shows every release. */
  version = $state<string | null>(null);
  /** Notes of a version that is downloaded but not running yet, from the updater. */
  notes = $state<string | null>(null);

  show(version: string | null = null, notes: string | null = null) {
    this.version = version;
    this.notes = notes;
    this.open = true;
  }

  close() {
    this.open = false;
  }

  markSeen() {
    if (this.version) writeSeen(this.version);
  }

  /** Notes of the update waiting for a restart, shown in the update banner. */
  showForUpdate(version: string, notes: string | null) {
    this.show(version, noteLines(notes).length > 0 ? notes : null);
  }

  async checkOnLaunch() {
    let version: string;
    try {
      version = await getVersion();
    } catch {
      return;
    }
    const seen = readSeen();
    // A version nobody has seen yet is a fresh install, not an update.
    if (!seen) {
      writeSeen(version);
      return;
    }
    if (seen === version) return;
    // Nothing to show when this release shipped without a changelog section.
    if (!releaseFor(version)) return;
    this.show(version);
  }
}

export const whatsNew = new WhatsNew();
