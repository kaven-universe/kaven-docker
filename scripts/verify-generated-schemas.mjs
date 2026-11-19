import { spawnSync } from "node:child_process";

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: process.cwd(),
    stdio: "inherit",
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

run("cargo", [
  "check",
  "--manifest-path",
  "src-tauri/Cargo.toml",
  "--all-targets",
  "--all-features",
  "--locked",
]);
run("git", ["diff", "--exit-code", "--", "src-tauri/gen/schemas"]);

const untracked = spawnSync(
  "git",
  ["ls-files", "--others", "--exclude-standard", "--", "src-tauri/gen/schemas"],
  { cwd: process.cwd(), encoding: "utf8" },
);
if (untracked.error) {
  throw untracked.error;
}
if (untracked.status !== 0) {
  process.exit(untracked.status ?? 1);
}
if (untracked.stdout.trim()) {
  console.error("Generated Tauri schemas contain untracked files:");
  console.error(untracked.stdout.trim());
  process.exit(1);
}
