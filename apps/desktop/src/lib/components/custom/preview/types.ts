export type PreviewKind =
  | "text"
  | "binary"
  | "directory"
  | "image"
  | "archive"
  | "pdf"
  | "video"
  | "audio"
  | "font"
  | "epub"
  | "spreadsheet"
  | "rtf"
  | "word"
  | "presentation"
  | "mail"
  | "mbox"
  | "contact"
  | "calendar"
  | "torrent"
  | "data"
  | "diff"
  | "log"
  | "comic"
  | "notebook"
  | "database"
  | "subtitle"
  | "certificate"
  | "model"
  | "geo"
  | "fb2"
  | "pcap"
  | "iso"
  | "msg"
  | "sketch"
  | "psd"
  | "dicom"
  | "mobi"
  | "avro"
  | "parquet"
  | "arrow";

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
