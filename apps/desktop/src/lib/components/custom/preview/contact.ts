import { invoke } from "@tauri-apps/api/core";

export type Contact = {
  name: string;
  organization: string;
  title: string;
  emails: string[];
  phones: string[];
  addresses: string[];
  urls: string[];
  note: string;
};

/** Parses one or more vCards from a `.vcf` file. */
export function openVcards(path: string): Promise<Contact[]> {
  return invoke<Contact[]>("open_vcards", { path });
}
