export type PreviewKind = "text" | "binary" | "directory" | "image" | "archive";

export type FilePreview = {
  name: string;
  size: number;
  created: number | null;
  modified: number | null;
  kind: PreviewKind;
  content: string | null;
  src: string | null;
  truncated: boolean;
};
