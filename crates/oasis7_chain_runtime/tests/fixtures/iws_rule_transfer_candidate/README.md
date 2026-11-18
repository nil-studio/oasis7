# IWS 转账 Rule 的真实候选构建原件

本目录保存 `m1.rule.transfer` 的 WASM、metadata 与 receipt4 三件原件，供 Linux 的真实 governed GEN → 双端接受 → Host 驻留集成用例使用。它们来自 [WasmDeterminism run 44249669224](https://github.com/nil-studio/oasis7/actions/runs/44249669224) 的 attempt 1，[M1 producer job 134617070263](https://github.com/nil-studio/oasis7/actions/runs/44249669224/job/134617070263) artifact 为 `14404046779`。

该次 run 的源码 head 为 `e41c26b64f7362475880aa8d52870301208b1746`，实际检出的 PR merge 为 `21f553fe6de09a58562985374660d0b1ff34aaaf`，tree 为 `afd40ec55482f5de071f6ae6d498913084e769ca`。同一 run 的 M1/M4/M5 共 37 组三件原件用于本分支的 candidate baseline；三个 artifact 分别为 `14404046779`、`14404118001`、`14404270570`。

三个 producer 作业均完成真实 canonical build/hash 检查，随后因原候选 identity 与新 producer 配方不一致退出；这些原件来自已有失败证据上传，未改写 receipt。新配方为 `76037477110038267f322d2fc40c39b05624ac0d0fe8a76cfdb5537270447427`，共享 BuildManifest 为 `2ae6babd15950354742fec8cdaad526c8d01532a9b40fae5689a6b71ccbe49df`。固定 fixture 的 builder execution 与新 descriptor 中同一个 M1 构建记录精确对应，WASM 字节与上一版相同。

| 原件 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `m1.rule.transfer.wasm` | 407277 | `4db769768e3bde644cc2e34d65ae61c8ab2776b678541cfe5ddd15b467e6a504` |
| `m1.rule.transfer.metadata.json` | 8005 | `5833f1c9050e915e7af808bb9932f68ed071d943484e431d8d95d3e0997871f2` |
| `m1.rule.transfer.build-receipt.json` | 7957 | `2ca78108f2803730dcbbc4ddf32fc841529c502427e7540e5f58654153ae14aa` |

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
