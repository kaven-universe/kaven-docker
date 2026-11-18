import assert from "node:assert/strict";
import test from "node:test";
import { evaluateAudit } from "./audit-npm.mjs";

const acceptance = {
  id: "GHSA-accepted",
  url: "https://example.test/GHSA-accepted",
  package: "leaf",
  expiresOn: "2027-02-17",
};

test("accepts only the registered advisory and its transitive dependants", () => {
  const result = evaluateAudit(
    {
      leaf: { severity: "high", via: [{ url: acceptance.url }] },
      parent: { severity: "high", via: ["leaf"] },
    },
    [acceptance],
    "2026-11-18",
  );
  assert.deepEqual(result.unaccepted, []);
  assert.deepEqual(result.stale, []);
  assert.deepEqual(result.observed, [acceptance]);
});

test("rejects an additional high-severity advisory", () => {
  const result = evaluateAudit(
    {
      leaf: { severity: "high", via: [{ url: acceptance.url }] },
      unexpected: { severity: "critical", via: [{ url: "https://example.test/unexpected" }] },
    },
    [acceptance],
    "2026-11-18",
  );
  assert.deepEqual(result.unaccepted, ["unexpected"]);
});

test("rejects expired and stale acceptances", () => {
  assert.throws(
    () => evaluateAudit({}, [{ ...acceptance, expiresOn: "2026-11-17" }], "2026-11-18"),
    /expired/,
  );
  const result = evaluateAudit({}, [acceptance], "2026-11-18");
  assert.deepEqual(result.stale, [acceptance]);
});
