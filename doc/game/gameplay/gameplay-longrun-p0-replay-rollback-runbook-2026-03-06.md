# Gameplay Long-Run P0：Replay/Rollback 运行手册（2026-03-06）

Lifecycle: current controlled recovery procedure; the date is establishment provenance. Its replay, freeze, signed receipt, player-action disposition, compensation threshold and all-clear rules are safety requirements, not evidence that recovery, rollback or release currently passes.

审计轮次: 7

- 当前产品边界：[`区域冲突、软赛季与可恢复损失`](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003) 的 `REQ-WR-CC-003` / [`AC-6`](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-6) / [`AC-7`](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7) 承接系统性危机、同一世界连续性与恢复项目的一次性玩家结果边界；[`分布式共识与状态可用性`](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 的 `REQ-DCS-002` / [`AC-DCS-002`](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) 拥有 finalized checkpoint、hash-bound snapshot、canonical replay、verified state root 与不匹配时隔离/只读的恢复链产品边界；[`确定性世界执行`](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) 的 `REQ-DWE-004` / [`AC-DWE-004`](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-004) 约束历史 replay 的 governing version；高影响保护动作的授权、证据、范围、期限、恢复/复核/申诉边界见 [`普通共同决策与宪制边界`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006) 的 `REQ-WR-GCB-006` / [`AC-WR-GCB-006`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#ac-wr-gcb-006)。
- 历史 provenance（本 runbook 于 2026-03-06 建档）：旧 `world-continuity-governance-and-recovery.prd.md` 的“7. 组合验收”已退休；其[精确历史 blob](https://api.github.com/repos/eng-cc/oasis7/git/blobs/7ec28b24fd8e139f0811dcc1921d3d78196ff7cf) 绑定提交 `8b3aa0769c5dc88c78af1dab01dbc77d1658fc1b`。后续 `CR-6` 映射仅作为 commit-bound 历史来源，不是 active authority、路线图或当前 readiness 证据。
- 语义缺口与跟进：上述产品锚点只界定玩家结果与治理边界，不代替 replay/rollback 的 runtime、P2P 或 QA 专业实现/证据；`runtime_engineer`、`blockchain_ops_engineer` 与 `qa_engineer` 需在本 runbook 支持当前实现或任何公开 claim 前补齐同一候选的恢复对账证据。
- 专业权威：`doc/world-runtime/prd.md`、`doc/p2p/prd.md`
- 覆盖任务：`TASK-GAME-014`（`PRD-GAME-006-02`）

## 1. 触发条件
- `verify_tick_consensus_chain()` 返回 `DistributedValidationFailed`。
- `first_tick_consensus_drift()` 返回非空（可定位 `mismatch_tick`）。
- 长稳门禁出现共识链路漂移告警，需要执行状态恢复演练或实战回滚。

## 2. 标准处置流程（Runbook）
1. 漂移定位：调用 `first_tick_consensus_drift()` 获取首个 `mismatch_tick` 与原因。
2. 影响区间确认：分别锁定回滚 checkpoint C 与最终 replay target T。C 必须引用保留的稳定 batch、snapshot hash 与 snapshot journal len；T 必须是 C 之后仍可验证的 finalized batch，并绑定 target journal len、target state root 与 canonical journal commitment。禁止把 C 与 T 隐式视为同一位置，也禁止隐式选择“最新” target。
3. 权限注册表预检：operator-local governance public manifest 必须恰好包含 `ops.rollback.on_call.v1` 与 `governance.rollback.v1` 两个固定槽位；两者均为显式 `scheme=ed25519`、`threshold=1`，且 `signer_id` 与公钥跨角色互不相同。先执行下述 import，再执行 strict audit；audit report 必须携带该次审计原始 manifest bytes 的 canonical `audited_manifest_digest`。signer CLI 与 runtime 必须分别验证 report digest、随附 manifest bytes、当前 active rollback registry 三者完全一致；任一步失败或 world/manifest 不完全一致时禁止发起回滚与 all-clear。
4. 协议与信封确认：客户端必须先完成 Viewer protocol v2 Hello，并确认唯一 canonical capability `governed_rollback_replay_v2`。只使用 `authoritative_recovery.rollback_v2`；legacy `rollback` 仅为 decode-only，返回 `rollback_shape_unsupported`，未协商 v2/capability 返回 `protocol_upgrade_required`。两位签名者分别对原始 v2 canonical intent bytes 签名；不得在签名后重新解释或替换字段。
5. 签名内容复核：intent 必须精确绑定 `rollback_ticket`、C 的 batch/snapshot hash/journal len、T 的 batch/journal len/state root/journal commitment、`expected_reorg_epoch`、`max_replay_events`、`max_replay_bytes`、reason、签发/过期时间与 nonce。任何 legacy、partial、未知字段或字段不一致的 shape 均须在授权执行前 fail closed。
6. 回滚与 replay：通过 Viewer 提交完整 `RollbackV2` 请求。Viewer 解析 C→T 的 finalized journal suffix，先校验 epoch、事件数/序列化字节上限和 runtime canonical journal commitment，再调用 direct runtime `rollback_to_snapshot_with_reconciliation(snapshot_C, journal_through_T, reason, target_batch_id_T, approval, now_ms)`。runtime 继续作为 registry、签名、有效期、nonce、journal commitment、target root 与 reconciliation 的最终判定方；只有 receipt 可序列化且所有校验成功后才能提交 Viewer 的 world、batch、challenge、checkpoint 与 reorg epoch 候选状态。
7. 恢复对账：再次执行 `first_tick_consensus_drift()` 与 `verify_tick_consensus_chain()`，必须均为“无漂移”；实际 target state root 与 journal commitment 必须与签名 intent/持久化 outcome 一致，committed reorg epoch 必须与 receipt/outcome 一致。
8. 幂等回执与审计：成功 outcome 按 nonce 与 canonical intent digest 持久化。相同 nonce + 相同 canonical intent 的重试不得再次 mutation 或增加 reorg epoch，必须返回同一逻辑 receipt；该 exact same-envelope retry 仅用于原信封尚未过期、authority generation 仍为当前版本时的即时 authenticated transport retry。延迟重试、信封已过期、authority 已轮换，或断线重连后的 receipt lookup，必须改用当前 active governance authority 签名且携带新的一次性 access nonce 的 `authoritative_recovery.get_rollback_receipt`；否则预期认证拒绝。相同 nonce + 不同 intent 返回 `rollback_nonce_conflict`。不得把 capability negotiation 或 rollback nonce 当作读取授权，也不得通过再次执行 rollback 猜测结果。确认 `RollbackApplied`、ticket、两个 authority id、nonce、target commitment/root 与 receipt evidence 均已归档。
9. 玩家动作处置：对受 C→T/fork 影响的每个动作记录 `preserved_at_target`、`replayed`、`rejected_fork` 或 `compensation_required`。未知归属经治理确认属于玩家时必须按既有 disposition 分流：`preserved_at_target`/`replayed` 只附加 player identity，保持原状态且不得创建补偿或工单；只有 `rejected_fork` 转为 `compensation_required`，关联公开安全的 owner/ticket 别名与状态。状态推进或未知归属修复必须由 active governance rollback authority 对绑定 batch/event、`issued_at_ms`、`expires_at_ms` 与一次性 nonce 的 canonical 请求签名。首次产生状态变化的归因请求必须把 world/readiness/recovery generation 与 operator nonce 消费记录原子持久化；过期、超出 future-skew、篡改或已消费 nonce 的请求必须在 mutation 前拒绝。对已经持久化的同一终态分类，使用新 nonce 的完全相同 authenticated retry 是幂等 no-op：返回当前 receipt，不消费该次提出的 operator nonce，也不改写 world、readiness 或 recovery generation evidence；不同 player、player/system 反向分类或其他不一致重试必须返回 `rollback_attribution_resolution_conflict`，同样不得消费 nonce 或改写证据。`rejected` 是已裁决终态，不再阻塞 all-clear；缺失玩家归属必须以 batch/event/action 身份进入 census 并留下 `player_attribution_incomplete` blocker，直至授权修复。经治理签名确认确属系统自动化而非玩家动作时，可持久化为 `system_authored`：必须保留原 batch/event/action census 证据，不得伪造 player id、玩家补偿或补偿工单；只有该分类与 nonce 原子持久化后，才能移除该条归因 blocker。玩家归属与 `system_authored` 都是该 batch/event census 条目的终态分类；纠错只能走新的治理事件/事故证据链，不得原位重写。对玩家/社区只能按已持久化分类与原 disposition 表述；`system_authored` 只能称为“已核验的系统自动化/非玩家动作”，不得称为受影响玩家或暗示补偿资格，`preserved_at_target`/`replayed` 玩家不得暗示获得补偿。

## 3. 演练命令（required-tier）
```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_ops_tools --bin oasis7_governance_registry_import -- --world-dir <world-dir> --public-manifest <operator-local-public-manifest.json>
env -u RUSTC_WRAPPER cargo run -p oasis7_ops_tools --bin oasis7_governance_registry_audit -- --world-dir <world-dir> --public-manifest <operator-local-public-manifest.json> --strict-manifest-match
env -u RUSTC_WRAPPER cargo run -p oasis7_protocol_tools --bin oasis7_rollback_strict_audit_evidence -- --audit-report <exact-audit-report.json> --manifest <exact-public-manifest.json> --output <signed-evidence.json> --authority-id <active-governance-authority-id> --rollback-ticket <ticket> --receipt-id <receipt-id> --canonical-intent-digest <intent-digest> --recovery-snapshot-hash <snapshot-hash> --candidate-state-root <target-root> --reorg-epoch <epoch> --issued-at-ms <issued-ms> --expires-at-ms <expires-ms> --nonce <one-time-audit-nonce> --private-key-file <custody-mounted-governance-key-file>
./scripts/governed-rollback-v2-drill.sh
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required runtime::tests::persistence::rollback_with_reconciliation_recovers_from_detected_tick_consensus_drift -- --nocapture
```

`governed-rollback-v2-drill.sh` 是 repo-native 可执行协议演练门禁，覆盖 v2 Hello capability、RollbackV2 协商边界、不可变 receipt/同 nonce 重试、nonce conflict、重启恢复与 `GetRollbackReceipt` 证据链。任一子测试失败即保持 rollback freeze；脚本通过不替代 operator-local manifest import/strict audit 或真实 custody 签名证据。

strict-audit evidence CLI 必须直接读取本轮 audit report 与 manifest 的原始字节，不得复制或重建 JSON。软件 custody 可使用仅当前 operator 可读的 `--private-key-file`；HSM/外部 signer 使用两步模式：先省略 signer/output 并指定 `--canonical-payload-output <payload.bin>`，由 HSM 对该文件签名，再以同一组参数加 `--signature-file <signature.hex> --signer-public-key-file <public-key.hex> --output <signed-evidence.json>` 组装并本地验签。CLI 不向 stdout/stderr 输出密钥、签名或 evidence 内容；当前仅支持在 Unix 上以 `0600` 权限创建私有临时文件并原子发布，非 Unix 平台在写入前 fail closed 且不生成 artifact，不得通过预建空文件或降低权限绕过。payload、签名和临时 key mount 按 custody policy 清理；每次执行使用新的输出路径，CLI 不覆盖已有文件或符号链接。

public manifest 只携带公钥。两把 Ed25519 私钥必须分别由 on-call 与 governance 的外部 custody/HSM 保存，不得写入 manifest、world snapshot、Viewer 请求模板、仓库或同一 operator 主机；签名者只接收并签署 canonical intent bytes。

Viewer API 与 direct runtime API 不可混用：operator/客户端只调用已协商 capability 的 `RollbackV2` 与 `GetRollbackReceipt`；capability negotiation 仅表示功能发现，不构成授权。原 `RollbackV2` exact same-envelope 只可在信封有效期内且 authority generation 未变化时用于即时 authenticated transport retry，不是延迟或重连后的 receipt 读取凭证。完整 `GetRollbackReceipt` 请求必须由当前 active governance rollback authority 对绑定 rollback nonce、签发/过期时间和新的一次性 access nonce 的 canonical request 签名；延迟、原信封过期、authority 轮换或断线重连后沿用原 rollback 信封/nonce 时预期认证拒绝。过期、篡改、玩家签名、重放或未知/停用 authority 均须拒绝，成功 access nonce 必须随 recovery generation 持久化并在重启后继续拒绝重放。direct runtime API 是 Viewer 内部的执行边界，不提供 protocol negotiation、C/T 解析、玩家 disposition 汇总或对外 receipt 查询。排障时可比对两层证据，但不得绕过 Viewer 直接构造 partial intent 作为标准处置。

readiness reevaluation 必须区分 supplied evidence 错误与有意记录 holding：传入 `Some(audit_evidence)` 时，任何签名、digest、report、manifest、当前 registry 或 receipt/root/epoch 绑定失败都返回 `strict_audit_evidence_invalid`，并且不得改写 readiness、消费 audit nonce、改变 receipt 或提交 recovery generation。operator 必须保留失败请求及验证输出到受控 incident evidence、升级 runtime/viewer 与 governance owner，并继续依据 `GetRollbackReceipt` 对当前 root/epoch、drift、consensus chain、compensation、attribution、evidence 与 registry 的动态检查保持 holding；不得把该错误解释为 blocker snapshot 已持久化。只有 operator 明确需要在没有 fresh audit evidence 时持久化当前 blocker snapshot，才提交 `audit_evidence=None` 的 reevaluation；成功后 receipt 必须包含 `fresh_strict_audit_evidence_required`，且此操作不消费 strict-audit nonce。不得用 `None` 掩盖本应调查的 invalid evidence，也不得因旧 readiness 记录仍存在而发布 all-clear。

### Authority rotation / revocation

1. 一旦私钥疑似泄露、人员离岗或 authority 需要轮换，立即停止所有 rollback，撤销旧 custody 权限并生成新的角色专属密钥；不得用另一角色的 signer id 或公钥临时顶替。
2. 在 operator-local manifest 中原子替换对应固定槽位的 `signer_id` 与公钥，保留另一角色不变；再次核对两个角色的 signer id、公钥均唯一，threshold 仍严格为 1。
3. 运行 import。import 会先验证完整 manifest 并构造两个 registry 候选，任何缺槽、错 scheme/threshold、重复 id/key 都 fail closed，且不会覆盖原 world。
4. 运行 strict audit，只有报告 `manifest_match_pass=true` 且命令成功退出后才能恢复 rollback。audit 失败时保留 rollback freeze，修复 manifest/world 漂移后重新 import + audit；禁止通过手工改 snapshot 或 runtime setter 绕过。
5. 将 manifest 摘要、import/audit 输出、轮换 ticket、custody 撤销/启用记录归档到 incident/change evidence；旧私钥销毁或进入不可签名的法定保留状态。

## 4. 事故沟通与证据边界

- Owner 与升级：当班 incident commander 为单一事故 owner，内部进展与阻塞统一回写绑定 incident/change ticket；runtime、blockchain ops 或 custody 任一门禁失败时立即升级到治理应急 sink 与对应工程 owner，不得在聊天或社区渠道单独决策。
- 影响与受众：每次更新先标注受影响 world/时间段、功能（登录、行动、交易、结算/排名）、数据区间与受众（值守/operator、节点运营者、全部或指定玩家）；未确认时写“评估中”，不推测丢失规模、恢复时间或补偿。
- 触发与节奏：内部触发为 drift/chain/root 校验失败或 rollback freeze；operator 触发为 import/strict audit/custody 失败；玩家触发为可见进度回退、动作被标记 `rejected_fork`/`compensation_required` 或结算暂停。首次 holding 后默认每 30 分钟更新一次，即使仍无新结论；更高严重等级政策规定更短节奏时以该政策为准。

模板（仅在对应触发后使用）：

- 内部：`[时间][严重度] <world> 检测到 <drift/root/chain> 异常；owner=<IC>，当前阶段=<freeze|audit|rollback|replay|verify>，影响=<已知/评估中>，下次更新=<time>。`
- Operator：`<world> 已进入 rollback freeze。请停止写入/结算操作，保留 manifest、import/audit 输出与 custody 证据，按 <ticket> 等待 IC 指令；不得使用 v1 fallback。`
- 玩家 holding：`我们正在处理 <world/功能> 的状态一致性问题，期间 <可用性/结算> 可能受限。玩家无需重复提交动作；下次更新不晚于 <time>。`
- 玩家更新：`<world/功能> 恢复工作处于 <audit|rollback|replay|verify> 阶段，当前影响=<已知/评估中>，暂无需玩家操作；下次更新=<time>。`
- All-clear：`<world> 已完成授权注册表 strict audit、回放至授权 target root、无 drift 复核与 consensus chain 验证；<功能> 已恢复。受影响动作/回执处置=<摘要>，后续跟进=<ticket/status page>。`

All-clear 必须同时满足：strict audit 成功且无 rollback authority blocker；完整 audit report 与 manifest 原始字节由当前 registry 中 active governance Ed25519 authority 签名，签名 envelope 绑定 rollback ticket、recovery generation snapshot、receipt、intent digest、target root、reorg epoch、有效期与一次性 nonce；该 audit nonce 在首次提交 readiness 前必须未被消费，并且只能在 readiness generation 成功持久化提交时恰好消费一次，提交失败不得留下仅内存或半提交的消费状态，成功后任何重放均须拒绝。receipt 可通过 `GetRollbackReceipt` 重取并在读取时重新通过 evidence 有效期、当前 root/epoch、drift、consensus chain 与 compensation 状态检查；receipt、持久化 outcome 与签名 intent 联合核对后 nonce/digest、C/T、journal commitment、target root 与 reorg epoch 一致；候选状态精确匹配 T 的 `state_root`；`first_tick_consensus_drift() == None`；`verify_tick_consensus_chain()` 通过；所有受影响动作均已有上述四类 disposition，所有 `compensation_required` 均有关联公开安全的 owner/ticket/status，且不存在 `player_attribution_incomplete`。任一项未满足只能发进展更新，不得发 all-clear。

访问与脱敏：incident/change ticket 、完整 nonce、manifest 全文、strict audit/import 原始产物及 custody/HSM 记录仅向 IC、授权 operator、governance/security 和必要工程 owner 开放，按最小权限存入受控 incident sink。玩家/社区渠道只发布影响摘要、阶段和下次更新时间；对 ticket 使用公开状态页引用或脱敏别名，对 nonce 仅显示缩短摘要，manifest/audit 仅公布结论与非敏感 digest，不公布 signer id、公钥组合、文件路径、签名或 custody 细节。

## 5. 通过标准
- 演练命令返回 `rc=0`。
- 漂移被成功定位到具体 `mismatch_tick`。
- 回滚后 `first_tick_consensus_drift() == None`。
- 回滚后 `verify_tick_consensus_chain()` 通过。
- 未协商 v2/capability 返回 `protocol_upgrade_required`；legacy `rollback` 返回 `rollback_shape_unsupported`；缺少完整 v2 approval、partial/未知字段、篡改、过期、C/T 或 epoch/bounds/commitment 不匹配、未知/停用 authority 均在 mutation 前拒绝，且 world、journal、batch 与 reorg epoch 不变。
- 相同 nonce + 相同 canonical intent 的重试返回持久化 receipt 且不产生第二次 mutation；相同 nonce + 不同 intent 返回 `rollback_nonce_conflict`；`GetRollbackReceipt` 可在重连后取得成功 outcome。
- 事件链存在唯一 `RollbackApplied` 记录，且 ticket、两个独立 authority id、nonce、C/T、journal commitment、target root 与已验证信封/receipt 完全一致。
- 所有受影响玩家动作均有最终 disposition；`compensation_required` 记录均有关联 owner、ticket 与当前状态。

## 6. 失败处置
- 若回滚后仍有漂移：立即阻断发布，保留快照/日志，升级到治理应急流程。
- 若漂移定位失败：先执行一次完整快照恢复重放，再人工比对 `tick_consensus_records` 链路。
- 若 receipt 无法重取、字段不完整、玩家 disposition 未完成或补偿无 owner/ticket：保持 rollback freeze 与玩家 holding，不得 all-clear；升级 runtime/viewer owner 补齐持久化证据后再复核。
