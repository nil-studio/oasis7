# IWS 转账 Rule 的真实候选构建原件

本目录保存 `m1.rule.transfer` 的 WASM、metadata 与 receipt4 三件原件，供 Linux 的真实 governed GEN、双端接受、Host 驻留及 Cold/Warm 续跑用例使用。它们来自 [WasmDeterminism run 44325546255](https://github.com/nil-studio/oasis7/actions/runs/44325546255) 的 attempt 1，[M1 producer job 134875946920](https://github.com/nil-studio/oasis7/actions/runs/44325546255/job/134875946920) artifact 为 `14437771904`。

该次 run 的源码 head 为 `a18cebea5dedbb0c09dfae296d517bbcbad33355`，实际检出的 PR merge 为 `596c69393c5fc5b563253d3b1d1ebbb2b96e6636`，tree 为 `46dc931ae11b44d5e5cb4b7b5462d1f8b0874f25`。该 merge 的两个父提交分别为 base `12494152135115b81e6d549bea40060797d907c0` 和上述 source head。同一 run 的 M1/M4/M5 共 37 组三件原件用于 candidate baseline；三个 artifact 分别为 `14437771904`、`14437659054`、`14436784461`。

三个 producer 作业均完成真实 canonical build/hash 检查，随后因旧候选 identity 与当前 producer 配方不一致退出；这些原件来自原失败证据上传，receipt 保持原字节。配方为 `0087d676e1cc7ba551ae9dd2979e7d928946ebc5d287decad9e9dc598e495165`，共享 BuildManifest 为 `ec5e5906eee671940f7af3fdbb9d57ad62bb6afc2296ab46fa2843c4cb8188e5`。本 fixture 的实际 M1 builder image 为 `sha256:726069b164cc168fa15852036c998e84ae67192860b7943ffb36f8c5faf11718`；其 builder execution 与 descriptor 中该 module 的原生成记录精确对应。WASM 字节与上一版相同。这些失败作业提供了真实原件，不表示该轮 Wasm gate 通过；当前工作树的适用性仍须由官方 prepare、三组 candidate check 和 fixture verifier 验证。

| 原件 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `m1.rule.transfer.wasm` | 407277 | `4db769768e3bde644cc2e34d65ae61c8ab2776b678541cfe5ddd15b467e6a504` |
| `m1.rule.transfer.metadata.json` | 8005 | `2d9bc32b41ca21374f3dec70e4536eca09cebc7b52930e51f0f6ff479102cc67` |
| `m1.rule.transfer.build-receipt.json` | 7957 | `74bb5819a440e5eedde678e42c35916f3cb546a3fb2f16d20fd4f444c95d02f8` |

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
