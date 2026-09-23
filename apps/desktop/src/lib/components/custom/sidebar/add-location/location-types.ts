import type { NetworkProtocol } from "$lib/remote/network-locations.js";
import type { Provider } from "$lib/remote/remote-locations.js";

export type LocationKind = NetworkProtocol | Provider;

export const LOCATION_TYPES: Record<LocationKind, { label: string; hint: string; mark: string }> = {
  smb: { label: "SMB", hint: "Windows, NAS, macOS sharing", mark: "SMB" },
  nfs: { label: "NFS", hint: "Linux and Unix exports", mark: "NFS" },
  webdav: { label: "WebDAV", hint: "Nextcloud, Synology, web servers", mark: "DAV" },
  sftp: { label: "SFTP", hint: "Files over SSH", mark: "SSH" },
  ftp: { label: "FTP", hint: "FTP and FTPS servers", mark: "FTP" },
  aws: { label: "Amazon S3", hint: "Buckets on AWS", mark: "aws" },
  r2: { label: "Cloudflare R2", hint: "Zero-egress object storage", mark: "R2" },
  custom: { label: "S3-compatible", hint: "MinIO, Backblaze, Wasabi, Ceph", mark: "" },
  gdrive: { label: "Google Drive", hint: "Connect your Google account", mark: "" },
  onedrive: { label: "OneDrive", hint: "Coming soon", mark: "OD" },
  dropbox: { label: "Dropbox", hint: "Coming soon", mark: "DB" },
  azblob: { label: "Azure Blob", hint: "Azure container storage", mark: "AZ" },
};

export const LOCATION_GROUPS: { label: string; kinds: LocationKind[] }[] = [
  { label: "Network share", kinds: ["smb", "nfs", "webdav"] },
  { label: "Server", kinds: ["sftp", "ftp"] },
  { label: "Cloud storage", kinds: ["aws", "r2", "custom", "gdrive", "onedrive", "dropbox", "azblob"] },
];
