import { invoke } from "@tauri-apps/api/core";

export type CertificateInfo = {
  subject: string;
  issuer: string;
  serial: string;
  notBefore: string;
  notAfter: string;
  signatureAlgorithm: string;
  publicKeyAlgorithm: string;
  keySize: number | null;
  isCa: boolean;
  selfSigned: boolean;
  subjectAltNames: string[];
  keyUsage: string[];
};

export type CertificatePreview = {
  kind: string;
  certificates: CertificateInfo[];
};

/** Parses a `.pem`/`.crt`/`.cer`/`.der` certificate or key. */
export function openCertificate(path: string): Promise<CertificatePreview> {
  return invoke<CertificatePreview>("open_certificate", { path });
}
