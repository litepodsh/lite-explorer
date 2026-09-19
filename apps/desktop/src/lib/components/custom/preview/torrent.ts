import { invoke } from "@tauri-apps/api/core";

export type TorrentFile = {
  path: string;
  size: number;
};

export type TorrentPreview = {
  name: string;
  announce: string;
  announceList: string[];
  comment: string;
  createdBy: string;
  creationDate: number | null;
  pieceLength: number;
  totalSize: number;
  pieceCount: number;
  private: boolean;
  files: TorrentFile[];
};

/** Reads BitTorrent metainfo from a `.torrent` file. */
export function openTorrent(path: string): Promise<TorrentPreview> {
  return invoke<TorrentPreview>("open_torrent", { path });
}
