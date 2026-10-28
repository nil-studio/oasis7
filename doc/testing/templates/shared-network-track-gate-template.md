# Legacy Shared-Network Rehearsal Track Gate 模板

审计轮次: 1

## 目的
- 为 `RTMIN-2` 提供 legacy network-rehearsal QA gate 模板。
- 固定历史 `public_testnet_rehearsal`、`staging`、`canary` 三层 rehearsal 轨道的最小 lane、`pass/partial/block` 判定与证据回写格式。
- 让 `qa_engineer`、`runtime_engineer`、`liveops_community` 在同一 candidate bundle 上给出一致结论。
- 本模板不得用于新的 active product/player-facing world claim；新测试环境应走 formal network tier / test environment candidate 证据链。

## 使用说明
- 先生成并校验同一份 `release_candidate_bundle`。
- 再按当前 track 填写对应 lanes TSV，并用 `./scripts/network-rehearsal-track-gate.sh` 生成 `summary.json/md`。
- 若缺失 required lane，脚本会直接把 gate 结论写成 `block`。
- 同一份 gate 只对应一个 legacy rehearsal `track + candidate_id`；不得混用不同候选或不同 runtime/governance 真值。

## 结论状态
| 状态 | 含义 |
| --- | --- |
| `pass` | required lanes 全部具备，且所有 lane 结论均为 `pass` |
| `partial` | required lanes 全部具备，但至少一条 lane 仍为 `partial` |
| `block` | 缺 required lane，或任一 lane 明确 `block` |

## Track Required Lanes
| Track | Required lanes |
| --- | --- |
| `public_testnet_rehearsal` | `candidate_bundle_integrity` / `shared_access` / `multi_entry_closure` / `mixed_topology_baseline` / `governance_live_drill` / `short_window_longrun` / `rollback_target_ready` |
| `staging` | `candidate_bundle_integrity` / `shared_access` / `unified_candidate_gate` / `mixed_topology_rehearsal` / `governance_live_drill` / `upgrade_rehearsal` / `rollback_rehearsal` / `incident_template` |
| `canary` | `candidate_bundle_integrity` / `promotion_record` / `canary_window` / `mixed_topology_claim_review` / `rollback_rehearsal` / `incident_review` / `exit_decision` |

## TSV 列格式
```tsv
# lane_id<TAB>owner<TAB>status<TAB>evidence_path<TAB>note
```

## 准备当前运行 TSV

按上表选择当前 track 的全部 required lane，依 TSV 列格式在本次证据输出目录新建文件，并填写 owner、真实 status、同一 candidate 的 evidence path 与 note。三条轨道分别为 7 / 8 / 7 条 required lane；不要把仓库存量示例中的状态和路径当作本次运行通过记录，也不要覆盖历史 TSV 原件。

先设置 `LANES_TSV` 为本次证据输出目录中尚不存在的新文件路径，并确保父目录已存在；以下命令只创建带注释表头的空表，不覆盖原件，也不生成运行证据：

```bash
: "${LANES_TSV:?请设置为本次尚不存在的 TSV 文件路径}"
( set -o noclobber; printf '# lane_id\towner\tstatus\tevidence_path\tnote\n' > "$LANES_TSV" ) || exit 1
```

再为所选 track 的每条 lane 填一行恰好五列的数据，使用真实制表符分隔，不写字面量 `<TAB>`；有表头时必须以 `#` 开头。`status` 只接受 `pass`、`partial`、`block`，每个 `evidence_path` 必须实际存在并指向该候选的证据；未完成项可引用真实的未完成/阻塞记录，但不能创建假通过证据。缺失路径会使脚本报错，缺 required lane 会得出 `block`，均不得记为通过。

最小命令只读取已经填写、存在且可读的 `LANES_TSV`；本模板不再给出不存在的默认文件名。空表不是可通过的输入，历史 TSV 原件保持不动。

## 最小命令
```bash
: "${LANES_TSV:?请设置为本次已填写的 TSV 文件路径}"
test -f "$LANES_TSV" && test -r "$LANES_TSV" || exit 1
./scripts/network-rehearsal-track-gate.sh \
  --track public_testnet_rehearsal \
  --candidate-bundle output/release-candidates/legacy-public-testnet-rehearsal-01.json \
  --lanes-tsv "$LANES_TSV" \
  --out-dir output/network-rehearsal-gates
```

## 审查清单
- 是否所有 lane 都引用同一 candidate bundle。
- 是否所有 evidence path 都可达。
- 是否 track 对应 required lanes 已全部填写。
- 是否 `pass/partial/block` 与证据内容一致。
- 本次运行结果是否记录在对应 PR 或已使用的 Task，并链接原始证据；只有稳定方法、接口或证据字段改变时才同步 `testing-manual.md` 与相关专业合同，不将逐次运行结论写回手册。
