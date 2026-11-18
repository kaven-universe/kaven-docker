import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

export function evaluateAudit(vulnerabilities, acceptances, today) {
  const acceptedUrls = new Map();
  for (const acceptance of acceptances) {
    if (
      typeof acceptance.id !== "string" ||
      typeof acceptance.url !== "string" ||
      typeof acceptance.package !== "string" ||
      typeof acceptance.expiresOn !== "string"
    ) {
      throw new Error("Invalid npm dependency risk acceptance");
    }
    if (acceptance.expiresOn < today) {
      throw new Error(
        `Dependency risk acceptance ${acceptance.id} expired on ${acceptance.expiresOn}`,
      );
    }
    acceptedUrls.set(acceptance.url, acceptance);
  }

  const memo = new Map();
  function isAcceptedPackage(name, stack = new Set()) {
    if (memo.has(name)) return memo.get(name);
    if (stack.has(name)) return false;
    const vulnerability = vulnerabilities[name];
    if (!vulnerability || !["high", "critical"].includes(vulnerability.severity)) return true;

    const nextStack = new Set(stack).add(name);
    const accepted =
      vulnerability.via.length > 0 &&
      vulnerability.via.every((via) => {
        if (typeof via === "string") return isAcceptedPackage(via, nextStack);
        const acceptance = acceptedUrls.get(via.url);
        return acceptance?.package === name;
      });
    memo.set(name, accepted);
    return accepted;
  }

  const unaccepted = Object.keys(vulnerabilities).filter((name) => !isAcceptedPackage(name));
  const observedAcceptedUrls = new Set(
    Object.values(vulnerabilities).flatMap((vulnerability) =>
      vulnerability.via
        .filter((via) => typeof via !== "string" && acceptedUrls.has(via.url))
        .map((via) => via.url),
    ),
  );
  const stale = [...acceptedUrls.entries()]
    .filter(([url]) => !observedAcceptedUrls.has(url))
    .map(([, acceptance]) => acceptance);
  return { unaccepted, stale, observed: [...acceptedUrls.values()].filter((a) => !stale.includes(a)) };
}

function main() {
  const riskRegister = JSON.parse(
    readFileSync(new URL("../docs/dependency-risk-acceptances.json", import.meta.url), "utf8"),
  );
  if (riskRegister.schemaVersion !== 1 || !Array.isArray(riskRegister.npm)) {
    throw new Error("Unsupported dependency risk acceptance schema");
  }

  const npmCommand = process.platform === "win32" ? (process.env.ComSpec ?? "cmd.exe") : "npm";
  const npmArguments =
    process.platform === "win32" ? ["/d", "/s", "/c", "npm audit --json"] : ["audit", "--json"];
  const audit = spawnSync(npmCommand, npmArguments, { encoding: "utf8", shell: false });
  if (audit.error) throw audit.error;

  let report;
  try {
    report = JSON.parse(audit.stdout);
  } catch {
    process.stderr.write(audit.stderr || audit.stdout);
    throw new Error("npm audit did not return valid JSON");
  }

  const result = evaluateAudit(
    report.vulnerabilities ?? {},
    riskRegister.npm,
    new Date().toISOString().slice(0, 10),
  );
  if (result.unaccepted.length > 0) {
    process.stderr.write(audit.stdout);
    process.stderr.write(
      `\nUnaccepted high-severity dependency findings: ${result.unaccepted.join(", ")}\n`,
    );
    process.exitCode = 1;
    return;
  }
  if (result.stale.length > 0) {
    process.stderr.write(
      `Registered npm risks are no longer reported: ${result.stale.map((a) => a.id).join(", ")}\n`,
    );
    process.exitCode = 1;
    return;
  }
  for (const acceptance of result.observed) {
    process.stdout.write(
      `Accepted ${acceptance.id} for ${acceptance.package} through ${acceptance.expiresOn}.\n`,
    );
  }
  process.stdout.write("No unaccepted high-severity npm dependency findings.\n");
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main();
}
