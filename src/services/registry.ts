import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, ConnectionStatus, ImageDetails, ProxyProfile, RegistryConfig, RepositoryPage, SaveAppSettingsInput, SaveProxyProfileInput, SaveRegistryInput, SyncBatchItem, SyncPlatform, SyncResult, TagList } from "../types";

export const registryApi = {
  getAppSettings: () => invoke<AppSettings>("get_app_settings"),
  saveAppSettings: (settings: SaveAppSettingsInput) => invoke<AppSettings>("save_app_settings", { settings }),
  listProxyProfiles: () => invoke<ProxyProfile[]>("list_proxy_profiles"),
  saveProxyProfile: (proxy: SaveProxyProfileInput) => invoke<ProxyProfile>("save_proxy_profile", { proxy }),
  deleteProxyProfile: (id: string) => invoke<void>("delete_proxy_profile", { id }),
  listRegistries: () => invoke<RegistryConfig[]>("list_registries"),
  saveRegistry: (registry: SaveRegistryInput) => invoke<RegistryConfig>("save_registry", { registry }),
  deleteRegistry: (id: string) => invoke<void>("delete_registry", { id }),
  reorderRegistries: (registryIds: string[]) => invoke<RegistryConfig[]>("reorder_registries", { registryIds }),
  addRepositories: (registryId: string, repositories: string[]) => invoke<string[]>("add_repositories", { registryId, repositories }),
  removeRepository: (registryId: string, repository: string) => invoke<void>("remove_repository", { registryId, repository }),
  testConnection: (registryId: string) => invoke<ConnectionStatus>("test_connection", { registryId }),
  listRepositories: (registryId: string) => invoke<RepositoryPage>("list_repositories", { registryId }),
  discoverRepositories: (registryId: string, namespace: string) => invoke<RepositoryPage>("discover_repositories", { registryId, namespace }),
  listTags: (registryId: string, repository: string) => invoke<TagList>("list_tags", { registryId, repository }),
  imageDetails: (registryId: string, repository: string, reference: string) => invoke<ImageDetails>("get_image_details", { registryId, repository, reference }),
  deleteManifest: (registryId: string, repository: string, reference: string) => invoke<string>("delete_manifest", { registryId, repository, reference }),
  syncImages: (operationId: string, sourceRegistryId: string, targetRegistryId: string, sourceRepository: string, targetRepository: string, tags: string[], platform: SyncPlatform | null) =>
    invoke<SyncResult>("sync_images", { operationId, sourceRegistryId, targetRegistryId, sourceRepository, targetRepository, tags, platform }),
  cancelSync: (operationId: string) => invoke<boolean>("cancel_sync", { operationId }),
  syncImagesBatch: (operationId: string, sourceRegistryId: string, targetRegistryId: string, items: SyncBatchItem[]) =>
    invoke<SyncResult>("sync_images_batch", { operationId, sourceRegistryId, targetRegistryId, items }),
};
