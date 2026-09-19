import { invoke } from "@tauri-apps/api/core";

export type WordDocument = {
  title: string | null;
  html: string;
};

export type Slide = {
  title: string;
  html: string;
};

export type Presentation = {
  title: string | null;
  slides: Slide[];
};

/** Renders a Word document (`.docx`, `.odt`, flat `.fodt`) to HTML. */
export function openWord(path: string): Promise<WordDocument> {
  return invoke<WordDocument>("open_word", { path });
}

/** Renders a presentation (`.pptx`, `.odp`, flat `.fodp`) into slides. */
export function openPresentation(path: string): Promise<Presentation> {
  return invoke<Presentation>("open_presentation", { path });
}
