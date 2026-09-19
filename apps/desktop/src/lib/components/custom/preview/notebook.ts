import { invoke } from "@tauri-apps/api/core";

export type NotebookOutput = {
  kind: "stream" | "error" | "html" | "image" | "text" | string;
  mime: string;
  text: string;
};

export type NotebookCell = {
  kind: "markdown" | "code" | "raw" | string;
  source: string;
  outputs: NotebookOutput[];
};

export type NotebookPreview = {
  language: string;
  kernel: string;
  cells: NotebookCell[];
  truncated: boolean;
};

/** Parses a Jupyter `.ipynb` notebook into cells and outputs. */
export function openNotebook(path: string): Promise<NotebookPreview> {
  return invoke<NotebookPreview>("open_notebook", { path });
}
