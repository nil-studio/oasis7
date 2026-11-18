# IWS 转账 Rule 的真实候选构建原件

本目录保存 `m1.rule.transfer` 的 WASM、metadata 与 receipt4 三件原件，供 Linux 的真实 governed GEN、双端接受、Host 驻留及 Cold/Warm 续跑用例使用。它们来自 [WasmDeterminism run 44362613486](https://github.com/nil-studio/oasis7/actions/runs/44362613486) 的 attempt 1，[M1 producer job 135008640667](https://github.com/nil-studio/oasis7/actions/runs/44362613486/job/135008640667) artifact 为 `14454774101`。

该次 run 的源码 head 为 `d7f69cdf224ddc44c058885324eeaffd0a92ab10`，实际检出的 PR merge 为 `a07e180bb5e5c9b3e2bbca13b071110d2f2e7ef4`，tree 为 `2bb901cffed3e163d58a61def2726030aa32cb50`。该 merge 的两个父提交分别为 base `957efb179306e3ffc2a55f9f6f1a1a76cb2f312c` 和上述 source head。同一 run 的 M1/M4/M5 共 37 组三件原件用于 candidate baseline；三个 artifact 分别为 `14454774101`、`14455569881`、`14455687405`。

三个 producer 作业均完成本组全部模块的真实 canonical 构建，随后在与旧候选 hash 清单比对时退出；这些原件来自原失败证据上传，receipt 保持原字节。配方为 `da9ee7a4c2b2db47a2811e108273ffd04b23c601bc9cfa122e87b315cedb6af6`，共享 BuildManifest 为 `aa2b0e8490efd00ad9366ec5f967b30aff88c3a2eb5f667984212c9d961b9355`。本 fixture 的实际 M1 builder image 为 `sha256:dc8bcdef362a18a91ec9967dc6ec63f98837ac70642de41db2960147fe7eab4f`；其 builder execution 与 descriptor 中该 module 的原生成记录精确对应。WASM 字节与上一版相同。该实际 merge 相对源码 head 的 12 条路径差异与记录的 source/recipe 输入无交集，全部 139 条 source 和 23 条 recipe 记录均逐项核对原字节。这些失败作业提供了真实原件，不表示该轮 Wasm gate 通过；当前工作树的适用性仍须由官方 prepare、三组 candidate check 和 fixture verifier 验证。

| 原件 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `m1.rule.transfer.wasm` | 407277 | `4db769768e3bde644cc2e34d65ae61c8ab2776b678541cfe5ddd15b467e6a504` |
| `m1.rule.transfer.metadata.json` | 8005 | `ae361e1002967381230b90e893bddf3e7fb8b0859f8625d2a20f59c922e73dea` |
| `m1.rule.transfer.build-receipt.json` | 7957 | `89a7d96781117a514d5c5c491957b4052f740a92049d73cace9dacb4d3a6d988` |

这些文件的 purpose 保持 `candidate-build`，不赋予 builtin release 资格。测试在自己的初始 World 中，通过真实提案、shadow、approve、apply 路径安装它；不会修改被冻结的 release 六文件、伪造 worker 或替换为 WAT stub。fixture 的专用签名只表达这个测试 World 的治理授权。

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
