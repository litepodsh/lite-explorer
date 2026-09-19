import { invoke } from "@tauri-apps/api/core";

export type Fb2Preview = {
  title: string;
  author: string;
  annotation: string;
  paragraphs: string[];
};

/** Parses a FictionBook `.fb2` into title, author and paragraphs. */
export function openFb2(path: string): Promise<Fb2Preview> {
  return invoke<Fb2Preview>("open_fb2", { path });
}
