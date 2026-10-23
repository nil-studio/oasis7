import { spawnSync } from "node:child_process";
const result = spawnSync(process.env.PYTHON || "python3", ["../protocol_codegen/generate.py", "check"], { stdio: "inherit" });
if (result.error) throw result.error;
process.exit(result.status ?? 1);
