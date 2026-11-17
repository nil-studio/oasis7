# IWS 转账 Rule 的真实候选构建原件

本目录保存 `m1.rule.transfer` 的 WASM、metadata 与 receipt4 三件原件，供 Linux 的真实 governed GEN → 双端接受 → Host 驻留集成用例使用。它们来自 [WasmDeterminism run 44045184107](https://github.com/nil-studio/oasis7/actions/runs/44045184107) 的 attempt 1，M1 producer artifact 为 `14311734547`。

该次 run 的源码 head 为 `f1c78ca9216805905a33ee5916575cd7ac21ccbc`，实际检出的 PR merge 为 `12200528ad3b53da6a0c139be682000c5a17dae3`，tree 为 `bc69e492b3ba2d98c60ec38ee2b4e85e5f50c40a`。同一 run 的 M1/M4/M5 共 37 组三件原件用于本分支的 candidate baseline；三个 artifact 分别为 `14311734547`、`14311194413`、`14311158959`。

| 原件 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `m1.rule.transfer.wasm` | 407277 | `4db769768e3bde644cc2e34d65ae61c8ab2776b678541cfe5ddd15b467e6a504` |
| `m1.rule.transfer.metadata.json` | 7811 | `7f291554ad156df5549ae0c4b31b11f88eab4c2ad1c72bf3b0c32d92dfda59d7` |
| `m1.rule.transfer.build-receipt.json` | 7763 | `ae55df21e27f100aca78162ae974162ff218712335fd433d6c60a83db6903f05` |

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
