export type DeviceInfo = {
  name: string;
  chip: string | null;
  cores: number;
  memory_bytes: number | null;
};

export type VolumeInfo = {
  name: string;
  mount_point: string;
  file_system: string | null;
  total_bytes: number;
  free_bytes: number;
  is_primary: boolean;
};

export type DiskOverview = {
  device: DeviceInfo;
  volumes: VolumeInfo[];
  home: string | null;
};

export type FolderUsageEntry = {
  name: string;
  path: string;
  bytes: number;
  is_hidden: boolean;
};

export type FolderUsage = {
  root: string;
  total_bytes: number;
  /** Unix seconds of the last finished scan, null before the first one. */
  scanned_at: number | null;
  scanning: boolean;
  entries: FolderUsageEntry[];
  /** Total size of the OS trash, or null when it can’t be measured on this platform. */
  trash_bytes: number | null;
};

export type FolderUsageProgress = {
  root: string;
  scanned_bytes: number;
  current: string | null;
  entry: FolderUsageEntry | null;
};

export type FolderUsageError = { root: string; message: string };
