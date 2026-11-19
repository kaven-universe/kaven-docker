import { afterEach, describe, expect, it, vi } from "vitest";
import { updateService } from "./update";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
  Channel: class MockChannel<T> { onmessage?: (event: T) => void },
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("signed updates", () => {
  it("checks the configured HTTPS manifest through the backend", async () => {
    invokeMock.mockResolvedValue({ currentVersion: "1.0.0", latestVersion: "1.2.3", updateAvailable: true, downloadUrl: "https://example.test/setup.exe" });

    await expect(updateService.check()).resolves.toMatchObject({ latestVersion: "1.2.3", updateAvailable: true });
    expect(invokeMock).toHaveBeenCalledWith("check_for_updates", { automatic: false });
  });

  it("marks background checks so the backend can apply its schedule", async () => {
    invokeMock.mockResolvedValue(null);

    await expect(updateService.check(true)).resolves.toBeNull();
    expect(invokeMock).toHaveBeenCalledWith("check_for_updates", { automatic: true });
  });

  it("streams signed updater progress through the install command", async () => {
    const onEvent = vi.fn();
    invokeMock.mockImplementation(async (command, invokeArguments) => {
      expect(command).toBe("install_update");
      expect(invokeArguments.expectedVersion).toBe("1.2.3");
      invokeArguments.onEvent.onmessage({ event: "progress", data: { chunkLength: 512 } });
    });

    await updateService.install("1.2.3", onEvent);

    expect(onEvent).toHaveBeenCalledWith({ event: "progress", data: { chunkLength: 512 } });
  });
});
