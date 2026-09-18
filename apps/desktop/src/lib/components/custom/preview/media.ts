import { invoke } from "@tauri-apps/api/core";

export type MediaUrl = {
  url: string;
  size: number;
};

/**
 * Asks the backend for a streamable `media://` URL for `path`.
 *
 * Local files are served on demand. S3 objects are proxied with range requests.
 * SFTP/FTP files are downloaded to the cache first, so this call can take a while
 * for large remote media.
 */
export function fetchMediaUrl(path: string): Promise<MediaUrl> {
  return invoke<MediaUrl>("media_url", { path });
}

/** Opens (or focuses) the media-only viewer window for `path`. */
export function openViewer(path: string): Promise<void> {
  return invoke("open_viewer", { path });
}

/** The path the viewer window should display when it first mounts. */
export function fetchViewerTarget(): Promise<string | null> {
  return invoke<string | null>("viewer_target");
}
