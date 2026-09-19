import { invoke } from "@tauri-apps/api/core";

export type MsgPreview = {
  subject: string;
  sender: string;
  to: string;
  cc: string;
  body: string;
  attachments: string[];
};

/** Extracts the readable properties of an Outlook `.msg` message. */
export function openMsg(path: string): Promise<MsgPreview> {
  return invoke<MsgPreview>("open_msg", { path });
}
