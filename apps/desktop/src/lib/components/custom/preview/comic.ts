import { invoke } from "@tauri-apps/api/core";

export type ComicPreview = {
  base: string;
  pages: string[];
};

/** Lists the pages of a `.cbz` comic book archive. */
export function openComic(path: string): Promise<ComicPreview> {
  return invoke<ComicPreview>("open_comic", { path });
}
