# IWS 转账 Rule 的真实候选构建原件

本目录保存 `m1.rule.transfer` 的 WASM、metadata 与 receipt4 三件原件，供 Linux 的真实 governed GEN、双端接受、Host 驻留及 Cold/Warm 续跑用例使用。它们来自 [WasmDeterminism run 44394616960](https://github.com/nil-studio/oasis7/actions/runs/44394616960) 的 attempt 1，[M1 producer job 135121330483](https://github.com/nil-studio/oasis7/actions/runs/44394616960/job/135121330483) artifact 为 `14470301324`。

该次 run 的源码 head 为 `ad6275211a4b5c06648f4f4706dfcf301a44a2df`，实际检出的 PR merge 为 `0de727d08ee66ea83580043ef3ac13a87707efed`，两个父提交分别为 base `8d42bab194538c6430ace3514e33076a3065fb9d` 和上述 source head。源码 head、实际 CI merge 与本地固定提交 `22caa8dc7d00dd77c0c68ddbcb74b9f9d4d70ac4` 的整棵 tree 均为 `e3a77ff675f9820f8e3d68029eabb424f76a5d5f`。同一 run 的三组完整原件用于 candidate baseline：

| 模块组 | 模块数 / 原件数 | Producer job | Artifact |
| --- | ---: | --- | --- |
| M1 | 10 / 30 | [135121330483](https://github.com/nil-studio/oasis7/actions/runs/44394616960/job/135121330483) | `14470301324` |
| M4 | 22 / 66 | [135121330222](https://github.com/nil-studio/oasis7/actions/runs/44394616960/job/135121330222) | `14470538214` |
| M5 | 5 / 15 | [135121330774](https://github.com/nil-studio/oasis7/actions/runs/44394616960/job/135121330774) | `14470066669` |

三个 producer 作业均打印 `candidate canonical build/hash checks complete`，随后以 `candidate source/build/identity freshness mismatch` 拒绝旧候选 identity。原失败证据上传保留了本轮完整 37 个模块、111 个文件，未用旧原件补齐；该轮 workflow 为 failure，汇总 verify 被跳过。配方为 `b9420503a04693c749342e3e272f95d78003bc402612da2cb9cc62d2c47f263c`，共享 BuildManifest 为 `a6bf2d6f4dbc36df1245a43ea0a21ab1f7514cb9e3ed2242444d76d99c4c0a2a`。本 fixture 的实际 M1 builder image 为 `sha256:af31afa608a8255643cd4fb5dab133e859bd9d9a39ebd83027f2cb3ffee2c2df`；其 builder execution 与 descriptor 中该 module 的原生成记录精确对应。全部 139 条 source 和 23 条 recipe 记录均逐项核对固定源码原字节，三组 canonical hash 清单及本 fixture 的 WASM 字节与上一版相同。

本地在上述 `22caa8dc` 固定源码上构建官方 `sync_builtin_wasm_identity` 工具，并以该二进制的 `--candidate-baseline-prepare` 入口生成完整七件 candidate 产物；三件 Rule 原件保持本轮 CI 的原字节。随后在本地整合提交 `5ea5ea5d0c47fba87fd026b85466704212e21a7f` 上，用同一官方二进制完成 M1/M4/M5 candidate check 和 fixture verifier，四项均成功。该二进制 SHA-256 为 `de1bed22a1c37a1e9485bfc17659448b97f2e0d365d99ba8cc5363116bafde8e`；其源文件及依赖输入相对 `22caa8dc` 未变。这些是本地官方入口的执行结果，CI 原件的生产来源仍是上述 `ad627521` / `0de727d0` 与 run。

| 原件 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `m1.rule.transfer.wasm` | 407277 | `4db769768e3bde644cc2e34d65ae61c8ab2776b678541cfe5ddd15b467e6a504` |
| `m1.rule.transfer.metadata.json` | 8005 | `c606bdf80785f9de8d1265f21dc932d43a10e1b7bc597842fbef8e5e5c5b1811` |
| `m1.rule.transfer.build-receipt.json` | 7957 | `769b468b30aa39a1bf820be50a5bd8734ff5bb823249d474c99f425760bbfe4e` |

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
