import { invoke } from "@tauri-apps/api/core";

export type MailAttachment = {
  name: string;
  mime: string;
  size: number;
  inline: boolean;
};

export type MailPreview = {
  subject: string;
  from: string;
  to: string;
  cc: string;
  date: string | null;
  html: string | null;
  text: string | null;
  attachments: MailAttachment[];
};

export type MailSummary = {
  index: number;
  subject: string;
  from: string;
  date: string | null;
  snippet: string;
};

export type MboxPreview = {
  messages: MailSummary[];
};

/** Parses a `.eml` or Apple Mail `.emlx` message. */
export function openMail(path: string): Promise<MailPreview> {
  return invoke<MailPreview>("open_mail", { path });
}

/** Lists the messages of an `.mbox` mailbox. */
export function openMbox(path: string): Promise<MboxPreview> {
  return invoke<MboxPreview>("open_mbox", { path });
}

/** Parses one message of an `.mbox` by its index. */
export function readMboxMessage(path: string, index: number): Promise<MailPreview> {
  return invoke<MailPreview>("read_mbox_message", { path, index });
}
