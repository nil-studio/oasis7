import { expect, it } from "vitest";
import { parseBrowserGameConfig } from "./viewer_runtime_config_policy.js";
const config = () => ({ endpoint: "/api/public/hosted-account/v2", servicePublicKey: "ab".repeat(32), participantIssuerPublicKey: "cd".repeat(32), world: { world_id: "world", genesis_digest: "genesis" }, identity: { service_id: "service", environment_id: "test", instance_epoch: "18446744073709551615" } });
it("disables absent browser trust and preserves exact u64 namespace", () => {
  expect(parseBrowserGameConfig()).toBeNull();
  const parsed = parseBrowserGameConfig(config());
  expect(parsed.identity.instance_epoch).toBe("18446744073709551615");
  expect(Object.isFrozen(parsed.world)).toBe(true);
});
it("rejects arbitrary gateways, missing pins and invalid namespace epochs", () => {
  for (const mutate of [c => c.endpoint = "https://evil.example", c => c.servicePublicKey = "", c => c.participantIssuerPublicKey = "", c => c.world.world_id = "", c => c.identity.instance_epoch = "18446744073709551616", c => c.identity.instance_epoch = "01", c => c.identity.instance_epoch = 1]) {
    const value = config(); mutate(value); expect(() => parseBrowserGameConfig(value)).toThrow();
  }
});
