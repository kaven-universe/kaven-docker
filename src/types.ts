export type AuthType = "anonymous" | "basic";
export type ProxyMode = "global" | "direct" | "profile" | "custom";

export interface RegistryConfig {
  id: string;
  name: string;
  url: string;
  username: string;
  authType: AuthType;
  allowHttp: boolean;
  skipTlsVerify: boolean;
  caCertPath: string | null;
  timeoutSecs: number;
  uploadChunkSizeMb: number;
  provider: string;
  repositories: string[];
  proxyMode: ProxyMode;
  proxyUrl: string;
  proxyUsername: string;
  proxyId: string | null;
  proxyHasPassword: boolean;
  hasPassword: boolean;
}

export interface SaveRegistryInput extends Omit<RegistryConfig, "hasPassword" | "proxyHasPassword"> {
  password?: string | null;
  proxyPassword?: string | null;
}

export interface AppSettings {
  defaultProxyId: string | null;
}

export type SaveAppSettingsInput = AppSettings;

export interface ProxyProfile {
  id: string;
  name: string;
  url: string;
  username: string;
  hasPassword: boolean;
}

export interface SaveProxyProfileInput extends Omit<ProxyProfile, "hasPassword"> {
  password?: string | null;
}

export interface ConnectionStatus { registryVersion: string; message: string }
export interface RepositoryPage { repositories: string[] }
export interface TagList { name: string; tags: string[] }

export interface PlatformInfo {
  architecture: string;
  os: string;
  variant: string | null;
  digest: string;
  size: number;
}

export interface LayerInfo { mediaType: string; digest: string; size: number }

export interface ImageDetails {
  repository: string;
  reference: string;
  digest: string;
  mediaType: string;
  size: number;
  created: string | null;
  architecture: string | null;
  os: string | null;
  labels: Record<string, string>;
  layers: LayerInfo[];
  platforms: PlatformInfo[];
}

export interface SyncResult {
  syncedTags: number;
  copiedBlobs: number;
  skippedBlobs: number;
  cachedBlobs: number;
  transferredBytes: number;
  succeededRepositories: number;
  failedRepositories: string[];
}

export interface SyncPlatform {
  os: string;
  architecture: string;
  variant: string | null;
}

export interface SyncBatchItem {
  sourceRepository: string;
  targetRepository: string;
  tags: string[];
  platform: SyncPlatform | null;
}

export interface BatchSyncProgress {
  operationId: string;
  currentRepository: string;
  current: number;
  total: number;
  succeeded: number;
  failed: number;
  remaining: number;
  status: "running" | "success" | "failed" | "complete";
  error: string | null;
}

export type SyncStage = "prepare" | "manifests" | "blobs" | "manifestsWrite" | "tags" | "complete";
export type SyncProgressStatus = "pending" | "active" | "complete" | "error";

export interface SyncProgress {
  operationId: string;
  timestamp: number;
  stage: SyncStage;
  status: SyncProgressStatus;
  current: number;
  total: number;
  bytesCurrent: number;
  bytesTotal: number | null;
  message: string;
  detail: string | null;
  logPath: string;
  appendLog: boolean;
}
