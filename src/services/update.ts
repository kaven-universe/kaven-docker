import { Channel, invoke } from "@tauri-apps/api/core";

export interface UpdateInfo {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  downloadUrl: string;
}

export type UpdateInstallEvent =
  | { event: "checking" }
  | { event: "started"; data: { contentLength?: number } }
  | { event: "progress"; data: { chunkLength: number } }
  | { event: "finished" };

export const updateService = {
  check: async (automatic = false) => invoke<UpdateInfo | null>("check_for_updates", { automatic }),
  install: async (expectedVersion: string, onEvent: (event: UpdateInstallEvent) => void) => {
    const channel = new Channel<UpdateInstallEvent>();
    channel.onmessage = onEvent;
    await invoke("install_update", { expectedVersion, onEvent: channel });
  },
};
