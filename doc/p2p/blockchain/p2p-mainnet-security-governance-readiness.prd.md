# oasis7 主网安全、治理与创世就绪度

- 对应设计文档：`doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md`
- 对应GitHub Issue/Project task truth：GitHub Issue / GitHub Project

## 目标与权威边界

本专题是 P2P 专业域关于系统级安全、治理 signer、创世与主网进入条件的稳定 authority。它吸收 2026-03 的 crypto baseline、MAINNET readiness、production signer custody、governance signer externalization 与 genesis ceremony 专题；那些历史三件套不再作为现行入口。

本专题不宣布主网、公开发售、production settlement、mint readiness、production custody 或 public launch。当前公开状态仍以根 `README.md` 为准；网络层级以 `formal-network-tiers-testnet-mechanism.*` 为准；玩家承诺不在本专题中定义。

## 范围

覆盖系统级安全判定、signer custody、registry-first governance、genesis/ceremony/QA 阻断条件及其专业验证入口；不覆盖代码实现、实际运维执行、公开承诺或玩家产品设计。

节点运营者需要分别确认本机身份、治理决定、共识实际采用和私有状态读取权限，不能从其中一个成功结果推导其余权限。本文拥有治理主体认证、成员应用和私有 full-state 读取授权的专业规则；[节点身份合同](../node/node-identity-replication-contract.prd.md#nir-bootstrap)拥有本机初始化的持久性。[产品 DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)与[DCS-006](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-006)分别拥有角色隔离和私有读取授权的产品结果。

## 接口 / 数据

`governance_finality_signer_registry`、`governance_validator_admissions`、`governance_main_token_controller_registry`、validator membership/governed stake/finality signer binding、genesis binding status 与 QA evidence verdict 是本专题的专业合同字段；私钥、seed 与助记词禁止写入。

## 当前安全与阶段判定

- 当前系统级判定保持 `not_mainnet_grade`；可运行的局部签名、allowlist、registry 或测试证据不能单独升级该判定。
- `public_testnet` 是 controlled、resettable、non-mainnet 的测试层级；它不承诺 production settlement、mainnet 或公开发行。
- 创世仍为 `not_mint_ready`：`logic_frozen_address_binding_pending`、`TBD_BEFORE_MINT`、`pending_binding`、`ready_pending_address_binding` 任一状态，未完成 ceremony evidence，或缺少 QA `pass` 都必须阻断。
- 主网或 mint 口径仅能在相应技术、治理、QA 与公开口径权威同时给出证据后重新评估；本专题不替代该决定。

下述新增授权与撤销条款是后继目标合同。源码基线 `b42a9e3fb8c8cd60679936c5f46c2fde6f62cf1e` 中，validator admission 已有 controller ID、状态转换、signer 唯一性与 epoch 校验，但四类通用治理 Action 尚未形成 controller proof 的端到端验证；未据此证明存在公开 HTTP 越权入口。成员视图当前为 `supports_safe_runtime_update=false`、`applied_consensus_membership=None`，只读投影不能证明共识采用。`FormalNodeReadGrantV1` 已验证 issuer、world、node、role、authority epoch 与最长 30 天租约；源端没有逐节点撤销状态，较高 `grant_epoch` 只是本机安装顺序。以下目标未由文档更新变成已实现或运行通过；设计的[当前差距](p2p-mainnet-security-governance-readiness.design.md#readiness-current-capability-boundary)保留源码依据。

## 现行合同

### 资产与密码学基线

- `POST /v1/chain/transfer/submit` 要求 normalized ed25519 `public_key` 和 `signature`；`from_account_id` 必须等于 `oc:pk:<normalized_public_key_hex>`。缺失/无效签名或账户不匹配必须在余额、nonce 和 consensus submit 前拒绝。
- `MainTokenActionAuthProof` 同时进入 `ConsensusActionPayloadEnvelope`，并在 shared `NodeRuntime` submit layer 对受保护 main-token actions 重验。有效请求仍执行 amount、same-account、balance 与 nonce-replay 规则。
- `oasis7_client_launcher` 的 native 与 wasm 转账入口必须先按同一 canonical `TransferMainToken` payload 与域前缀生成 ed25519 `public_key/signature`，再提交；wasm 通过 `oasis7_web_launcher` 服务 HTML 时注入的 `window.__OASIS7_VIEWER_AUTH_ENV` 读取受信本地 signer bootstrap。
- bootstrap 缺失或仅有 public/private key 任一侧时，客户端必须在本地明确失败并阻止 transfer POST；不得静默降级为 unsigned submission。该 local env/config/HTML bootstrap 仅是便利路径，不构成钱包托管、生产 keystore 或 custody。
- 上述完成态不等于 custody 或 mainnet readiness，也不替代系统级多证据判定。
- Node/replication 的签名与 allowlist 是局部防护信号；系统级评估仍须同时检查 custody、治理、创世和验证证据。
- 所有系统级 verdict 必须能回溯到当前代码、专业文档或测试证据；不得以历史专题的完成复选框替代当前核验。

### Signer custody 与治理真值

- local/dev/preview 的 config、env、HTML bootstrap 或 deterministic seed 只能作为便利路径，不能被描述为 production custody。
- production 目标要求受控 signer material、明确的 rotation、revocation、审计留痕与环境策略；私钥、seed 和助记词不得进入仓库或本专题。
- runtime 以 world-state governance registry 为治理 finality signer、validator membership、governed stake 与 controller policy 的优先真值。registry 存在时不得退回 local seed/config 作为 production truth。
- validator/finality admission 保持 `apply -> approved_candidate -> probation_ready -> active -> revoke/rotate` 生命周期。准入、activation、stake-weighted quorum 与 finality signer binding 必须由 world-state 及其治理动作验证。
- genesis import 只适用于空 execution world；已有 snapshot/journal world 必须走专用 registry migration/import，不得覆盖或伪造恢复状态。

<a id="readiness-governance-authorization"></a>
### 治理主体与动作授权目标

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；唯一产品声明：[REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)。系统设计：[治理主体认证](p2p-mainnet-security-governance-readiness.design.md#readiness-governance-authorization)。

validator 的 approve、activate、rotate、revoke 及受保护的 admission 写入必须由当时合法 controller policy 授权。`controller_account_id`、本机 root/PeerId、进程归属、transport 身份、运营者配置或单个有效签名都不能替代该 policy 要求的授权。公开申请即使另行开放，也只能形成无权威候选意图，不能自我批准、激活或撤销其他节点。

授权必须绑定确切世界、完整动作、适用的已提交 policy、重放防护和有效范围；所有进入 World 的外部输入路径采用同一规则。不能在一个批次中先改变 policy，再用该变化追认本批治理动作。签名不足、policy 错误或已失效、跨世界、动作篡改与重放均在修改 registry 前拒绝。历史重放仍验证当时合法 policy 与原证明，不用今日 policy 撤销已确认历史。

验收须覆盖“正确公开 controller ID 但没有密钥／授权”“有效 signer 但不足 threshold”“本批自授权”“跨世界／旧 policy／重复请求”和经不同入口提交的等效负例；拒绝后 admission、registry、nonce 与权威历史不得出现未授权效果。已有字符串与状态机测试不能代签此验收。

当前受控 profile-v2 的跨 parent 历史消费按[qualified 治理历史认证](p2p-mainnet-security-governance-readiness.design.md#readiness-governance-history-authentication)接通原 policy／nonce 重验及目标完整 checkpoint 恢复；不等于任意 raw journal tail 的全状态重建。组件安装、私有读取撤销与 BFT 等后续义务保持。

<a id="readiness-membership-application"></a>
### 治理生效与实际成员应用目标

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；唯一产品声明：[REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)。系统设计：[成员应用](p2p-mainnet-security-governance-readiness.design.md#readiness-membership-application)。

运营者必须能区分申请／批准、已提交并排程、协议已生效和本机已应用。World registry 的 effective 结果是治理真值，只有对应运行组件完成同一转换并给出持久应用确认，才可报告该节点按新集合 voting／proposing；`None` 表示缺少应用证据，不表示空集合。

目标 BFT 的唯一转换规则由[P2P 集合转换设计](../design.md#des-p2p-bft-validator-transition-v1)拥有：高度 `h` 最终提交的更新固定在 `h+2` 生效，`h/h+1` 使用旧集合。未来 epoch 申请只是意图，必须先确定到协议高度的映射再排程，不能让各节点自行推迟已提交的生效高度。成员、stake、signer binding 与对应写权必须一致切换；未完成安装的节点在边界后停止投票／提议，不继续使用旧集合，也不把可读副本误报为验证者就绪。被撤销或替换 key 只对生效点后的新权威动作失去资格，合法历史仍可验证。

验收须在同一转换前后覆盖新增、撤销、轮换、重复安装、安装中断／重启与滞后节点；治理证明、实际应用摘要和边界前后消息资格一致，旧集合不能越界推进。受控单权威 profile 的 writer 交接继续服从其独立 epoch/fencing 合同，不因本条获得 BFT 成员能力。

<a id="readiness-full-sync-read-authorization"></a>
### 私有 full-state 读取授权与撤销目标

Trace refs: O7-REQ-cb84969d-dd7f-4782-ab30-167ffc59a0c8；唯一产品声明：[REQ-DCS-006](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-006)。系统设计：[读取授权与撤销](p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-read-authorization)。

full/state-sync 等服务角色可以按网络规则参与，但取得包含私有内容的完整 Snapshot/Journal 必须有独立、可验证且仍有效的读取授权。读取 grant 不授予 validator、writer、governance 或 archive 保证；角色参与权限和私有数据权限不能互相推导。

目标 grant 撤销由源端执行：每个被授权 node/role 有持久、单调的授权代际与 active/revoked 状态，较高代际替换及显式撤销都必须使旧授权不能继续取得新数据。manifest、object 和尚未放行的后续 chunk 均受当前授权及有界新鲜度约束；缺失、陈旧、冲突或无法验证的授权状态必须拒绝读取。issuer 自身恢复旧备份也不能重新授予已撤销代际；授权及签发记录须有独立不可回退的下界，不能仅凭同一密钥签出的新 lease 恢复读取。签发、替换与撤销只接受该 issuer 既有信任边界授权的管理主体，读取 grant 或 node key 本身不授予管理权限。

撤销请求已接受、授权真值已持久变更和所有可服务 provider 实例已停止放行分别表达。只有完整签发记录中的每个实例／lease 已有不可回退的撤销 cutoff、或全部对应服务能力被可靠隔离、或旧授权新鲜度 lease 已到期且停止读取可验证时，才能报告撤销完成。同名 provider 的新实例确认不能关闭旧／复制实例的未过期 lease；无法证明 cutoff 不会被旧快照恢复时必须等该旧 lease 的最晚有效边界。仅移出 provider 列表、断开控制面、停止客户端或本机安装新 grant 不足以证明完成。分区 provider 不能无限沿用缓存授权；撤销期间保留 pending 与实际剩余边界。

撤销阻止生效边界后的新授权放行，不承诺删除已交付副本或收回此前合法放行并进入传输缓冲区的字节。验收覆盖多 provider、分区／重启、旧代际回放、连续 chunk 传输和缓存授权过期；同时证明其他未撤销授权仍按原范围可用。现有 v1 租约仍可能持续至到期、最长 30 天；要求本目标的部署不得静默降级到 v1 撤销语义。

<a name="创世、ceremony-与-qa"></a>

### 创世、ceremony 与 QA

- 创世 freeze 必须同时满足 slot registry、bucket execution、recipient/controller binding、signer policy、ceremony checklist 和 QA evidence bundle。
- evidence 只能记录公钥、账户绑定、threshold、审批与 QA verdict；不得记录任何私密 signer material。
- `conditional_draft_only`、缺失 evidence 或未通过 QA 均为阻断，不能用历史规格 gate 完成态绕过。

## 主网进入条件与非目标

主网进入至少需要：受控 custody、registry-first governance、可审计的 rotation/revocation/failover、冻结创世与 QA pass、formal network-tier 的 mainnet prerequisites，以及独立的产品/公开口径复核。任何单一 topic 或 rehearsal 成功都不构成主网进入授权。

本专题不定义 network-tier manifest、public endpoint、faucet、release procedure、实际 mint、密钥托管后端或运行时实现，也不替代 QA 放行、LiveOps 对外表述或产品真值。

## 里程碑

- M1：吸收历史 security/governance/genesis authority，保留负向 readiness 标记。
- M2：由对应专业角色持续验证 custody、governance、genesis 与 network-tier 前置条件。

## 风险

- 将规格完成态误报为 production/mainnet/mint 完成态。
- 用 local seed/config 或 rehearsal 证据替代 registry、ceremony、QA 或 tier gate。

## 验证与追溯

- 文档迁移后应通过 `./scripts/doc-governance-check.sh`、`./scripts/readme-link-check.sh` 与旧源路径零引用扫描。
- 语义回归至少抽检 asset authorization、governance registry drill 与 network-tier manifest smoke；这些是合同核验，不能被解释为 mainnet readiness pass。
- 新增治理认证、实际成员安装与多 provider 读取撤销分别按配对设计的六列验证映射实施；现有 registry drill、成员只读视图或 grant lease 测试不能代签这些目标。
- 历史完成过程由 Git history 与 GitHub task evidence 追溯；本稳定专题只承载当前合同、阻断条件和后续验证入口。
