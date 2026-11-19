# IWS 转账 Rule 的真实候选构建原件

本目录保存 `m1.rule.transfer` 的 WASM、metadata 与 receipt4 三件原件，供 Linux 的真实 governed GEN、双端接受、Host 驻留及 Cold/Warm 续跑用例使用。本轮替换原件来自 [WasmDeterminism run 44441003810](https://github.com/nil-studio/oasis7/actions/runs/44441003810) 的 attempt 1，[M1 producer job 135281723579](https://github.com/nil-studio/oasis7/actions/runs/44441003810/job/135281723579) artifact 为 `14490409383`。

该次 run 的源码 head 为 `9cc9585b550967b3eaac5f34d67289476a5be671`，实际检出的 PR merge 为 `e50b39ad0d353c1f4c5a91f08ecb835a369e23fb`，两个父提交分别为 base `aa52b7f44b7d0041a254a1176f57a99c74ac23b1` 和上述 source head。源码 head 与实际 CI merge 的整棵 tree 均为 `ee8c4cffb615393b90c94f9ee9a23a844d42d12c`。

M1 producer 完成 10 个模块的真实构建后，在旧 candidate hash 清单校验处以 `canonical hash mismatch for current platform module_id=m1.rule.move` 失败。三组 producer 的失败证据 artifact 保留了同一次 run / attempt 的完整 37 个模块、111 件原件；该轮 workflow 为 failure，汇总 verify 被跳过。本目录三件 Rule 原件逐字节取自 M1 artifact，未修改 metadata 或 receipt，也未用旧原件补齐。

| 模块组 | 模块数 / 原件数 | Producer job | Artifact |
| --- | ---: | --- | --- |
| M1 | 10 / 30 | [135281723579](https://github.com/nil-studio/oasis7/actions/runs/44441003810/job/135281723579) | `14490409383` |
| M4 | 22 / 66 | [135281723557](https://github.com/nil-studio/oasis7/actions/runs/44441003810/job/135281723557) | `14491528400` |
| M5 | 5 / 15 | [135281723560](https://github.com/nil-studio/oasis7/actions/runs/44441003810/job/135281723560) | `14490963609` |

全部 ZIP 的 API 摘要、CRC、成员清单与解包原字节已独立复核。全套材料中的 244 条 source-file、22 条 input-declaration 和 851 条 recipe 记录均与上述固定源码相符，37 个 source hash 均按 SourceRule2 重算一致。

本地在上述 `9cc9585b` 固定源码上构建官方 `sync_builtin_wasm_identity`，使用 Rust 1.96.0、dev profile 及与 Rust CI 相同的 `CARGO_PROFILE_DEV_DEBUG=0`；优化与断言配置未改。二进制 SHA-256 为 `68c52df793842e7f9838052ec99edea22d43469eae75d7548c430a47f288614e`。在真实非 CI 环境中，以既有 `local-dev` 写入标志执行 `--candidate-baseline-prepare`，从同批 111 件原件生成完整七件 candidate 产物并原样复制到仓库。

随后对整合后的七件 candidate 产物、三件 Rule 原件及对应消费断言，使用同一官方二进制完成 M1/M4/M5 `--candidate-baseline-check`、`--verify-iws-rule-candidate-fixture`，以及 sensor、cargo、mobility、memory 四个 `--verify-builtin-test-artifact`，共八项均成功。四个默认 Runtime 模块的实际字节符合冻结 release 哈希。六件 release 文件、storage v0.2 身份及两处 IWS incomplete guard 均未改。这些本地结果覆盖材料来源与身份约束，不代表该轮失败 CI 已转绿，也不代替真实 Rule、Host、冷恢复或父项规模验收。

本 Rule 的 receipt 记录 6 项 source input 和 23 项 recipe input，均已逐项核对上述固定源码的原字节。构建使用 `nightly-2025-12-11`、`wasm32-unknown-unknown`、release profile、`std,panic_abort` 和 `strip-custom-sections-v1`；两份 JSON 的共同字段与实际 WASM 的 SHA-256、字节数一致。

| 本轮构建字段 | 原件中的值 |
| --- | --- |
| Source hash | `f90337002f387192cbfbaf4faf2c1c485b16645d1b5a9917e0eae87a51b1e9a2` |
| BuildManifest hash | `e63caf8a05c08c02e2909e7be4fa7d280d20d429afe40a69087e92a96747c2cd` |
| Producer recipe hash | `ac336cac21994d5e56c5c8ff150e8a7b3a1abd62ed7f95a422e7f3a8c658de6a` |
| Builder execution kind / platform | `local-docker-build` / `linux/amd64` |
| Builder image ref | `oasis7/wasm-builder:candidate-ac336cac21994d5e56c5c8ff150e8a7b3a1abd62ed7f95a422e7f3a8c658de6a` |
| Builder image ID | `sha256:16954e92bcabfb9020da9d5242332bbc86e958c17f87ae6449fbf9fc982cd8c4` |

| 原件 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `m1.rule.transfer.wasm` | 349764 | `2d3a20151d9b8f7766ecc89a8353d0de10bb65c877e10bb898fbbec616cda316` |
| `m1.rule.transfer.metadata.json` | 8005 | `7f611510195936b0a4be55ee80d946dd2d2172ec6984cab041b100f6db74cb95` |
| `m1.rule.transfer.build-receipt.json` | 7957 | `57017c1f3efc7b27c0f4d683a540db1b3b4f6d29a7594b8d73c9696ff6d05449` |

这些文件的 purpose 保持 `candidate-build`，不赋予 builtin release 资格。测试在自己的初始 World 中，通过真实提案、shadow、approve、apply 路径安装它；不会修改被冻结的 release 六文件、伪造 worker 或替换为 WAT stub。fixture 的专用签名只表达这个测试 World 的治理授权。

## 已替换原件的历史验证

上一版来自 [run 44394616960](https://github.com/nil-studio/oasis7/actions/runs/44394616960) attempt 1，M1 artifact 为 `14470301324`；源码 head 为 `ad6275211a4b5c06648f4f4706dfcf301a44a2df`，实际 CI merge 为 `0de727d08ee66ea83580043ef3ac13a87707efed`。当时三个 producer 均完成 canonical build/hash checks，随后以 `candidate source/build/identity freshness mismatch` 失败；完整 37 个模块、111 件原件保存在失败 artifact 中，workflow 为 failure，汇总 verify 被跳过。旧 Rule WASM 为 407277 字节，SHA-256 为 `4db769768e3bde644cc2e34d65ae61c8ab2776b678541cfe5ddd15b467e6a504`。

历史本地固定提交 `22caa8dc7d00dd77c0c68ddbcb74b9f9d4d70ac4` 与当时源码、CI merge 的 tree 均为 `e3a77ff675f9820f8e3d68029eabb424f76a5d5f`。在该源码构建的官方工具通过 `--candidate-baseline-prepare` 生成七件 candidate 产物，并在历史整合提交 `5ea5ea5d0c47fba87fd026b85466704212e21a7f` 完成 M1/M4/M5 candidate check 与 fixture verifier，四项成功；工具二进制 SHA-256 为 `de1bed22a1c37a1e9485bfc17659448b97f2e0d365d99ba8cc5363116bafde8e`。这些结果只覆盖已替换的旧原件，不是本轮材料的验证结果。

## 只读验证

从仓库根目录执行：

```bash
cargo run --locked -p oasis7_distfs --features sync-builtin-wasm-identity \
  --bin sync_builtin_wasm_identity -- --verify-iws-rule-candidate-fixture \
  "$PWD" "$PWD/crates/oasis7_chain_runtime/tests/fixtures/iws_rule_transfer_candidate"
```

该入口验证实际 WASM、两份 JSON 的一致性、当前 source-input closure、manifest、producer recipe、candidate descriptor 及其中登记的真实 builder execution；并检查 release 六文件冻结约束。Linux IWS CI 入口在运行 Host 用例前执行同一只读验证。缺件或与当前来源不符会失败，不会忽略用例或自动生成替代原件。

## 更新原件

在具备原 producer 所需 Docker 环境的工作区，从仓库根目录运行原单模块 producer：

```bash
scripts/build-builtin-wasm-modules.sh --candidate-build \
  --module-id m1.rule.transfer --out-dir "$PWD/.tmp/iws-rule-transfer-candidate"
```

三件原件必须作为同一次实际构建的整体更新；同时按原 candidate-baseline 流程更新并验证 descriptor。source closure 或 recipe 变化时，必须重新构建所有受影响模块，不能修改 receipt、哈希或执行身份来继续使用旧原件。当前目录只是其中一个固定输入，不取代全套 WASM 确定性检查，也不证明父 issue 的完整正常提交、冷恢复或规模验收。
