import { invoke } from "@tauri-apps/api/core";

export type PcapPacket = {
  index: number;
  timeMs: number;
  length: number;
  protocol: string;
};

export type PcapPreview = {
  linkType: string;
  packets: PcapPacket[];
  truncated: boolean;
};

/** Lists the packets of a `.pcap` capture. */
export function openPcap(path: string): Promise<PcapPreview> {
  return invoke<PcapPreview>("open_pcap", { path });
}
