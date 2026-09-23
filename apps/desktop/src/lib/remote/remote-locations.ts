import { activity } from "$lib/transfers/jobs.js";
import { baseName } from "$lib/file-ops/files.js";
import { invoke } from "@tauri-apps/api/core";
import type { DirectoryEntry } from "$lib/components/custom/file-list/list-item.svelte";
import type { Location } from "$lib/tabs/tabs.js";

export type Provider = "aws" | "r2" | "custom" | "gdrive" | "onedrive" | "dropbox" | "azblob";

/** Form state of the add-location dialog. Mirrors `RemoteLocationInput` in `src-tauri/src/remote.rs`. */
export type RemoteLocationInput = {
  provider: Provider;
  name: string;
  accountId: string;
  endpoint: string;
  region: string;
  accessKeyId: string;
  secretAccessKey: string;
  bucket: string;
  prefix: string;
  pathStyle: boolean;
};

export type ConnectionTest = { buckets: string[] | null };

export const PROVIDERS: { id: Provider; label: string; hint: string }[] = [
  { id: "aws", label: "Amazon S3", hint: "Buckets on AWS" },
  { id: "r2", label: "Cloudflare R2", hint: "Zero-egress object storage" },
  { id: "custom", label: "S3 compatible", hint: "MinIO, Backblaze, Wasabi, Ceph" },
  { id: "gdrive", label: "Google Drive", hint: "Connect securely with Google" },
  { id: "onedrive", label: "OneDrive", hint: "OAuth connection — setup required" },
  { id: "dropbox", label: "Dropbox", hint: "OAuth connection — setup required" },
  { id: "azblob", label: "Azure Blob", hint: "Container storage — setup required" },
];

export const AWS_REGIONS = [
  "us-east-1",
  "us-east-2",
  "us-west-1",
  "us-west-2",
  "ca-central-1",
  "sa-east-1",
  "eu-west-1",
  "eu-west-2",
  "eu-west-3",
  "eu-central-1",
  "eu-north-1",
  "ap-south-1",
  "ap-northeast-1",
  "ap-northeast-2",
  "ap-southeast-1",
  "ap-southeast-2",
];

export function emptyInput(provider: Provider = "aws"): RemoteLocationInput {
  return {
    provider,
    name: "",
    accountId: "",
    endpoint: "",
    region: provider === "r2" ? "auto" : "us-east-1",
    accessKeyId: "",
    secretAccessKey: "",
    bucket: "",
    prefix: "",
    pathStyle: true,
  };
}

/** Switch provider, keeping what the user already typed that still applies. */
export function withProvider(input: RemoteLocationInput, provider: Provider): RemoteLocationInput {
  const { name, accessKeyId, secretAccessKey, bucket, prefix } = input;
  return { ...emptyInput(provider), name, accessKeyId, secretAccessKey, bucket, prefix };
}

/** Labels of required fields that are still empty. The backend validates formats. */
export function missingFields(input: RemoteLocationInput): string[] {
  if (isPendingProvider(input.provider)) return ["Provider setup"];
  if (input.provider === "gdrive") return [];
  const missing: string[] = [];
  if (input.provider === "r2" && !input.accountId.trim()) missing.push("Account ID");
  if ((input.provider === "custom" || input.provider === "azblob") && !input.endpoint.trim()) missing.push("Endpoint URL");
  if (input.provider === "azblob" && !input.bucket.trim()) missing.push("Container");
  if (!input.accessKeyId.trim()) missing.push("Access key ID");
  if (!input.secretAccessKey) missing.push("Secret access key");
  return missing;
}

export function isPendingProvider(provider: Provider): boolean {
  return provider === "onedrive" || provider === "dropbox";
}

export function describeTest(input: RemoteLocationInput, result: ConnectionTest): string {
  if (result.buckets === null) return `Bucket ${input.bucket.trim()} is reachable`;
  const count = result.buckets.length;
  return `Connected, ${count} ${count === 1 ? "bucket" : "buckets"} available`;
}

const SCHEME = "s3://";

/** Remote paths look like `s3://<location-id>/<bucket>/<key>`; see `src-tauri/src/remote.rs`. */
export function isRemotePath(path: string): boolean {
  return path.startsWith(SCHEME);
}

/** Standard `s3://bucket/key` URI for a remote path, dropping the internal location id. */
export function toS3Uri(path: string): string {
  if (!isRemotePath(path)) return path;
  const rest = path.slice(SCHEME.length);
  const slash = rest.indexOf("/");
  return slash === -1 ? SCHEME : SCHEME + rest.slice(slash + 1);
}

export function testRemoteLocation(input: RemoteLocationInput): Promise<ConnectionTest> {
  return invoke<ConnectionTest>("test_remote_location", { input });
}

export function addRemoteLocation(input: RemoteLocationInput): Promise<Location> {
  return activity.action(`Add location: ${input.name}`, "", () =>
    invoke<Location>("add_remote_location", { input }),
  );
}

export function connectGoogleDrive(input: Pick<RemoteLocationInput, "name" | "prefix">): Promise<Location> {
  return activity.action("Connect Google Drive", "", () => invoke<Location>("connect_google_drive", { input }));
}

/** Downloads an object to the app cache and returns the local copy's path. */
export function downloadRemoteFile(path: string): Promise<string> {
  return activity.action(`Download: ${baseName(path)}`, path, () =>
    invoke<string>("download_remote_file", { path }),
  );
}

export function removeRemoteLocation(location: Location): Promise<void> {
  return activity.action(`Remove location: ${location.name}`, location.path, () =>
    invoke("remove_remote_location", { path: location.path }),
  );
}

export type CreatedBucket = { bucket: DirectoryEntry; warning: string | null };

/** Creates a bucket from the bucket list of an account location, optionally with settings. */
export function createRemoteBucket(
  location: string,
  name: string,
  options: { versioning?: boolean; public?: boolean } = {},
): Promise<CreatedBucket> {
  return activity.track("create", `Create bucket: ${name}`, location, () =>
    invoke<CreatedBucket>("create_remote_bucket", { location, name, ...options }),
  );
}

export function remoteProvider(path: string): Promise<Provider> {
  return invoke<Provider>("remote_provider", { path });
}

export type BucketSettings = {
  provider: Provider;
  /** `null` when the service doesn't support versioning. */
  versioning: "off" | "enabled" | "suspended" | null;
  /** `null` when public access can't be managed through the S3 API. */
  publicAccess: { public: boolean; publicElsewhere: boolean } | null;
};

export function getBucketSettings(path: string): Promise<BucketSettings> {
  return invoke<BucketSettings>("bucket_settings", { path });
}

export function setBucketVersioning(path: string, enabled: boolean): Promise<BucketSettings> {
  return activity.action(`${enabled ? "Enable" : "Disable"} bucket versioning`, path, () =>
    invoke<BucketSettings>("set_bucket_versioning", { path, enabled }),
  );
}

export function setBucketPublic(path: string, isPublic: boolean): Promise<BucketSettings> {
  return activity.action(`Make bucket ${isPublic ? "public" : "private"}`, path, () =>
    invoke<BucketSettings>("set_bucket_public", { path, public: isPublic }),
  );
}

/** Deletes an empty bucket. */
export function deleteRemoteBucket(path: string): Promise<void> {
  return activity.track("delete", `Delete bucket: ${path}`, "", () =>
    invoke<void>("delete_remote_bucket", { path }),
  );
}

export function deleteRemoteItems(paths: string[]): Promise<void> {
  return activity.track(
    "delete",
    paths.length === 1 ? `Delete: ${paths[0]}` : `Delete ${paths.length} items`,
    "",
    () => invoke<void>("delete_remote_items", { paths }),
  );
}

export function uploadRemoteFiles(destination: string, sources: string[]): Promise<void> {
  return invoke("upload_remote_files", { destination, sources });
}

export function downloadRemoteItems(paths: string[], destination: string): Promise<void> {
  return invoke("download_remote_items", { paths, destination });
}
