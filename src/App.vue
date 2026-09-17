<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, reactive, ref, watch } from "vue";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import { useI18n } from "vue-i18n";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  ArrowRight, Boxes, Box, Check, ChevronRight, CircleAlert, Clipboard, CloudUpload, Database, Download,
  ArrowDown, ArrowUp, ExternalLink, Folder, GripVertical, KeyRound, Layers3, LoaderCircle, Maximize2, Minimize2, MoreHorizontal,
  Network, Package, Plus, RefreshCw, Search, Server, Settings2, ShieldCheck,
  Tag, Trash2, X,
} from "@lucide/vue";
import { registryApi } from "./services/registry";
import type { BatchSyncProgress, ProxyProfile, RegistryConfig, SaveRegistryInput, SyncBatchItem, SyncPlatform, SyncProgress, SyncResult, SyncStage } from "./types";
import type { AppLocale } from "./i18n";

const queryClient = useQueryClient();
const { t, locale } = useI18n();
document.documentElement.lang = locale.value;
const selectedRegistryId = ref("");
const draggedRegistryId = ref("");
const selectedRepository = ref("");
const selectedTag = ref("");
const repositorySearch = ref("");
const tagSearch = ref("");
const showRegistryDialog = ref(false);
const showGlobalSettingsDialog = ref(false);
const selectedProxyId = ref("");
const defaultProxyId = ref("");
const showRepositoryDialog = ref(false);
const repositoryAddMode = ref<"discover" | "manual">("discover");
const repositoryNamespace = ref("");
const repositoryInput = ref("");
const discoveredRepositories = ref<string[]>([]);
const selectedRepositoryCandidates = ref<string[]>([]);
const showDeleteDialog = ref(false);
const showSyncDialog = ref(false);
const syncMode = ref<"single" | "batch">("single");
const syncSourceRegistryId = ref("");
const syncSourceRepository = ref("");
const syncTargetRegistryId = ref("");
const syncTargetRepository = ref("");
const selectedSyncTags = ref<string[]>([]);
const syncTagSearch = ref("");
const syncPlatformMode = ref<"amd64" | "arm64" | "all" | "custom">("amd64");
const customSyncPlatform = reactive({ os: "linux", architecture: "amd64", variant: "" });
const batchRepositorySearch = ref("");
const batchSyncRepositories = ref<string[]>([]);
const activeBatchRepository = ref("");
const batchTagOptions = ref<Record<string, string[]>>({});
const batchSelectedTags = ref<Record<string, string[]>>({});
const batchTargetRepositories = ref<Record<string, string>>({});
const batchTagLoading = ref<string[]>([]);
const batchTagErrors = ref<Record<string, string>>({});
const syncOperationId = ref("");
const syncRunStatus = ref<"idle" | "running" | "success" | "error" | "cancelled">("idle");
const syncCancelRequested = ref(false);
const syncProgressByStage = ref<Partial<Record<SyncStage, SyncProgress>>>({});
const syncLogs = ref<SyncProgress[]>([]);
const syncLogPath = ref("");
const syncResult = ref<SyncResult | null>(null);
const batchSyncProgress = ref<BatchSyncProgress | null>(null);
const batchFailedRepositories = ref<string[]>([]);
const syncMaximized = ref(false);
const syncLogView = ref<HTMLElement | null>(null);
let unlistenSyncProgress: UnlistenFn | null = null;
let unlistenBatchSyncProgress: UnlistenFn | null = null;
let closeAfterSyncCancel = false;
let repositoryDiscoveryTimer: number | undefined;
const editingRegistryId = ref<string | null>(null);
const toast = ref<{ message: string; tone: "success" | "error" } | null>(null);
let toastTimer: number | undefined;
const proxyErrorCodes = new Set(["proxiesRead", "proxiesFormat", "proxiesSave", "proxyMissing", "proxyInUse", "invalidProxyName", "invalidProxyUrl", "proxyAuthUnsupported"]);
const syncErrorCodes = new Set(["invalidSyncOperation", "invalidSyncPlatform", "syncPlatformNotFound", "syncCancelled", "syncLogCreate", "syncTempCreate", "syncTempWrite", "syncTempRead", "syncCacheCreate", "syncCacheRead", "syncCacheWrite", "syncCacheDigest"]);
const syncBatchErrorCodes = new Set(["syncBatchSelectionRequired"]);
const discoveryErrorCodes = new Set(["invalidNamespace", "repositoryDiscoveryUnsupported", "repositoryDiscoveryAuthRequired", "repositoryDiscoveryFormat"]);
const registryOrderErrorCodes = new Set(["invalidRegistryOrder"]);

interface RegistryPreset {
  provider: string;
  name: string;
  url: string;
  mark: string;
  example: string;
}

const registryPresets: RegistryPreset[] = [
  { provider: "dockerHub", name: "Docker Hub", url: "https://registry-1.docker.io", mark: "DH", example: "library/nginx" },
  { provider: "ghcr", name: "GitHub Container Registry", url: "https://ghcr.io", mark: "GH", example: "owner/image" },
  { provider: "quay", name: "Quay.io", url: "https://quay.io", mark: "Q", example: "organization/image" },
  { provider: "kubernetes", name: "Kubernetes Registry", url: "https://registry.k8s.io", mark: "K8S", example: "pause" },
  { provider: "mcr", name: "Microsoft Container Registry", url: "https://mcr.microsoft.com", mark: "MS", example: "dotnet/runtime" },
];

const form = reactive<SaveRegistryInput>({
  id: "",
  name: "",
  url: "https://",
  username: "",
  authType: "anonymous",
  password: "",
  allowHttp: false,
  skipTlsVerify: false,
  caCertPath: null,
  timeoutSecs: 15,
  uploadChunkSizeMb: 32,
  provider: "generic",
  repositories: [],
  proxyMode: "global",
  proxyUrl: "",
  proxyUsername: "",
  proxyId: null,
  proxyPassword: "",
});

type ProxyProtocol = "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h";

const proxyForm = reactive({
  id: "",
  name: "",
  protocol: "http" as ProxyProtocol,
  hostname: "127.0.0.1",
  port: 8080,
  username: "",
  password: "",
});

const proxyProtocols: ProxyProtocol[] = ["http", "https", "socks4", "socks4a", "socks5", "socks5h"];
const isSocks4Protocol = computed(() => ["socks4", "socks4a"].includes(proxyForm.protocol));

function defaultProxyPort(protocol: ProxyProtocol) {
  if (protocol === "http") return 8080;
  if (protocol === "https") return 443;
  return 1080;
}

watch(() => proxyForm.protocol, (protocol, previous) => {
  if (proxyForm.port === defaultProxyPort(previous)) proxyForm.port = defaultProxyPort(protocol);
  if (["socks4", "socks4a"].includes(protocol)) {
    proxyForm.username = "";
    proxyForm.password = "";
  }
});

function notify(message: string, tone: "success" | "error" = "success") {
  toast.value = { message, tone };
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (toast.value = null), 3200);
}

function errorText(error: unknown) {
  const message = typeof error === "string" ? error : error instanceof Error ? error.message : t("errors.unknown");
  if (!message.startsWith("KAVEN_DOCKER|")) return message;
  const [, code, ...details] = message.split("|");
  const key = proxyErrorCodes.has(code) ? `proxyErrors.${code}` : syncErrorCodes.has(code) ? `syncErrors.${code}` : syncBatchErrorCodes.has(code) ? `syncBatchErrors.${code}` : discoveryErrorCodes.has(code) ? `discoveryErrors.${code}` : registryOrderErrorCodes.has(code) ? `registryOrderErrors.${code}` : `backend.${code}`;
  const localized = t(key);
  const detail = details.join("|").trim();
  return detail ? `${localized}。${t("errors.detail", { detail })}` : localized;
}

function isSyncCancelled(error: unknown) {
  const message = typeof error === "string" ? error : error instanceof Error ? error.message : "";
  return message.startsWith("KAVEN_DOCKER|syncCancelled");
}

function setLocale(nextLocale: AppLocale) {
  locale.value = nextLocale;
  localStorage.setItem("kaven-docker.locale", nextLocale);
  document.documentElement.lang = nextLocale;
}

const registriesQuery = useQuery({
  queryKey: ["registries"],
  queryFn: registryApi.listRegistries,
});

const appSettingsQuery = useQuery({
  queryKey: ["appSettings"],
  queryFn: registryApi.getAppSettings,
});

const proxyProfilesQuery = useQuery({
  queryKey: ["proxyProfiles"],
  queryFn: registryApi.listProxyProfiles,
});

const registries = computed(() => registriesQuery.data.value ?? []);
const privateSyncTargets = computed(() => registries.value.filter((registry) => registry.provider === "generic"));
const selectedRegistry = computed(() => registries.value.find((item) => item.id === selectedRegistryId.value));
const canRemoveSelectedRepository = computed(() => !!selectedRepository.value && !!selectedRegistry.value?.repositories.includes(selectedRepository.value));
const proxyProfiles = computed(() => proxyProfilesQuery.data.value ?? []);
const selectedProxy = computed(() => proxyProfiles.value.find((proxy) => proxy.id === selectedProxyId.value));
const registryProxyChoice = computed({
  get: () => form.proxyMode === "profile" ? `profile:${form.proxyId ?? ""}` : form.proxyMode,
  set: (value: string) => {
    if (value.startsWith("profile:")) {
      form.proxyMode = "profile";
      form.proxyId = value.slice(8);
    } else {
      form.proxyMode = value as "global" | "direct" | "custom";
      form.proxyId = null;
    }
  },
});
const repositoryExample = computed(() => registryPresets.find((item) => item.provider === selectedRegistry.value?.provider)?.example ?? "team/image");
const canDiscoverRepositories = computed(() => ["generic", "dockerHub", "ghcr", "quay"].includes(selectedRegistry.value?.provider ?? ""));
const repositoryCandidates = computed(() => {
  const namespace = repositoryNamespace.value.trim().replace(/^\/+|\/+$/g, "");
  const manual = repositoryInput.value
    .split(/[\n,]+/)
    .map((item) => item.trim().replace(/^\/+|\/+$/g, ""))
    .filter(Boolean)
    .map((item) => repositoryAddMode.value === "discover" && namespace && !item.includes("/") ? `${namespace}/${item}` : item);
  const discovered = repositoryAddMode.value === "discover" ? discoveredRepositories.value : [];
  return Array.from(new Set([...discovered, ...manual])).sort();
});
const allRepositoryCandidatesSelected = computed(() => repositoryCandidates.value.length > 0 && repositoryCandidates.value.every((repository) => selectedRepositoryCandidates.value.includes(repository)));
const syncStages: SyncStage[] = ["prepare", "manifests", "blobs", "manifestsWrite", "tags"];
const syncFailure = computed(() => [...syncLogs.value].reverse().find((entry) => entry.status === "error") ?? null);
const batchSyncProgressPercent = computed(() => {
  if (!batchSyncProgress.value?.total) return 0;
  return Math.min(100, (batchSyncProgress.value.current / batchSyncProgress.value.total) * 100);
});
const selectedSyncPlatform = computed<SyncPlatform | null>(() => {
  if (syncPlatformMode.value === "all") return null;
  if (syncPlatformMode.value === "amd64") return { os: "linux", architecture: "amd64", variant: null };
  if (syncPlatformMode.value === "arm64") return { os: "linux", architecture: "arm64", variant: null };
  return {
    os: customSyncPlatform.os.trim(),
    architecture: customSyncPlatform.architecture.trim(),
    variant: customSyncPlatform.variant.trim() || null,
  };
});

function syncProgressPercent(stage: SyncStage) {
  const progress = syncProgressByStage.value[stage];
  if (!progress) return 0;
  if (progress.status === "complete") return 100;
  if (!progress.total) return progress.status === "active" ? 4 : 0;
  let completed = progress.current;
  if (stage === "blobs" && progress.bytesTotal && progress.bytesCurrent) {
    completed += Math.min(1, progress.bytesCurrent / progress.bytesTotal);
  }
  return Math.min(100, Math.max(0, (completed / progress.total) * 100));
}

function resetSyncProgress() {
  syncOperationId.value = "";
  syncRunStatus.value = "idle";
  syncCancelRequested.value = false;
  syncProgressByStage.value = {};
  syncLogs.value = [];
  syncLogPath.value = "";
  syncResult.value = null;
  batchSyncProgress.value = null;
  batchFailedRepositories.value = [];
}

onBeforeUnmount(() => {
  unlistenSyncProgress?.();
  unlistenBatchSyncProgress?.();
  window.clearTimeout(repositoryDiscoveryTimer);
});

watch(registries, (items) => {
  if (!selectedRegistryId.value && items.length) selectedRegistryId.value = items[0].id;
  if (selectedRegistryId.value && !items.some((item) => item.id === selectedRegistryId.value)) {
    selectedRegistryId.value = items[0]?.id ?? "";
  }
}, { immediate: true });

watch(selectedRegistryId, () => {
  selectedRepository.value = "";
  selectedTag.value = "";
  repositorySearch.value = "";
  tagSearch.value = "";
});

const connectionQuery = useQuery({
  queryKey: computed(() => ["connection", selectedRegistryId.value]),
  queryFn: () => registryApi.testConnection(selectedRegistryId.value),
  enabled: computed(() => !!selectedRegistryId.value),
  retry: false,
});

const repositoriesQuery = useQuery({
  queryKey: computed(() => ["repositories", selectedRegistryId.value]),
  queryFn: () => registryApi.listRepositories(selectedRegistryId.value),
  enabled: computed(() => !!selectedRegistryId.value),
});

const repositories = computed(() => {
  const needle = repositorySearch.value.trim().toLowerCase();
  return (repositoriesQuery.data.value?.repositories ?? []).filter((name) => name.toLowerCase().includes(needle));
});

watch(() => repositoriesQuery.data.value, (page) => {
  if (!selectedRepository.value && page?.repositories.length) selectedRepository.value = page.repositories[0];
});

watch(selectedRepository, () => {
  selectedTag.value = "";
  tagSearch.value = "";
});

const tagsQuery = useQuery({
  queryKey: computed(() => ["tags", selectedRegistryId.value, selectedRepository.value]),
  queryFn: () => registryApi.listTags(selectedRegistryId.value, selectedRepository.value),
  enabled: computed(() => !!selectedRegistryId.value && !!selectedRepository.value),
});

const tags = computed(() => {
  const needle = tagSearch.value.trim().toLowerCase();
  return (tagsQuery.data.value?.tags ?? []).filter((name) => name.toLowerCase().includes(needle));
});

const syncRepositoriesQuery = useQuery({
  queryKey: computed(() => ["repositories", syncSourceRegistryId.value]),
  queryFn: () => registryApi.listRepositories(syncSourceRegistryId.value),
  enabled: computed(() => showSyncDialog.value && !!syncSourceRegistryId.value),
});

watch(() => syncRepositoriesQuery.data.value, (page) => {
  if (!showSyncDialog.value || !page) return;
  if (syncMode.value === "batch") return;
  if (!page.repositories.includes(syncSourceRepository.value)) {
    syncSourceRepository.value = page.repositories[0] ?? "";
    syncTargetRepository.value = syncSourceRepository.value;
    selectedSyncTags.value = [];
  }
});

const syncTagsQuery = useQuery({
  queryKey: computed(() => ["tags", syncSourceRegistryId.value, syncSourceRepository.value]),
  queryFn: () => registryApi.listTags(syncSourceRegistryId.value, syncSourceRepository.value),
  enabled: computed(() => showSyncDialog.value && !!syncSourceRegistryId.value && !!syncSourceRepository.value),
});

const syncTags = computed(() => {
  const needle = syncTagSearch.value.trim().toLowerCase();
  return (syncTagsQuery.data.value?.tags ?? []).filter((name) => name.toLowerCase().includes(needle));
});
const batchRepositories = computed(() => {
  const needle = batchRepositorySearch.value.trim().toLowerCase();
  return (syncRepositoriesQuery.data.value?.repositories ?? []).filter((repository) => repository.toLowerCase().includes(needle));
});
const activeBatchTags = computed(() => {
  const needle = syncTagSearch.value.trim().toLowerCase();
  return (batchTagOptions.value[activeBatchRepository.value] ?? []).filter((tag) => tag.toLowerCase().includes(needle));
});
const batchSelectedTagCount = computed(() => batchSyncRepositories.value.reduce((count, repository) => count + (batchSelectedTags.value[repository]?.length ?? 0), 0));

watch(() => tagsQuery.data.value, (result) => {
  if (!selectedTag.value && result?.tags.length) selectedTag.value = result.tags[0];
});

const detailsQuery = useQuery({
  queryKey: computed(() => ["details", selectedRegistryId.value, selectedRepository.value, selectedTag.value]),
  queryFn: () => registryApi.imageDetails(selectedRegistryId.value, selectedRepository.value, selectedTag.value),
  enabled: computed(() => !!selectedRegistryId.value && !!selectedRepository.value && !!selectedTag.value),
});

const saveMutation = useMutation({
  mutationFn: registryApi.saveRegistry,
  onSuccess: async (saved) => {
    await queryClient.invalidateQueries({ queryKey: ["registries"] });
    selectedRegistryId.value = saved.id;
    showRegistryDialog.value = false;
    notify(editingRegistryId.value ? t("toast.updated") : t("toast.added"));
  },
  onError: (error) => notify(errorText(error), "error"),
});

const saveAppSettingsMutation = useMutation({
  mutationFn: (defaultProxyId: string | null) => registryApi.saveAppSettings({ defaultProxyId }),
  onSuccess: async () => {
    await appSettingsQuery.refetch();
    await queryClient.invalidateQueries({ queryKey: ["connection"] });
    void queryClient.invalidateQueries({ queryKey: ["repositories"] });
    void queryClient.invalidateQueries({ queryKey: ["tags"] });
    void queryClient.invalidateQueries({ queryKey: ["details"] });
    notify(t("toast.proxySaved"));
  },
  onError: (error) => notify(errorText(error), "error"),
});

const saveProxyMutation = useMutation({
  mutationFn: registryApi.saveProxyProfile,
  onSuccess: async (saved) => {
    await proxyProfilesQuery.refetch();
    void queryClient.invalidateQueries({ queryKey: ["connection"] });
    void queryClient.invalidateQueries({ queryKey: ["repositories"] });
    void queryClient.invalidateQueries({ queryKey: ["tags"] });
    void queryClient.invalidateQueries({ queryKey: ["details"] });
    selectedProxyId.value = saved.id;
    populateProxyForm(saved);
    notify(t("toast.proxySaved"));
  },
  onError: (error) => notify(errorText(error), "error"),
});

const deleteProxyMutation = useMutation({
  mutationFn: registryApi.deleteProxyProfile,
  onSuccess: async () => {
    await proxyProfilesQuery.refetch();
    openNewProxy();
    notify(t("toast.proxyRemoved"));
  },
  onError: (error) => notify(errorText(error), "error"),
});

const addPresetMutation = useMutation({
  mutationFn: (preset: RegistryPreset) => registryApi.saveRegistry({
    id: crypto.randomUUID(),
    name: preset.name,
    url: preset.url,
    username: "",
    authType: "anonymous",
    password: null,
    allowHttp: false,
    skipTlsVerify: false,
    caCertPath: null,
    timeoutSecs: 15,
    uploadChunkSizeMb: 32,
    provider: preset.provider,
    repositories: [],
    proxyMode: "global",
    proxyUrl: "",
    proxyUsername: "",
    proxyId: null,
    proxyPassword: null,
  }),
  onSuccess: async (saved) => {
    await queryClient.invalidateQueries({ queryKey: ["registries"] });
    selectedRegistryId.value = saved.id;
    showRegistryDialog.value = false;
    notify(t("presets.added", { name: saved.name }));
  },
  onError: (error) => notify(errorText(error), "error"),
});

const addRepositoryMutation = useMutation({
  mutationFn: () => registryApi.addRepositories(selectedRegistryId.value, selectedRepositoryCandidates.value),
  onSuccess: async (added) => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["registries"] }),
      queryClient.invalidateQueries({ queryKey: ["repositories", selectedRegistryId.value] }),
    ]);
    selectedRepository.value = added[0] ?? selectedRepository.value;
    showRepositoryDialog.value = false;
    notify(t("repositories.addedMany", { count: added.length }));
  },
  onError: (error) => notify(errorText(error), "error"),
});

const discoverRepositoriesMutation = useMutation({
  mutationFn: (namespace: string) => registryApi.discoverRepositories(selectedRegistryId.value, namespace),
  onSuccess: (page, namespace) => {
    if (repositoryNamespace.value.trim().replace(/^\/+|\/+$/g, "") !== namespace) return;
    discoveredRepositories.value = page.repositories;
  },
});

const removeRepositoryMutation = useMutation({
  mutationFn: (repository: string) => registryApi.removeRepository(selectedRegistryId.value, repository),
  onSuccess: async (_result, repository) => {
    selectedRepository.value = "";
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["registries"] }),
      queryClient.invalidateQueries({ queryKey: ["repositories", selectedRegistryId.value] }),
    ]);
    notify(t("repositories.removed", { repository }));
  },
  onError: (error) => notify(errorText(error), "error"),
});

const deleteRegistryMutation = useMutation({
  mutationFn: registryApi.deleteRegistry,
  onSuccess: async () => {
    await queryClient.invalidateQueries({ queryKey: ["registries"] });
    showRegistryDialog.value = false;
    notify(t("toast.removed"));
  },
  onError: (error) => notify(errorText(error), "error"),
});

const reorderRegistriesMutation = useMutation({
  mutationFn: (registryIds: string[]) => registryApi.reorderRegistries(registryIds),
  onMutate: async (registryIds) => {
    await queryClient.cancelQueries({ queryKey: ["registries"] });
    const previous = queryClient.getQueryData<RegistryConfig[]>(["registries"]);
    const byId = new Map((previous ?? []).map((registry) => [registry.id, registry]));
    const reordered = registryIds.map((id) => byId.get(id)).filter((registry): registry is RegistryConfig => !!registry);
    queryClient.setQueryData<RegistryConfig[]>(["registries"], reordered);
    return { previous };
  },
  onSuccess: (saved) => {
    queryClient.setQueryData(["registries"], saved);
    notify(t("toast.registryOrderSaved"));
  },
  onError: (error, _registryIds, context) => {
    if (context?.previous) queryClient.setQueryData(["registries"], context.previous);
    notify(errorText(error), "error");
  },
});

const deleteManifestMutation = useMutation({
  mutationFn: () => registryApi.deleteManifest(selectedRegistryId.value, selectedRepository.value, selectedTag.value),
  onSuccess: async () => {
    showDeleteDialog.value = false;
    selectedTag.value = "";
    await queryClient.invalidateQueries({ queryKey: ["tags", selectedRegistryId.value, selectedRepository.value] });
    notify(t("toast.manifestDeleted"));
  },
  onError: (error) => notify(errorText(error), "error"),
});

const syncMutation = useMutation({
  mutationFn: (operationId: string) => syncMode.value === "batch"
    ? registryApi.syncImagesBatch(operationId, syncSourceRegistryId.value, syncTargetRegistryId.value, batchSyncRepositories.value.map((repository): SyncBatchItem => ({
      sourceRepository: repository,
      targetRepository: batchTargetRepositories.value[repository]?.trim() ?? repository,
      tags: batchSelectedTags.value[repository] ?? [],
      platform: selectedSyncPlatform.value ? { ...selectedSyncPlatform.value } : null,
    })))
    : registryApi.syncImages(
      operationId,
      syncSourceRegistryId.value,
      syncTargetRegistryId.value,
      syncSourceRepository.value,
      syncTargetRepository.value.trim(),
      selectedSyncTags.value,
      selectedSyncPlatform.value,
    ),
  onSuccess: (result) => {
    syncResult.value = result;
    batchFailedRepositories.value = result.failedRepositories ?? [];
    syncRunStatus.value = syncMode.value === "batch" && batchFailedRepositories.value.length ? "error" : "success";
    void queryClient.invalidateQueries({ queryKey: ["repositories", syncTargetRegistryId.value] });
    if (syncMode.value === "batch") {
      for (const repository of batchSyncRepositories.value) {
        const targetRepository = batchTargetRepositories.value[repository]?.trim() || repository;
        void queryClient.invalidateQueries({ queryKey: ["tags", syncTargetRegistryId.value, targetRepository] });
      }
    } else {
      void queryClient.invalidateQueries({ queryKey: ["tags", syncTargetRegistryId.value, syncTargetRepository.value.trim()] });
    }
    if (syncMode.value === "batch" && batchFailedRepositories.value.length) {
      notify(t("syncBatch.partialCompleted", { succeeded: result.succeededRepositories, failed: batchFailedRepositories.value.length }), "error");
    } else {
      notify(t("sync.completed", { tags: result.syncedTags, copied: result.copiedBlobs, skipped: result.skippedBlobs, cached: result.cachedBlobs }));
    }
  },
  onError: (error) => {
    const cancelled = isSyncCancelled(error);
    syncRunStatus.value = cancelled ? "cancelled" : "error";
    if (!cancelled && syncMode.value === "batch" && !batchFailedRepositories.value.length) {
      batchFailedRepositories.value = [...batchSyncRepositories.value];
    }
    notify(errorText(error), cancelled ? "success" : "error");
  },
  onSettled: () => {
    syncCancelRequested.value = false;
    if (closeAfterSyncCancel) {
      closeAfterSyncCancel = false;
      unlistenSyncProgress?.();
      unlistenSyncProgress = null;
      unlistenBatchSyncProgress?.();
      unlistenBatchSyncProgress = null;
      showSyncDialog.value = false;
    }
  },
});

function openNewRegistry() {
  editingRegistryId.value = null;
  Object.assign(form, {
    id: crypto.randomUUID(), name: "", url: "https://", username: "", authType: "anonymous",
    password: "", allowHttp: false, skipTlsVerify: false, caCertPath: null, timeoutSecs: 15, uploadChunkSizeMb: 32,
    provider: "generic", repositories: [], proxyMode: "global", proxyUrl: "",
    proxyUsername: "", proxyId: null, proxyPassword: "",
  });
  showRegistryDialog.value = true;
}

function openEditRegistry(registry: RegistryConfig) {
  editingRegistryId.value = registry.id;
  Object.assign(form, { ...registry, password: "", proxyPassword: "" });
  showRegistryDialog.value = true;
}

function submitRegistry() {
  const normalizedUrl = form.url.trim().replace(/\/+$/, "");
  if (!form.name.trim() || !normalizedUrl) return notify(t("validation.required"), "error");
  if (!/^https?:\/\//i.test(normalizedUrl)) return notify(t("validation.protocol"), "error");
  if (normalizedUrl.startsWith("http://") && !form.allowHttp) return notify(t("validation.allowHttp"), "error");
  if (form.proxyMode === "custom" && !isProxyUrl(form.proxyUrl)) return notify(t("validation.proxyUrl"), "error");
  if (form.proxyMode === "custom" && usesSocks4Auth(form.proxyUrl, form.proxyUsername)) return notify(t("proxyErrors.proxyAuthUnsupported"), "error");
  if (form.proxyMode === "profile" && !form.proxyId) return notify(t("validation.proxyRequired"), "error");
  saveMutation.mutate({ ...form, name: form.name.trim(), url: normalizedUrl, username: form.username.trim(), password: form.password || null, proxyUrl: form.proxyUrl.trim(), proxyUsername: form.proxyUsername.trim(), proxyPassword: form.proxyPassword || null });
}

function isProxyUrl(value: string) {
  try {
    const parsed = new URL(value.trim());
    return ["http:", "https:", "socks4:", "socks4a:", "socks5:", "socks5h:"].includes(parsed.protocol) && !!parsed.hostname && !parsed.username && !parsed.password;
  } catch {
    return false;
  }
}

function usesSocks4Auth(url: string, username: string) {
  try {
    return ["socks4:", "socks4a:"].includes(new URL(url.trim()).protocol) && !!username.trim();
  } catch {
    return false;
  }
}

function openGlobalSettings() {
  defaultProxyId.value = appSettingsQuery.data.value?.defaultProxyId ?? "";
  const first = proxyProfiles.value.find((proxy) => proxy.id === defaultProxyId.value) ?? proxyProfiles.value[0];
  if (first) {
    editProxy(first);
  } else {
    openNewProxy();
  }
  showGlobalSettingsDialog.value = true;
}

function submitGlobalSettings() {
  saveAppSettingsMutation.mutate(defaultProxyId.value || null);
}

function openNewProxy() {
  selectedProxyId.value = "";
  Object.assign(proxyForm, { id: crypto.randomUUID(), name: "", protocol: "http", hostname: "127.0.0.1", port: 8080, username: "", password: "" });
}

function editProxy(proxy: ProxyProfile) {
  selectedProxyId.value = proxy.id;
  populateProxyForm(proxy);
}

function populateProxyForm(proxy: ProxyProfile) {
  const parsed = new URL(proxy.url);
  const protocol = parsed.protocol.slice(0, -1) as ProxyProtocol;
  Object.assign(proxyForm, {
    id: proxy.id,
    name: proxy.name,
    protocol,
    hostname: parsed.hostname.replace(/^\[|\]$/g, ""),
    port: Number(parsed.port) || (protocol === "http" ? 80 : defaultProxyPort(protocol)),
    username: proxy.username,
    password: "",
  });
}

function proxyUrlFromForm() {
  const hostname = proxyForm.hostname.trim();
  const formattedHost = hostname.includes(":") && !hostname.startsWith("[") ? `[${hostname}]` : hostname;
  return `${proxyForm.protocol}://${formattedHost}:${proxyForm.port}`;
}

function proxySummary(proxy: ProxyProfile) {
  const parsed = new URL(proxy.url);
  return `${parsed.hostname}${parsed.port ? `:${parsed.port}` : ""} · ${parsed.protocol.slice(0, -1).toUpperCase()}`;
}

function submitProxy() {
  if (!proxyForm.name.trim()) return notify(t("validation.proxyName"), "error");
  const hostname = proxyForm.hostname.trim();
  const url = proxyUrlFromForm();
  if (!hostname || /[\s/@?#]/.test(hostname) || !Number.isInteger(proxyForm.port) || proxyForm.port < 1 || proxyForm.port > 65535 || !isProxyUrl(url)) return notify(t("validation.proxyAddress"), "error");
  saveProxyMutation.mutate({
    id: proxyForm.id,
    name: proxyForm.name.trim(),
    url,
    username: proxyForm.username.trim(),
    password: proxyForm.password || null,
  });
}

function openAddRepository() {
  repositoryAddMode.value = canDiscoverRepositories.value ? "discover" : "manual";
  repositoryNamespace.value = selectedRepository.value.includes("/")
    ? selectedRepository.value.split("/")[0]
    : selectedRegistry.value?.username ?? "";
  repositoryInput.value = "";
  discoveredRepositories.value = [];
  selectedRepositoryCandidates.value = [];
  discoverRepositoriesMutation.reset();
  showRepositoryDialog.value = true;
  void nextTick(scheduleRepositoryDiscovery);
}

function submitRepository() {
  if (!selectedRepositoryCandidates.value.length) return notify(t("repositories.required"), "error");
  addRepositoryMutation.mutate();
}

function toggleRepositoryCandidate(repository: string) {
  selectedRepositoryCandidates.value = selectedRepositoryCandidates.value.includes(repository)
    ? selectedRepositoryCandidates.value.filter((item) => item !== repository)
    : [...selectedRepositoryCandidates.value, repository];
}

function toggleAllRepositoryCandidates() {
  selectedRepositoryCandidates.value = allRepositoryCandidatesSelected.value ? [] : [...repositoryCandidates.value];
}

function discoverRepositoriesNow() {
  window.clearTimeout(repositoryDiscoveryTimer);
  const namespace = repositoryNamespace.value.trim().replace(/^\/+|\/+$/g, "");
  if (!namespace || !canDiscoverRepositories.value) return;
  discoverRepositoriesMutation.mutate(namespace);
}

function scheduleRepositoryDiscovery() {
  window.clearTimeout(repositoryDiscoveryTimer);
  discoveredRepositories.value = [];
  discoverRepositoriesMutation.reset();
  const namespace = repositoryNamespace.value.trim().replace(/^\/+|\/+$/g, "");
  if (repositoryAddMode.value !== "discover" || !namespace || !canDiscoverRepositories.value) return;
  repositoryDiscoveryTimer = window.setTimeout(discoverRepositoriesNow, 450);
}

watch(repositoryNamespace, () => {
  scheduleRepositoryDiscovery();
});

watch(repositoryAddMode, (mode) => {
  discoveredRepositories.value = [];
  selectedRepositoryCandidates.value = mode === "manual" ? repositoryCandidates.value : [];
  discoverRepositoriesMutation.reset();
  if (mode === "discover") scheduleRepositoryDiscovery();
});

watch(repositoryCandidates, (candidates, previous = []) => {
  const retained = selectedRepositoryCandidates.value.filter((item) => candidates.includes(item));
  const added = candidates.filter((item) => !previous.includes(item));
  selectedRepositoryCandidates.value = Array.from(new Set([...retained, ...added]));
});

function openSync() {
  if (!privateSyncTargets.value.length) return notify(t("sync.noPrivateTarget"), "error");
  unlistenSyncProgress?.();
  unlistenSyncProgress = null;
  unlistenBatchSyncProgress?.();
  unlistenBatchSyncProgress = null;
  closeAfterSyncCancel = false;
  resetSyncProgress();
  syncMode.value = "single";
  syncMaximized.value = false;
  syncSourceRegistryId.value = selectedRegistryId.value || registries.value[0]?.id || "";
  syncSourceRepository.value = selectedRepository.value;
  syncTargetRegistryId.value = privateSyncTargets.value.find((registry) => registry.id === selectedRegistryId.value)?.id ?? privateSyncTargets.value[0].id;
  syncTargetRepository.value = selectedRepository.value;
  selectedSyncTags.value = selectedTag.value ? [selectedTag.value] : [];
  syncTagSearch.value = "";
  syncPlatformMode.value = "amd64";
  Object.assign(customSyncPlatform, { os: "linux", architecture: "amd64", variant: "" });
  batchRepositorySearch.value = "";
  batchSyncRepositories.value = [];
  activeBatchRepository.value = "";
  batchTagOptions.value = {};
  batchSelectedTags.value = {};
  batchTargetRepositories.value = {};
  batchTagLoading.value = [];
  batchTagErrors.value = {};
  showSyncDialog.value = true;
}

function closeSyncDialog() {
  if (syncMutation.isPending.value) {
    closeAfterSyncCancel = true;
    void requestSyncCancellation();
    return;
  }
  unlistenSyncProgress?.();
  unlistenSyncProgress = null;
  unlistenBatchSyncProgress?.();
  unlistenBatchSyncProgress = null;
  showSyncDialog.value = false;
}

async function requestSyncCancellation() {
  if (!syncMutation.isPending.value || !syncOperationId.value || syncCancelRequested.value) return;
  syncCancelRequested.value = true;
  try {
    await registryApi.cancelSync(syncOperationId.value);
  } catch (error) {
    syncCancelRequested.value = false;
    closeAfterSyncCancel = false;
    notify(errorText(error), "error");
  }
}

function changeSyncSourceRegistry() {
  syncSourceRepository.value = "";
  syncTargetRepository.value = "";
  selectedSyncTags.value = [];
  syncTagSearch.value = "";
  batchSyncRepositories.value = [];
  activeBatchRepository.value = "";
  batchTagOptions.value = {};
  batchSelectedTags.value = {};
  batchTargetRepositories.value = {};
  batchTagErrors.value = {};
}

function changeSyncSourceRepository() {
  syncTargetRepository.value = syncSourceRepository.value;
  selectedSyncTags.value = [];
  syncTagSearch.value = "";
}

function toggleSyncTag(tag: string) {
  selectedSyncTags.value = selectedSyncTags.value.includes(tag)
    ? selectedSyncTags.value.filter((item) => item !== tag)
    : [...selectedSyncTags.value, tag];
}

function toggleAllSyncTags() {
  const visible = syncTags.value;
  const allSelected = visible.length > 0 && visible.every((tag) => selectedSyncTags.value.includes(tag));
  selectedSyncTags.value = allSelected
    ? selectedSyncTags.value.filter((tag) => !visible.includes(tag))
    : Array.from(new Set([...selectedSyncTags.value, ...visible]));
}

async function loadBatchTags(repository: string) {
  if (batchTagOptions.value[repository] || batchTagLoading.value.includes(repository)) return;
  batchTagLoading.value = [...batchTagLoading.value, repository];
  const errors = { ...batchTagErrors.value };
  delete errors[repository];
  batchTagErrors.value = errors;
  try {
    const result = await registryApi.listTags(syncSourceRegistryId.value, repository);
    batchTagOptions.value = { ...batchTagOptions.value, [repository]: result.tags };
    const defaults = ["latest", "stable"].filter((tag) => result.tags.includes(tag));
    batchSelectedTags.value = { ...batchSelectedTags.value, [repository]: defaults };
  } catch (error) {
    batchTagErrors.value = { ...batchTagErrors.value, [repository]: errorText(error) };
  } finally {
    batchTagLoading.value = batchTagLoading.value.filter((item) => item !== repository);
  }
}

function toggleBatchRepository(repository: string) {
  if (batchSyncRepositories.value.includes(repository)) {
    batchSyncRepositories.value = batchSyncRepositories.value.filter((item) => item !== repository);
    if (activeBatchRepository.value === repository) activeBatchRepository.value = batchSyncRepositories.value[0] ?? "";
    return;
  }
  batchSyncRepositories.value = [...batchSyncRepositories.value, repository];
  batchTargetRepositories.value = { ...batchTargetRepositories.value, [repository]: repository };
  activeBatchRepository.value = repository;
  void loadBatchTags(repository);
}

function activateBatchRepository(repository: string) {
  if (!batchSyncRepositories.value.includes(repository)) {
    toggleBatchRepository(repository);
  } else {
    activeBatchRepository.value = repository;
  }
}

async function toggleAllBatchRepositories() {
  const visible = batchRepositories.value;
  const allSelected = visible.length > 0 && visible.every((repository) => batchSyncRepositories.value.includes(repository));
  if (allSelected) {
    batchSyncRepositories.value = batchSyncRepositories.value.filter((repository) => !visible.includes(repository));
    activeBatchRepository.value = batchSyncRepositories.value[0] ?? "";
    return;
  }
  const added = visible.filter((repository) => !batchSyncRepositories.value.includes(repository));
  batchSyncRepositories.value = Array.from(new Set([...batchSyncRepositories.value, ...visible]));
  for (const repository of added) {
    batchTargetRepositories.value = { ...batchTargetRepositories.value, [repository]: repository };
  }
  activeBatchRepository.value ||= visible[0] ?? "";
  await Promise.all(added.map(loadBatchTags));
}

function toggleBatchTag(tag: string) {
  const repository = activeBatchRepository.value;
  if (!repository) return;
  const selected = batchSelectedTags.value[repository] ?? [];
  batchSelectedTags.value = {
    ...batchSelectedTags.value,
    [repository]: selected.includes(tag) ? selected.filter((item) => item !== tag) : [...selected, tag],
  };
}

function toggleAllBatchTags() {
  const repository = activeBatchRepository.value;
  if (!repository) return;
  const visible = activeBatchTags.value;
  const selected = batchSelectedTags.value[repository] ?? [];
  const allSelected = visible.length > 0 && visible.every((tag) => selected.includes(tag));
  batchSelectedTags.value = {
    ...batchSelectedTags.value,
    [repository]: allSelected ? selected.filter((tag) => !visible.includes(tag)) : Array.from(new Set([...selected, ...visible])),
  };
}

watch(syncMode, (mode) => {
  if (mode !== "batch" || !syncSourceRepository.value || batchSyncRepositories.value.length) return;
  toggleBatchRepository(syncSourceRepository.value);
});

async function submitSync() {
  if (!syncSourceRegistryId.value) return notify(t("sync.validationSource"), "error");
  if (!syncTargetRegistryId.value) return notify(t("sync.validationTarget"), "error");
  if (syncPlatformMode.value === "custom" && (!customSyncPlatform.os.trim() || !customSyncPlatform.architecture.trim())) return notify(t("sync.validationPlatform"), "error");
  if (syncMode.value === "batch") {
    if (!batchSyncRepositories.value.length) return notify(t("syncBatch.validationRepositories"), "error");
    if (batchSyncRepositories.value.some((repository) => !batchTargetRepositories.value[repository]?.trim())) return notify(t("sync.validationRepository"), "error");
    if (batchSyncRepositories.value.some((repository) => !(batchSelectedTags.value[repository]?.length))) return notify(t("syncBatch.validationTags"), "error");
  } else {
    if (!syncSourceRepository.value) return notify(t("sync.validationSourceRepository"), "error");
    if (!syncTargetRepository.value.trim()) return notify(t("sync.validationRepository"), "error");
    if (!selectedSyncTags.value.length) return notify(t("sync.validationTags"), "error");
  }
  resetSyncProgress();
  syncOperationId.value = crypto.randomUUID();
  try {
    unlistenSyncProgress?.();
    unlistenBatchSyncProgress?.();
    unlistenSyncProgress = await listen<SyncProgress>("sync-progress", ({ payload }) => {
      if (payload.operationId !== syncOperationId.value) return;
      syncLogPath.value = payload.logPath;
      if (payload.stage !== "complete") {
        const previous = syncProgressByStage.value[payload.stage];
        const progress = payload.status === "error" && previous ? {
          ...payload,
          current: previous.current,
          total: previous.total,
          bytesCurrent: previous.bytesCurrent,
          bytesTotal: previous.bytesTotal,
        } : payload;
        syncProgressByStage.value = { ...syncProgressByStage.value, [payload.stage]: progress };
      }
      if (payload.appendLog) {
        syncLogs.value = [...syncLogs.value, payload];
        void nextTick(() => {
          if (syncLogView.value) syncLogView.value.scrollTop = syncLogView.value.scrollHeight;
        });
      }
      if (payload.status === "error") syncRunStatus.value = "error";
    });
    unlistenBatchSyncProgress = syncMode.value === "batch"
      ? await listen<BatchSyncProgress>("sync-batch-progress", ({ payload }) => {
        if (payload.operationId !== syncOperationId.value) return;
        batchSyncProgress.value = payload;
        if (payload.status === "failed" && !batchFailedRepositories.value.includes(payload.currentRepository)) {
          batchFailedRepositories.value = [...batchFailedRepositories.value, payload.currentRepository];
        }
      })
      : null;
  } catch (error) {
    return notify(errorText(error), "error");
  }
  syncRunStatus.value = "running";
  syncMutation.mutate(syncOperationId.value);
}

async function retryFailedSync() {
  const failed = [...batchFailedRepositories.value];
  if (!failed.length || syncMutation.isPending.value) return;
  batchSyncRepositories.value = failed;
  await submitSync();
}

function editSyncSelection() {
  if (syncMutation.isPending.value) return;
  resetSyncProgress();
}

function syncLogText() {
  return syncLogs.value.map((entry) => {
    const time = new Date(entry.timestamp).toISOString();
    const detail = entry.detail ? ` | ${entry.detail}` : "";
    return `${time} [${entry.status.toUpperCase()}] [${entry.stage}] ${entry.message}${detail}`;
  }).join("\n");
}

function copySyncLogs() {
  return copyText(syncLogText(), t("toast.logsCopied"));
}

function refreshAll() {
  if (!selectedRegistryId.value) return;
  void queryClient.invalidateQueries({ queryKey: ["connection", selectedRegistryId.value] });
  void queryClient.invalidateQueries({ queryKey: ["repositories", selectedRegistryId.value] });
  if (selectedRepository.value) void queryClient.invalidateQueries({ queryKey: ["tags", selectedRegistryId.value, selectedRepository.value] });
  notify(t("toast.refreshing"));
}

function applyRegistryOrder(ordered: RegistryConfig[]) {
  if (reorderRegistriesMutation.isPending.value) return;
  reorderRegistriesMutation.mutate(ordered.map((registry) => registry.id));
}

function moveRegistry(registryId: string, offset: number) {
  const ordered = [...registries.value];
  const index = ordered.findIndex((registry) => registry.id === registryId);
  const target = index + offset;
  if (index < 0 || target < 0 || target >= ordered.length) return;
  [ordered[index], ordered[target]] = [ordered[target], ordered[index]];
  applyRegistryOrder(ordered);
}

function dropRegistry(targetId: string) {
  const sourceId = draggedRegistryId.value;
  draggedRegistryId.value = "";
  if (!sourceId || sourceId === targetId) return;
  const ordered = [...registries.value];
  const sourceIndex = ordered.findIndex((registry) => registry.id === sourceId);
  const targetIndex = ordered.findIndex((registry) => registry.id === targetId);
  if (sourceIndex < 0 || targetIndex < 0) return;
  const [moved] = ordered.splice(sourceIndex, 1);
  ordered.splice(targetIndex, 0, moved);
  applyRegistryOrder(ordered);
}

function formatBytes(bytes: number) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}`;
}

function shortDigest(digest: string) {
  return digest.length > 24 ? `${digest.slice(0, 19)}…${digest.slice(-7)}` : digest;
}

function formatDate(value: string | null) {
  if (!value) return t("common.unknown");
  return new Intl.DateTimeFormat(locale.value === "en" ? "en-US" : "zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
}

async function copyText(text: string, message: string) {
  await navigator.clipboard.writeText(text);
  notify(message);
}

const imageAddress = computed(() => {
  if (!selectedRegistry.value || !selectedRepository.value || !selectedTag.value) return "";
  const host = selectedRegistry.value.url.replace(/^https?:\/\//, "").replace(/\/$/, "");
  return `${host}/${selectedRepository.value}:${selectedTag.value}`;
});
</script>

<template>
  <div class="app-shell">
    <aside class="registry-rail">
      <div class="brand">
        <div class="brand-mark"><Boxes :size="20" /></div>
        <div><strong>Kaven</strong><span>Docker</span></div>
      </div>

      <div class="rail-heading">
        <span>{{ t("nav.registries") }}</span>
        <button class="icon-button rail-add" :title="t('nav.addRegistry')" @click="openNewRegistry"><Plus :size="16" /></button>
      </div>

      <div class="registry-list">
        <div
          v-for="(registry, index) in registries"
          :key="registry.id"
          class="registry-item"
          :class="{ active: registry.id === selectedRegistryId, dragging: registry.id === draggedRegistryId }"
          draggable="true"
          role="button"
          tabindex="0"
          @click="selectedRegistryId = registry.id"
          @keydown.enter="selectedRegistryId = registry.id"
          @keydown.space.prevent="selectedRegistryId = registry.id"
          @dragstart="draggedRegistryId = registry.id"
          @dragend="draggedRegistryId = ''"
          @dragover.prevent
          @drop.prevent="dropRegistry(registry.id)"
        >
          <span class="registry-glyph"><GripVertical class="registry-grip" :size="13" /><Server :size="16" /></span>
          <span class="registry-copy"><strong>{{ registry.name }}</strong><small>{{ registry.url.replace(/^https?:\/\//, '') }}</small></span>
          <span class="registry-item-end">
            <span v-if="registry.id === selectedRegistryId" class="status-dot" :class="{ error: connectionQuery.isError.value }"></span>
            <span class="registry-order-actions">
              <button type="button" :title="t('nav.moveUp')" :disabled="index === 0 || reorderRegistriesMutation.isPending.value" @click.stop="moveRegistry(registry.id, -1)"><ArrowUp :size="12" /></button>
              <button type="button" :title="t('nav.moveDown')" :disabled="index === registries.length - 1 || reorderRegistriesMutation.isPending.value" @click.stop="moveRegistry(registry.id, 1)"><ArrowDown :size="12" /></button>
            </span>
          </span>
        </div>
        <button v-if="!registries.length && !registriesQuery.isLoading.value" class="empty-add" @click="openNewRegistry">
          <Plus :size="18" /><span>{{ t("nav.addFirst") }}</span>
        </button>
      </div>

      <div class="rail-footer">
        <div class="language-switch" :title="t('nav.language')"><button :class="{ active: locale === 'zh-CN' }" @click="setLocale('zh-CN')">中</button><button :class="{ active: locale === 'en' }" @click="setLocale('en')">EN</button></div>
        <div class="rail-footer-actions"><button class="icon-button dark" :title="t('nav.globalSettings')" @click="openGlobalSettings"><Network :size="17" /></button><button v-if="selectedRegistry" class="icon-button dark" :title="t('nav.settings')" @click="openEditRegistry(selectedRegistry)"><Settings2 :size="17" /></button></div>
      </div>
    </aside>

    <main class="workspace">
      <header class="topbar">
        <div class="breadcrumb">
          <span>{{ selectedRegistry?.name ?? "Registry" }}</span>
          <template v-if="selectedRepository"><ChevronRight :size="15" /><strong>{{ selectedRepository }}</strong></template>
          <template v-if="selectedTag"><ChevronRight :size="15" /><span>{{ selectedTag }}</span></template>
        </div>
        <div class="top-actions">
          <div v-if="selectedRegistry" class="connection-state" :class="{ failed: connectionQuery.isError.value }">
            <LoaderCircle v-if="connectionQuery.isFetching.value" :size="14" class="spin" />
            <ShieldCheck v-else-if="connectionQuery.isSuccess.value" :size="14" />
            <CircleAlert v-else :size="14" />
            <span>{{ connectionQuery.isFetching.value ? t("connection.checking") : connectionQuery.isSuccess.value ? t("connection.connected") : t("connection.failed") }}</span>
          </div>
          <button class="icon-button" :title="t('common.retry')" :disabled="!selectedRegistry" @click="refreshAll"><RefreshCw :size="17" /></button>
          <button class="secondary-button" :title="privateSyncTargets.length ? t('sync.action') : t('sync.noPrivateTarget')" :disabled="!registries.length || !privateSyncTargets.length" @click="openSync"><CloudUpload :size="16" />{{ t("sync.action") }}</button>
          <button class="primary-button" @click="openNewRegistry"><Plus :size="16" />{{ t("nav.addRegistry") }}</button>
        </div>
      </header>

      <section v-if="!registries.length && !registriesQuery.isLoading.value" class="welcome-state">
        <div class="welcome-visual">
          <div class="stack-box back"></div><div class="stack-box middle"></div>
          <div class="stack-box front"><Package :size="38" /></div>
        </div>
        <p class="eyebrow">{{ t("welcome.eyebrow") }}</p>
        <h1>{{ t("welcome.title") }}</h1>
        <p>{{ t("welcome.body") }}</p>
        <button class="primary-button large" @click="openNewRegistry"><Plus :size="18" />{{ t("nav.addRegistry") }}</button>
      </section>

      <section v-else class="content-grid">
        <div class="list-panel repositories-panel">
          <div class="panel-title"><div><span class="eyebrow">{{ t("repositories.eyebrow") }}</span><h2>{{ t("repositories.title") }}</h2></div><div class="panel-title-actions"><span class="count">{{ repositories.length }}</span><button v-if="canRemoveSelectedRepository" class="icon-button compact repository-remove" :title="t('repositories.removeHint')" :disabled="removeRepositoryMutation.isPending.value" @click="removeRepositoryMutation.mutate(selectedRepository)"><LoaderCircle v-if="removeRepositoryMutation.isPending.value" class="spin" :size="14" /><Trash2 v-else :size="14" /></button><button class="icon-button compact" :title="t('repositories.add')" @click="openAddRepository"><Plus :size="15" /></button></div></div>
          <label class="search-box"><Search :size="15" /><input v-model="repositorySearch" :placeholder="t('repositories.search')" /></label>
          <div class="scroll-list">
            <div v-if="repositoriesQuery.isLoading.value" class="loading-block"><LoaderCircle class="spin" :size="20" />{{ t("repositories.loading") }}</div>
            <div v-else-if="repositoriesQuery.isError.value" class="error-block"><CircleAlert :size="20" /><strong>{{ t("repositories.error") }}</strong><span>{{ errorText(repositoriesQuery.error.value) }}</span><button @click="repositoriesQuery.refetch()">{{ t("common.retry") }}</button></div>
            <button
              v-for="repository in repositories"
              v-else
              :key="repository"
              class="list-row"
              :class="{ active: repository === selectedRepository }"
              @click="selectedRepository = repository"
            >
              <span class="row-icon"><Folder :size="17" /></span><span class="row-main"><strong>{{ repository }}</strong><small>{{ t("common.repository") }}</small></span><ChevronRight :size="15" />
            </button>
            <div v-if="!repositoriesQuery.isLoading.value && !repositories.length" class="quiet-empty"><Database :size="25" /><span>{{ selectedRegistry?.provider === 'generic' ? t("repositories.empty") : t("repositories.publicEmpty") }}</span><button v-if="selectedRegistry?.provider !== 'generic'" class="small-action" @click="openAddRepository"><Plus :size="14" />{{ t("repositories.add") }}</button></div>
          </div>
        </div>

        <div class="list-panel tags-panel">
          <div class="panel-title"><div><span class="eyebrow">{{ t("tags.eyebrow") }}</span><h2>{{ t("tags.title") }}</h2></div><span class="count">{{ tags.length }}</span></div>
          <label class="search-box"><Search :size="15" /><input v-model="tagSearch" :placeholder="t('tags.search')" :disabled="!selectedRepository" /></label>
          <div class="scroll-list">
            <div v-if="tagsQuery.isLoading.value" class="loading-block"><LoaderCircle class="spin" :size="20" />{{ t("tags.loading") }}</div>
            <div v-else-if="tagsQuery.isError.value" class="error-block"><CircleAlert :size="20" /><strong>{{ t("tags.error") }}</strong><span>{{ errorText(tagsQuery.error.value) }}</span><button @click="tagsQuery.refetch()">{{ t("common.retry") }}</button></div>
            <button v-for="tagName in tags" v-else :key="tagName" class="list-row tag-row" :class="{ active: tagName === selectedTag }" @click="selectedTag = tagName">
              <span class="row-icon"><Tag :size="16" /></span><span class="row-main"><strong>{{ tagName }}</strong><small>{{ selectedRepository }}</small></span><ChevronRight :size="15" />
            </button>
            <div v-if="!selectedRepository" class="quiet-empty"><Tag :size="25" /><span>{{ t("tags.selectRepository") }}</span></div>
            <div v-else-if="!tagsQuery.isLoading.value && !tags.length" class="quiet-empty"><Tag :size="25" /><span>{{ t("tags.empty") }}</span></div>
          </div>
        </div>

        <article class="details-panel">
          <div v-if="detailsQuery.isLoading.value" class="details-loading"><LoaderCircle class="spin" :size="24" /><span>{{ t("details.loading") }}</span></div>
          <div v-else-if="detailsQuery.isError.value" class="details-error"><CircleAlert :size="26" /><h2>{{ t("details.error") }}</h2><p>{{ errorText(detailsQuery.error.value) }}</p><button class="secondary-button" @click="detailsQuery.refetch()"><RefreshCw :size="16" />{{ t("common.retry") }}</button></div>
          <template v-else-if="detailsQuery.data.value">
            <div class="details-head">
              <div class="image-title"><span class="image-icon"><Box :size="23" /></span><div><span class="eyebrow">{{ t("details.eyebrow") }}</span><h1>{{ selectedTag }}</h1><p>{{ selectedRepository }}</p></div></div>
              <button class="icon-button" :title="t('details.more')"><MoreHorizontal :size="18" /></button>
            </div>
            <div class="pull-command"><code>docker pull {{ imageAddress }}</code><button :title="t('details.copyPull')" @click="copyText(`docker pull ${imageAddress}`, t('toast.pullCopied'))"><Clipboard :size="15" /></button></div>
            <section class="detail-section reference-section">
              <div class="section-heading"><h3>{{ t("details.reference") }}</h3><span>IMAGE</span></div>
              <dl class="manifest-table reference-table">
                <div><dt>{{ t("details.imageName") }}</dt><dd><code>{{ selectedRepository }}</code><button :title="t('details.copyImageName')" @click="copyText(selectedRepository, t('toast.imageNameCopied'))"><Clipboard :size="14" /></button></dd></div>
                <div><dt>{{ t("details.tag") }}</dt><dd><code>{{ selectedTag }}</code><button :title="t('details.copyTag')" @click="copyText(selectedTag, t('toast.tagCopied'))"><Clipboard :size="14" /></button></dd></div>
                <div><dt>{{ t("details.fullReference") }}</dt><dd><code>{{ imageAddress }}</code><button :title="t('details.copyReference')" @click="copyText(imageAddress, t('toast.referenceCopied'))"><Clipboard :size="14" /></button></dd></div>
              </dl>
            </section>
            <div class="stat-grid">
              <div><span>{{ t("details.imageSize") }}</span><strong>{{ formatBytes(detailsQuery.data.value.size) }}</strong></div>
              <div><span>{{ t("details.platforms") }}</span><strong>{{ detailsQuery.data.value.platforms.length ? t("details.platformCount", { count: detailsQuery.data.value.platforms.length }) : `${detailsQuery.data.value.os ?? 'unknown'}/${detailsQuery.data.value.architecture ?? 'unknown'}` }}</strong></div>
              <div><span>{{ t("details.created") }}</span><strong>{{ formatDate(detailsQuery.data.value.created) }}</strong></div>
            </div>
            <section class="detail-section">
              <div class="section-heading"><h3>{{ t("details.manifest") }}</h3><span>{{ detailsQuery.data.value.mediaType.includes('index') || detailsQuery.data.value.mediaType.includes('list') ? 'INDEX' : 'IMAGE' }}</span></div>
              <dl class="manifest-table">
                <div><dt>{{ t("details.digest") }}</dt><dd><code>{{ shortDigest(detailsQuery.data.value.digest) }}</code><button :title="t('details.copyDigest')" @click="copyText(detailsQuery.data.value!.digest, t('toast.digestCopied'))"><Clipboard :size="14" /></button></dd></div>
                <div><dt>{{ t("details.mediaType") }}</dt><dd>{{ detailsQuery.data.value.mediaType }}</dd></div>
                <div><dt>{{ t("details.layers") }}</dt><dd>{{ detailsQuery.data.value.layers.length }}</dd></div>
              </dl>
            </section>
            <section v-if="detailsQuery.data.value.platforms.length" class="detail-section">
              <div class="section-heading"><h3>{{ t("details.platforms") }}</h3><span>{{ detailsQuery.data.value.platforms.length }}</span></div>
              <div class="platform-list">
                <div v-for="platform in detailsQuery.data.value.platforms" :key="platform.digest" class="platform-row">
                  <span class="platform-icon"><Layers3 :size="16" /></span><strong>{{ platform.os }}/{{ platform.architecture }}<template v-if="platform.variant">/{{ platform.variant }}</template></strong><code>{{ shortDigest(platform.digest) }}</code><span>{{ formatBytes(platform.size) }}</span>
                </div>
              </div>
            </section>
            <section v-if="Object.keys(detailsQuery.data.value.labels).length" class="detail-section">
              <div class="section-heading"><h3>{{ t("details.labels") }}</h3><span>{{ Object.keys(detailsQuery.data.value.labels).length }}</span></div>
              <dl class="label-list"><div v-for="(value, key) in detailsQuery.data.value.labels" :key="key"><dt>{{ key }}</dt><dd>{{ value }}</dd></div></dl>
            </section>
            <div class="danger-zone"><div><strong>{{ t("danger.title") }}</strong><span>{{ t("danger.hint") }}</span></div><button class="danger-button" @click="showDeleteDialog = true"><Trash2 :size="16" />{{ t("danger.action") }}</button></div>
          </template>
          <div v-else class="detail-placeholder"><Box :size="31" /><h2>{{ t("details.selectTitle") }}</h2><p>{{ t("details.selectBody") }}</p></div>
        </article>
      </section>
    </main>

    <div v-if="showSyncDialog" class="modal-backdrop" @mousedown.self="closeSyncDialog">
      <form class="modal sync-modal" :class="{ maximized: syncMaximized }" @submit.prevent="submitSync">
        <div class="modal-header"><div><span class="eyebrow">{{ t("sync.eyebrow") }}</span><h2>{{ t("sync.title") }}</h2></div><div class="modal-header-actions"><button type="button" class="icon-button" :title="syncMaximized ? t('syncWindow.restore') : t('syncWindow.maximize')" @click="syncMaximized = !syncMaximized"><Minimize2 v-if="syncMaximized" :size="18" /><Maximize2 v-else :size="18" /></button><button type="button" class="icon-button" :title="t('common.cancel')" @click="closeSyncDialog"><X :size="18" /></button></div></div>
        <template v-if="syncRunStatus === 'idle'">
        <div class="sync-route">
          <div>
            <span>{{ t("sync.source") }}</span>
            <select v-model="syncSourceRegistryId" @change="changeSyncSourceRegistry"><option v-for="registry in registries" :key="registry.id" :value="registry.id">{{ registry.name }}</option></select>
            <select v-if="syncMode === 'single'" v-model="syncSourceRepository" :disabled="syncRepositoriesQuery.isLoading.value || !syncRepositoriesQuery.data.value?.repositories.length" @change="changeSyncSourceRepository"><option value="" disabled>{{ syncRepositoriesQuery.isLoading.value ? t("repositories.loading") : t("sync.selectSourceRepository") }}</option><option v-for="repository in syncRepositoriesQuery.data.value?.repositories ?? []" :key="repository" :value="repository">{{ repository }}</option></select>
            <small v-else>{{ t("syncBatch.repositoryCount", { count: batchSyncRepositories.length }) }}</small>
          </div>
          <span class="sync-arrow"><ArrowRight :size="18" /></span>
          <div><span>{{ t("sync.target") }}</span><select v-model="syncTargetRegistryId"><option v-for="registry in privateSyncTargets" :key="registry.id" :value="registry.id">{{ registry.name }}</option></select><small>{{ t("sync.privateTargetsOnly") }}</small></div>
        </div>
        <div class="sync-body">
          <div class="segmented sync-mode"><button type="button" :class="{ active: syncMode === 'single' }" @click="syncMode = 'single'">{{ t("syncBatch.singleMode") }}</button><button type="button" :class="{ active: syncMode === 'batch' }" @click="syncMode = 'batch'">{{ t("syncBatch.batchMode") }}</button></div>
          <div class="sync-platform-config">
            <label class="field"><span>{{ t("sync.platform") }}</span><select v-model="syncPlatformMode"><option value="amd64">linux/amd64 · {{ t("sync.platformDefault") }}</option><option value="arm64">linux/arm64</option><option value="all">{{ t("sync.allPlatforms") }}</option><option value="custom">{{ t("sync.customPlatform") }}</option></select></label>
            <div v-if="syncPlatformMode === 'custom'" class="sync-platform-fields">
              <label class="field"><span>OS</span><input v-model="customSyncPlatform.os" required placeholder="linux" /></label>
              <label class="field"><span>{{ t("sync.architecture") }}</span><input v-model="customSyncPlatform.architecture" required placeholder="amd64" /></label>
              <label class="field"><span>Variant</span><input v-model="customSyncPlatform.variant" placeholder="v8" /></label>
            </div>
            <small>{{ syncPlatformMode === 'all' ? t("sync.allPlatformsHint") : t("sync.singlePlatformHint") }}</small>
          </div>
          <template v-if="syncMode === 'single'">
          <label class="field"><span>{{ t("sync.targetRepository") }}</span><input v-model="syncTargetRepository" placeholder="team/image" /></label>
          <div class="sync-tags-head"><div><strong>{{ t("sync.selectTags") }}</strong><span>{{ t("sync.selectedCount", { count: selectedSyncTags.length }) }}</span></div><button type="button" @click="toggleAllSyncTags">{{ syncTags.length && syncTags.every((tagName) => selectedSyncTags.includes(tagName)) ? t("sync.clearVisible") : t("sync.selectVisible") }}</button></div>
          <label class="search-box sync-search"><Search :size="15" /><input v-model="syncTagSearch" :placeholder="t('tags.search')" /></label>
          <div class="sync-tag-list">
            <div v-if="syncTagsQuery.isLoading.value" class="sync-empty"><LoaderCircle class="spin" :size="18" />{{ t("tags.loading") }}</div>
            <div v-else-if="syncTagsQuery.isError.value" class="sync-empty sync-error"><CircleAlert :size="18" />{{ errorText(syncTagsQuery.error.value) }}</div>
            <button v-for="tagName in syncTags" v-else :key="tagName" type="button" class="sync-tag-row" :class="{ selected: selectedSyncTags.includes(tagName) }" @click="toggleSyncTag(tagName)">
              <span class="tag-check"><Check v-if="selectedSyncTags.includes(tagName)" :size="13" /></span><Tag :size="15" /><strong>{{ tagName }}</strong>
            </button>
            <div v-if="!syncTagsQuery.isLoading.value && !syncTagsQuery.isError.value && !syncTags.length" class="sync-empty">{{ syncSourceRepository ? t("tags.empty") : t("sync.selectSourceRepository") }}</div>
          </div>
          </template>
          <div v-else class="batch-sync">
            <div class="batch-sync-toolbar"><label class="search-box"><Search :size="15" /><input v-model="batchRepositorySearch" :placeholder="t('repositories.search')" /></label><button type="button" @click="toggleAllBatchRepositories">{{ batchRepositories.length && batchRepositories.every((repository) => batchSyncRepositories.includes(repository)) ? t("syncBatch.clearVisibleRepositories") : t("syncBatch.selectVisibleRepositories") }}</button></div>
            <div class="batch-sync-grid">
              <div class="batch-repository-list">
                <div v-if="syncRepositoriesQuery.isLoading.value" class="sync-empty"><LoaderCircle class="spin" :size="18" />{{ t("repositories.loading") }}</div>
                <button v-for="repository in batchRepositories" v-else :key="repository" type="button" class="batch-repository-row" :class="{ selected: batchSyncRepositories.includes(repository), active: activeBatchRepository === repository }" @click="activateBatchRepository(repository)">
                  <span class="tag-check" @click.stop="toggleBatchRepository(repository)"><Check v-if="batchSyncRepositories.includes(repository)" :size="13" /></span>
                  <Folder :size="15" /><strong>{{ repository }}</strong>
                  <LoaderCircle v-if="batchTagLoading.includes(repository)" class="spin" :size="14" />
                  <span v-else>{{ batchSelectedTags[repository]?.length ?? 0 }}</span>
                </button>
              </div>
              <div class="batch-tag-panel">
                <template v-if="activeBatchRepository">
                  <label class="field"><span>{{ t("sync.targetRepository") }}</span><input v-model="batchTargetRepositories[activeBatchRepository]" :disabled="!batchSyncRepositories.includes(activeBatchRepository)" /></label>
                  <div class="sync-tags-head"><div><strong>{{ activeBatchRepository }}</strong><span>{{ t("sync.selectedCount", { count: batchSelectedTags[activeBatchRepository]?.length ?? 0 }) }}</span></div><button type="button" :disabled="!batchSyncRepositories.includes(activeBatchRepository)" @click="toggleAllBatchTags">{{ activeBatchTags.length && activeBatchTags.every((tag) => batchSelectedTags[activeBatchRepository]?.includes(tag)) ? t("sync.clearVisible") : t("sync.selectVisible") }}</button></div>
                  <label class="search-box sync-search"><Search :size="15" /><input v-model="syncTagSearch" :placeholder="t('tags.search')" /></label>
                  <div class="sync-tag-list batch-tags">
                    <div v-if="batchTagLoading.includes(activeBatchRepository)" class="sync-empty"><LoaderCircle class="spin" :size="18" />{{ t("tags.loading") }}</div>
                    <div v-else-if="batchTagErrors[activeBatchRepository]" class="sync-empty sync-error"><CircleAlert :size="18" />{{ batchTagErrors[activeBatchRepository] }}</div>
                    <button v-for="tagName in activeBatchTags" v-else :key="tagName" type="button" class="sync-tag-row" :class="{ selected: batchSelectedTags[activeBatchRepository]?.includes(tagName) }" :disabled="!batchSyncRepositories.includes(activeBatchRepository)" @click="toggleBatchTag(tagName)"><span class="tag-check"><Check v-if="batchSelectedTags[activeBatchRepository]?.includes(tagName)" :size="13" /></span><Tag :size="15" /><strong>{{ tagName }}</strong></button>
                    <div v-if="!batchTagLoading.includes(activeBatchRepository) && !batchTagErrors[activeBatchRepository] && !activeBatchTags.length" class="sync-empty">{{ t("tags.empty") }}</div>
                  </div>
                </template>
                <div v-else class="sync-empty">{{ t("syncBatch.selectRepository") }}</div>
              </div>
            </div>
          </div>
          <div class="sync-note"><CircleAlert :size="15" /><span>{{ t("sync.note") }}</span></div>
        </div>
        </template>
        <div v-else class="sync-progress-body">
          <div class="sync-progress-summary" :class="syncRunStatus">
            <LoaderCircle v-if="syncRunStatus === 'running'" class="spin" :size="20" />
            <Check v-else-if="syncRunStatus === 'success'" :size="20" />
            <CircleAlert v-else :size="20" />
            <div><strong>{{ syncCancelRequested ? t("sync.status.stopping") : (syncMode === 'batch' && syncResult && batchFailedRepositories.length ? t("syncBatch.partial") : t(`sync.status.${syncRunStatus}`)) }}</strong><span>{{ syncFailure?.detail ? errorText(syncFailure.detail) : (syncResult ? t("sync.resultSummary", { tags: syncResult.syncedTags, copied: syncResult.copiedBlobs, skipped: syncResult.skippedBlobs, cached: syncResult.cachedBlobs, bytes: formatBytes(syncResult.transferredBytes) }) : t("sync.progressHint")) }}</span></div>
          </div>
          <section v-if="syncMode === 'batch' && batchSyncProgress" class="batch-overall-progress">
            <div class="batch-overall-heading">
              <div><span>{{ t("syncBatch.currentImage") }}</span><strong>{{ batchSyncProgress.currentRepository || t("syncBatch.complete") }}</strong></div>
              <b>{{ batchSyncProgress.current }}/{{ batchSyncProgress.total }}</b>
            </div>
            <div class="batch-overall-track"><span :style="{ width: `${batchSyncProgressPercent}%` }"></span></div>
            <div class="batch-overall-stats">
              <div><span>{{ t("syncBatch.total") }}</span><strong>{{ batchSyncProgress.total }}</strong></div>
              <div class="success"><span>{{ t("syncBatch.succeeded") }}</span><strong>{{ batchSyncProgress.succeeded }}</strong></div>
              <div :class="{ failed: batchSyncProgress.failed }"><span>{{ t("syncBatch.failed") }}</span><strong>{{ batchSyncProgress.failed }}</strong></div>
              <div><span>{{ t("syncBatch.remaining") }}</span><strong>{{ batchSyncProgress.remaining }}</strong></div>
            </div>
            <div v-if="batchFailedRepositories.length" class="batch-failed-repositories">
              <span>{{ t("syncBatch.failedImages") }}</span>
              <div><code v-for="repository in batchFailedRepositories" :key="repository">{{ repository }}</code></div>
            </div>
          </section>
          <div class="sync-stage-list">
            <div v-for="(stage, index) in syncStages" :key="stage" class="sync-stage" :class="syncProgressByStage[stage]?.status ?? 'pending'">
              <span class="sync-stage-icon"><Check v-if="syncProgressByStage[stage]?.status === 'complete'" :size="13" /><CircleAlert v-else-if="syncProgressByStage[stage]?.status === 'error'" :size="13" /><LoaderCircle v-else-if="syncProgressByStage[stage]?.status === 'active'" class="spin" :size="13" /><span v-else>{{ index + 1 }}</span></span>
              <div class="sync-stage-content">
                <div><strong>{{ t(`sync.stages.${stage}`) }}</strong><span v-if="syncProgressByStage[stage]?.total">{{ syncProgressByStage[stage]?.current }}/{{ syncProgressByStage[stage]?.total }}</span></div>
                <div class="sync-progress-track"><span :style="{ width: `${syncProgressPercent(stage)}%` }"></span></div>
                <small>{{ syncProgressByStage[stage]?.message ?? t("sync.waiting") }}</small>
                <small v-if="stage === 'blobs' && syncProgressByStage[stage]?.bytesTotal" class="sync-byte-progress">{{ formatBytes(syncProgressByStage[stage]?.bytesCurrent ?? 0) }} / {{ formatBytes(syncProgressByStage[stage]?.bytesTotal ?? 0) }}</small>
              </div>
            </div>
          </div>
          <div class="sync-log-section">
            <div class="sync-log-heading"><div><strong>{{ t("sync.logs") }}</strong><span>{{ syncLogs.length }}</span></div><button type="button" class="icon-button compact" :disabled="!syncLogs.length" :title="t('sync.copyLogs')" @click="copySyncLogs"><Clipboard :size="14" /></button></div>
            <div ref="syncLogView" class="sync-log-view">
              <div v-for="(entry, index) in syncLogs" :key="`${entry.timestamp}-${index}`" :class="entry.status"><time>{{ new Date(entry.timestamp).toLocaleTimeString() }}</time><span>{{ t(`sync.stages.${entry.stage}`) }}</span><code>{{ entry.message }}</code><small v-if="entry.detail">{{ entry.detail }}</small></div>
            </div>
            <div v-if="syncLogPath" class="sync-log-path"><span>{{ t("sync.logFile") }}</span><code>{{ syncLogPath }}</code></div>
          </div>
        </div>
        <div class="modal-footer"><span class="sync-summary">{{ syncCancelRequested ? t("sync.status.stopping") : (syncRunStatus === 'idle' ? (syncMode === 'batch' ? t("syncBatch.ready", { repositories: batchSyncRepositories.length, tags: batchSelectedTagCount }) : t("sync.ready", { count: selectedSyncTags.length })) : (syncMode === 'batch' && syncResult && batchFailedRepositories.length ? t("syncBatch.partial") : t(`sync.status.${syncRunStatus}`))) }}</span><div><button v-if="syncRunStatus === 'error' || syncRunStatus === 'success' || syncRunStatus === 'cancelled'" type="button" class="secondary-button" @click="editSyncSelection">{{ t("sync.back") }}</button><button type="button" class="secondary-button" @click="closeSyncDialog">{{ syncRunStatus === 'idle' ? t("common.cancel") : t("sync.close") }}</button><button v-if="syncMode === 'batch' && syncRunStatus === 'error' && batchFailedRepositories.length" type="button" class="primary-button" @click="retryFailedSync"><RefreshCw :size="16" />{{ t("syncBatch.retryFailed", { count: batchFailedRepositories.length }) }}</button><button v-else-if="syncRunStatus === 'idle'" class="primary-button"><CloudUpload :size="16" />{{ t("sync.start") }}</button><button v-else-if="syncRunStatus === 'running'" type="button" class="danger-button" :disabled="syncCancelRequested" @click="requestSyncCancellation"><LoaderCircle v-if="syncCancelRequested" class="spin" :size="16" /><X v-else :size="16" />{{ syncCancelRequested ? t("sync.stopping") : t("sync.stop") }}</button></div></div>
      </form>
    </div>

    <div v-if="showRegistryDialog" class="modal-backdrop" @mousedown.self="showRegistryDialog = false">
      <form class="modal" @submit.prevent="submitRegistry">
        <div class="modal-header"><div><span class="eyebrow">{{ t("form.eyebrow") }}</span><h2>{{ editingRegistryId ? t("form.editTitle") : t("form.addTitle") }}</h2></div><button type="button" class="icon-button" :title="t('common.cancel')" @click="showRegistryDialog = false"><X :size="18" /></button></div>
        <div v-if="!editingRegistryId" class="preset-section">
          <div class="preset-heading"><strong>{{ t("presets.title") }}</strong><span>{{ t("presets.subtitle") }}</span></div>
          <div class="preset-grid">
            <button v-for="preset in registryPresets" :key="preset.provider" type="button" class="preset-card" :disabled="addPresetMutation.isPending.value" @click="addPresetMutation.mutate(preset)">
              <span class="preset-mark" :data-provider="preset.provider">{{ preset.mark }}</span>
              <span><strong>{{ preset.name }}</strong><small>{{ preset.url.replace('https://', '') }}</small></span>
              <LoaderCircle v-if="addPresetMutation.isPending.value && addPresetMutation.variables.value?.provider === preset.provider" class="spin" :size="16" />
              <Plus v-else :size="16" />
            </button>
          </div>
          <div class="preset-divider"><span>{{ t("presets.custom") }}</span></div>
        </div>
        <div class="form-grid">
          <label class="field"><span>{{ t("form.displayName") }}</span><input v-model="form.name" autofocus placeholder="Production Registry" /></label>
          <label class="field"><span>{{ t("form.address") }}</span><input v-model="form.url" placeholder="https://registry.example.com" /></label>
          <div class="field"><span>{{ t("form.auth") }}</span><div class="segmented"><button type="button" :class="{ active: form.authType === 'anonymous' }" @click="form.authType = 'anonymous'">{{ t("form.anonymous") }}</button><button type="button" :class="{ active: form.authType === 'basic' }" @click="form.authType = 'basic'">{{ t("form.basic") }}</button></div></div>
          <div v-if="form.authType === 'basic'" class="credentials-row">
            <label class="field"><span>{{ t("form.username") }}</span><input v-model="form.username" autocomplete="username" placeholder="registry-user" /></label>
            <label class="field"><span>{{ t("form.password") }}</span><input v-model="form.password" type="password" autocomplete="current-password" :placeholder="editingRegistryId && selectedRegistry?.hasPassword ? t('form.keepPassword') : t('form.enterPassword')" /></label>
          </div>
          <label class="field"><span>{{ t("form.caPath") }}</span><input v-model="form.caCertPath" placeholder="C:\certs\registry-ca.pem" /></label>
          <div class="options-row">
            <label class="check-row"><input v-model="form.allowHttp" type="checkbox" /><span><strong>{{ t("form.allowHttp") }}</strong><small>{{ t("form.allowHttpHint") }}</small></span></label>
            <label class="check-row"><input v-model="form.skipTlsVerify" type="checkbox" /><span><strong>{{ t("form.skipTls") }}</strong><small>{{ t("form.skipTlsHint") }}</small></span></label>
          </div>
          <div class="field"><span>{{ t("proxy.registryMode") }}</span><select v-model="registryProxyChoice"><option value="global">{{ t("proxy.followGlobal") }}</option><option value="direct">{{ t("proxy.direct") }}</option><optgroup v-if="proxyProfiles.length" :label="t('proxy.savedProfiles')"><option v-for="proxy in proxyProfiles" :key="proxy.id" :value="`profile:${proxy.id}`">{{ proxy.name }}</option></optgroup><option v-if="form.proxyMode === 'custom'" value="custom">{{ t("proxy.legacyCustom") }}</option></select></div>
          <template v-if="form.proxyMode === 'custom'">
            <label class="field"><span>{{ t("proxy.url") }}</span><input v-model="form.proxyUrl" placeholder="http://127.0.0.1:7890" /></label>
            <div class="credentials-row">
              <label class="field"><span>{{ t("proxy.username") }}</span><input v-model="form.proxyUsername" autocomplete="username" /></label>
              <label class="field"><span>{{ t("proxy.password") }}</span><input v-model="form.proxyPassword" type="password" autocomplete="current-password" :placeholder="editingRegistryId && selectedRegistry?.proxyHasPassword ? t('form.keepPassword') : t('form.enterPassword')" /></label>
            </div>
          </template>
          <label class="field timeout-field"><span>{{ t("form.timeout") }}</span><div><input v-model.number="form.timeoutSecs" type="number" min="3" max="120" /><span>{{ t("form.seconds") }}</span></div></label>
          <label class="field timeout-field"><span>{{ t("form.uploadChunkSize") }}</span><div><input v-model.number="form.uploadChunkSizeMb" type="number" min="1" max="256" step="1" required /><span>MiB</span></div><small>{{ t("form.uploadChunkSizeHint") }}</small></label>
        </div>
        <div class="modal-footer">
          <button v-if="editingRegistryId" type="button" class="text-danger" @click="deleteRegistryMutation.mutate(editingRegistryId)"><Trash2 :size="15" />{{ t("form.remove") }}</button>
          <span v-else></span>
          <div><button type="button" class="secondary-button" @click="showRegistryDialog = false">{{ t("common.cancel") }}</button><button class="primary-button" :disabled="saveMutation.isPending.value"><LoaderCircle v-if="saveMutation.isPending.value" class="spin" :size="16" /><Download v-else :size="16" />{{ t("form.save") }}</button></div>
        </div>
      </form>
    </div>

    <div v-if="showGlobalSettingsDialog" class="modal-backdrop" @mousedown.self="showGlobalSettingsDialog = false">
      <div class="modal proxy-manager-modal">
        <div class="modal-header"><div><span class="eyebrow">{{ t("proxy.eyebrow") }}</span><h2>{{ t("proxy.globalTitle") }}</h2></div><button type="button" class="icon-button" :title="t('common.cancel')" @click="showGlobalSettingsDialog = false"><X :size="18" /></button></div>
        <div class="proxy-default-row"><label class="field"><span>{{ t("proxy.globalDefault") }}</span><select v-model="defaultProxyId"><option value="">{{ t("proxy.direct") }}</option><option v-for="proxy in proxyProfiles" :key="proxy.id" :value="proxy.id">{{ proxy.name }}</option></select></label><button class="secondary-button" :disabled="saveAppSettingsMutation.isPending.value" @click="submitGlobalSettings"><LoaderCircle v-if="saveAppSettingsMutation.isPending.value" class="spin" :size="15" /><Check v-else :size="15" />{{ t("proxy.applyDefault") }}</button></div>
        <div class="proxy-manager-body">
          <aside class="proxy-profile-list">
            <div class="proxy-list-heading"><strong>{{ t("proxy.profiles") }}</strong><button class="icon-button compact" :title="t('proxy.add')" @click="openNewProxy"><Plus :size="15" /></button></div>
            <button v-for="proxy in proxyProfiles" :key="proxy.id" class="proxy-list-item" :class="{ active: selectedProxyId === proxy.id }" @click="editProxy(proxy)"><span class="proxy-list-icon"><Network :size="15" /></span><span><strong>{{ proxy.name }}</strong><small>{{ proxySummary(proxy) }}</small></span></button>
            <div v-if="!proxyProfiles.length" class="proxy-list-empty">{{ t("proxy.empty") }}</div>
          </aside>
          <form class="proxy-editor" @submit.prevent="submitProxy">
            <div class="proxy-editor-heading"><div><span class="eyebrow">{{ selectedProxy ? t("proxy.editEyebrow") : t("proxy.newEyebrow") }}</span><h3>{{ selectedProxy?.name || t("proxy.newProfile") }}</h3></div><button v-if="selectedProxy" type="button" class="text-danger" :disabled="deleteProxyMutation.isPending.value" @click="deleteProxyMutation.mutate(selectedProxy.id)"><Trash2 :size="14" />{{ t("proxy.remove") }}</button></div>
            <div class="proxy-editor-fields">
              <label class="field"><span>{{ t("proxy.name") }}</span><input v-model="proxyForm.name" placeholder="Office Proxy" /></label>
              <div class="proxy-address-row">
                <label class="field"><span>{{ t("proxy.type") }}</span><select v-model="proxyForm.protocol"><option v-for="protocol in proxyProtocols" :key="protocol" :value="protocol">{{ protocol.toUpperCase() }}</option></select></label>
                <label class="field"><span>{{ t("proxy.hostname") }}</span><input v-model="proxyForm.hostname" placeholder="127.0.0.1" /></label>
                <label class="field"><span>{{ t("proxy.port") }}</span><input v-model.number="proxyForm.port" type="number" min="1" max="65535" inputmode="numeric" /></label>
              </div>
              <div class="credentials-row">
                <label class="field"><span>{{ t("proxy.username") }}</span><input v-model="proxyForm.username" :disabled="isSocks4Protocol" autocomplete="username" /></label>
                <label class="field"><span>{{ t("proxy.password") }}</span><input v-model="proxyForm.password" :disabled="isSocks4Protocol" type="password" autocomplete="current-password" :placeholder="selectedProxy?.hasPassword ? t('form.keepPassword') : t('form.enterPassword')" /></label>
              </div>
              <p class="field-hint">{{ t("proxy.credentialsHint") }}</p>
            </div>
            <div class="proxy-editor-footer"><button class="primary-button" :disabled="saveProxyMutation.isPending.value"><LoaderCircle v-if="saveProxyMutation.isPending.value" class="spin" :size="16" /><Download v-else :size="16" />{{ t("proxy.saveProfile") }}</button></div>
          </form>
        </div>
      </div>
    </div>

    <div v-if="showRepositoryDialog" class="modal-backdrop" @mousedown.self="showRepositoryDialog = false">
      <form class="modal repository-modal" @submit.prevent="submitRepository">
        <div class="modal-header"><div><span class="eyebrow">{{ t("repositories.eyebrow") }}</span><h2>{{ t("repositories.addTitle") }}</h2></div><button type="button" class="icon-button" :title="t('common.cancel')" @click="showRepositoryDialog = false"><X :size="18" /></button></div>
        <div class="form-grid">
          <div class="field"><span>{{ t("repositories.addMethod") }}</span><div class="segmented repository-mode"><button type="button" :class="{ active: repositoryAddMode === 'discover' }" @click="repositoryAddMode = 'discover'">{{ t("repositories.discoverMode") }}</button><button type="button" :class="{ active: repositoryAddMode === 'manual' }" @click="repositoryAddMode = 'manual'">{{ t("repositories.manualMode") }}</button></div></div>
          <template v-if="repositoryAddMode === 'discover'">
            <label class="field"><span>{{ t("repositories.namespace") }}</span><div class="repository-namespace-input"><input v-model="repositoryNamespace" autofocus :placeholder="repositoryExample.split('/')[0]" @input="scheduleRepositoryDiscovery" /><button type="button" :title="t('repositories.discover')" :disabled="!repositoryNamespace.trim() || !canDiscoverRepositories || discoverRepositoriesMutation.isPending.value" @click="discoverRepositoriesNow"><LoaderCircle v-if="discoverRepositoriesMutation.isPending.value" class="spin" :size="16" /><Search v-else :size="16" /></button></div></label>
            <div v-if="repositoryNamespace.trim() && !canDiscoverRepositories" class="repository-discovery-note"><CircleAlert :size="15" />{{ t("repositories.discoveryUnsupported") }}</div>
            <div v-else-if="discoverRepositoriesMutation.isError.value" class="repository-discovery-note error"><CircleAlert :size="15" />{{ errorText(discoverRepositoriesMutation.error.value) }}</div>
            <div v-else-if="discoverRepositoriesMutation.isSuccess.value" class="repository-discovery-note success"><Check :size="15" />{{ t("repositories.discovered", { count: discoveredRepositories.length }) }}</div>
            <label class="field"><span>{{ t("repositories.additionalNames") }}</span><textarea v-model="repositoryInput" rows="3" :placeholder="t('repositories.namesPlaceholder')"></textarea></label>
            <p class="field-hint">{{ t("repositories.addHint", { example: repositoryExample }) }}</p>
          </template>
          <template v-else>
            <label class="field"><span>{{ t("repositories.fullNames") }}</span><textarea v-model="repositoryInput" autofocus rows="4" :placeholder="repositoryExample"></textarea></label>
            <p class="field-hint">{{ t("repositories.manualHint", { example: repositoryExample }) }}</p>
          </template>
          <div v-if="repositoryCandidates.length" class="repository-picker">
            <div class="repository-picker-heading"><strong>{{ t("repositories.preview") }}</strong><div><span>{{ t("repositories.selected", { count: selectedRepositoryCandidates.length }) }}</span><button type="button" @click="toggleAllRepositoryCandidates">{{ allRepositoryCandidatesSelected ? t("repositories.deselectAll") : t("repositories.selectAll") }}</button></div></div>
            <button v-for="repository in repositoryCandidates" :key="repository" type="button" class="repository-choice" :class="{ selected: selectedRepositoryCandidates.includes(repository) }" @click="toggleRepositoryCandidate(repository)">
              <span class="tag-check"><Check v-if="selectedRepositoryCandidates.includes(repository)" :size="13" /></span><Folder :size="15" /><strong>{{ repository }}</strong>
            </button>
          </div>
        </div>
        <div class="modal-footer"><span></span><div><button type="button" class="secondary-button" @click="showRepositoryDialog = false">{{ t("common.cancel") }}</button><button class="primary-button" :disabled="addRepositoryMutation.isPending.value || !selectedRepositoryCandidates.length"><LoaderCircle v-if="addRepositoryMutation.isPending.value" class="spin" :size="16" /><Plus v-else :size="16" />{{ t("repositories.saveCount", { count: selectedRepositoryCandidates.length }) }}</button></div></div>
      </form>
    </div>

    <div v-if="showDeleteDialog" class="modal-backdrop" @mousedown.self="showDeleteDialog = false">
      <div class="confirm-modal">
        <div class="danger-icon"><Trash2 :size="23" /></div><h2>{{ t("confirm.title", { tag: selectedTag }) }}</h2>
        <p>{{ t("confirm.bodyBefore") }} <strong>{{ selectedRepository }}:{{ selectedTag }}</strong>{{ t("confirm.bodyAfter") }}</p>
        <div class="confirm-actions"><button class="secondary-button" @click="showDeleteDialog = false">{{ t("common.cancel") }}</button><button class="danger-button solid" :disabled="deleteManifestMutation.isPending.value" @click="deleteManifestMutation.mutate()"><LoaderCircle v-if="deleteManifestMutation.isPending.value" class="spin" :size="16" /><Trash2 v-else :size="16" />{{ t("confirm.action") }}</button></div>
      </div>
    </div>

    <transition name="toast"><div v-if="toast" class="toast" :class="toast.tone"><ShieldCheck v-if="toast.tone === 'success'" :size="17" /><CircleAlert v-else :size="17" />{{ toast.message }}</div></transition>
  </div>
</template>
