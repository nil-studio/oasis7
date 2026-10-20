# 主网安全、治理与创世就绪度设计

- 对应需求文档：`doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.prd.md`
- 对应GitHub Issue/Project task truth：GitHub Issue / GitHub Project

## 设计原则

1. 将历史专题按 authority 合并，而不是将历史完成状态升级为当前状态。
2. 保持 `registry-first`：world-state registry 是治理 membership、stake、finality signer 与 controller policy 的运行真值。
3. 以 fail-closed 处理 signer/custody、genesis binding、ceremony 和 QA 证据缺失。
4. 将正式网络层级、运行步骤和公开沟通分别下钻到 formal-network-tier、runbook 和相应专业权威。

## 合并结构

| 历史主题 | 吸收后的稳定章节 | 不应丢失的限制 |
| --- | --- | --- |
| crypto baseline | 资产与密码学基线 | 局部原语/授权完成不等于系统级安全。 |
| MAINNET readiness | 主网进入条件 | `not_mainnet_grade` 直到所有跨域条件具备。 |
| signer custody | Signer custody 与治理真值 | preview bootstrap 不得成为 production custody。 |
| governance signer | Signer custody 与治理真值 | registry-first、admission、quorum、rotation/revocation。 |
| genesis ceremony | 创世、ceremony 与 QA | `not_mint_ready` 直到 binding、ceremony 与 QA pass。 |
| signed transaction authorization | 资产与密码学基线 | HTTP parsing、canonical auth proof、shared-layer re-verification 与 nonce enforcement，以及 native/wasm 同协议本地产签和缺失/partial bootstrap 的 fail-closed submission 必须同时存在；不等于 custody/mainnet。 |

## 实现和验证下钻

- 资产授权：本专题“资产与密码学基线”的 signed transaction contract。
- 客户端签名路径：native 复用本地 signer helper；wasm 使用 `window.__OASIS7_VIEWER_AUTH_ENV` 的受信本地 bootstrap，并以与 native 相同的 canonical payload/域前缀生成签名。缺失或 partial bootstrap 必须在页面侧阻断，不得发出 unsigned transfer POST；这不等同于 production custody。
- Node/replication 的可恢复签名合同：`doc/p2p/blockchain/p2p-blockchain-p2pfs-hardening.*`。
- formal tier、manifest 和 public-testnet checklist：`doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.*` 及 companion runbook。
- registry 操作和 evidence：`scripts/governance-registry-drill.sh`、`scripts/governance-registry-live-drill.sh` 与 `doc/testing/evidence/`。

任何实现改动均须分别由 runtime、blockchain ops、QA 与适用的 product/LiveOps authority 审核；本文不提供实现替代方案或放行判断。

## 审读基线与当前差距

- Owner role：`runtime_engineer`、`blockchain_ops_engineer`；验证由 `qa_engineer` 独立核对。
- 本次文档/源码审读基线：`4e94163a3dff9ba69f6959b406ca3fcedf8aa097`，日期 `2026-10-15`。当前实现说明与历史阶段记录分别保留；本次不执行签名、部署、恢复或构建，不推导新的运行完成态。
- 产品关系只承接下表明确列出的局部义务；功能整体仍为部分覆盖，现有产品实现状态与原验收层级不变。
- 下表产品 N/A 仅限专业签名校验与 ceremony 证据方法，不表示转账玩家体验、资产权属或公开发行没有产品影响；这些消费关系未在本批准确建立，仍待对应产品与专业 owner 核对。原签名、资产授权、创世与 QA 专业义务全部保留，不因此豁免。

## 问题、目标与上游分工

目标是把局部 crypto、governance 与 genesis 证据连接到各自准入条件，同时保留 not_mainnet_grade / not_mint_ready。配对 PRD 的现行合同拥有安全与阶段判定；formal network tier 拥有网络分级；QA 拥有实际 pass；产品/LiveOps 拥有公开口径。本设计不生成任何准入授权。

<a id="readiness-registry-isolation"></a>
## 治理权限与状态转换

本节局部承接 DCS-003 的受治理 validator authority：world-state registry 优先于本地 seed/config，membership、stake、finality signer 和 controller policy 必须经治理验证。生命周期为 apply → approved_candidate → probation_ready → active → revoke/rotate；候选或 probation 不能代替 active。服务节点、local signer bootstrap 与单个有效签名不能取得最终写入权。完整公开服务暴露面、轮换证明与 BFT quorum 的组合仍须 P2P/runtime 与 QA 验证。

<a id="readiness-auth-genesis"></a>
## 接口、提交与恢复边界

资产请求的 public_key/signature 与 from_account_id 必须匹配；canonical auth proof 在 shared NodeRuntime 再验，余额、nonce、amount 与 same-account 规则仍执行。native/wasm 均先本地产签；缺失/partial bootstrap 在 POST 前失败，不降级 unsigned。客户端 bootstrap 不等于 custody。Genesis import 仅适用于空 execution world；已有 snapshot/journal world 只能走专用 registry migration/import，不覆盖恢复状态。Ceremony 的 slot、bucket、recipient/controller、signer policy 和 QA evidence bundle 缺任一项就保持阻断；pending_binding 等状态不能被历史 gate completion 升级。

## 部署、安全、兼容与剩余风险

production custody、rotation/revocation/audit、真实 registry activation、ceremony 和 QA pass 必须分别提供证据；本次没有运行这些动作，也不读取或生成 private signer material。兼容 local/dev/preview 便利路径不意味着允许它成为 production truth。单次 drill、三等权 topology 或 signature 原语不能证明完整 BFT/f=1、主网或 mint readiness；上述主网进入条件不因文档规范化改变。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | registry-first 与 validator activation 保留治理 authority，不从 local/config 或服务角色取得写权。 | [registry 隔离](#readiness-registry-isolation) | governance / runtime / QA | 仅 registry 优先级与 activation 子范围；服务暴露面、完整 BFT/rotation 证明未覆盖。 |
| [资产与密码学基线](p2p-mainnet-security-governance-readiness.prd.md#资产与密码学基线) | 客户端先签名并阻止缺失/partial bootstrap；API 与 shared runtime 必须验签及 nonce 约束。 | [授权与创世](#readiness-auth-genesis) | client launcher / runtime / QA | 产品适用性：N/A；理由：本行承接专业资产授权合同，未建立独立玩家经济或发行承诺；范围：signed transfer 边界。native/wasm 与 shared-layer 完整联测仍需补实际证据。 |
| [创世、ceremony 与 QA](p2p-mainnet-security-governance-readiness.prd.md#创世、ceremony-与-qa) | 缺 binding、ceremony、QA evidence 时不进入 mint；drill 不签发 release verdict。 | [授权与创世](#readiness-auth-genesis) | blockchain ops / QA / product / LiveOps | 产品适用性：N/A；理由：本行定义专业证据与安全阻断条件，不宣布玩家开放；范围：ceremony/readiness。真实 mint、production custody 与公开发布未覆盖。 |

### 11.1 验证映射表

以下是验证要求与已有入口的对应，不是本次执行结果。源码/测试定义只提供静态核对；实际验收须在同候选、适用 feature/config 与环境下运行，并保留命令、提交、输出、失败样例和未覆盖范围。文档检查通过不证明运行、部署或 release readiness。

| 上游 requirement / acceptance | 本设计条款 | 验证目标 / oracle | 验证方法 / 精确来源 | 证据产物 | 未覆盖边界 / residual risk |
| --- | --- | --- | --- | --- | --- |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | [registry 隔离](#readiness-registry-isolation) | world registry 优先；无合法 registry 的已有世界不能用 genesis 覆盖；测试通过也不等于生产 custody。 | 审读并执行 [testing manual](../../../testing-manual.md#s4b：governance-registry-drill（l4-前置-l4）) 对应 registry 回归文件 `crates/oasis7_chain_runtime/src/host/governance_registry_tests.rs` 中 world_finality_registry_overrides_node_pos_config、activation 后 effective registry 及 existing-world genesis 拒绝断言。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 仅 registry 优先级与 activation 子范围；服务暴露面、完整 BFT/rotation 证明未覆盖。 |
| [资产与密码学基线](p2p-mainnet-security-governance-readiness.prd.md#资产与密码学基线) | [授权与创世](#readiness-auth-genesis) | 无效签名/账户不匹配在 consensus submit 前拒绝；不能以 API 局部测试代签所有层。 | 执行 [testing manual](../../../testing-manual.md) 对应 API 回归文件 `crates/oasis7_chain_runtime/src/host/transfer_submit_api_tests.rs` 的 missing/invalid signature、account mismatch 拒绝断言；nonce replay 另补 shared-layer 证据；该文件只覆盖 API 子范围，客户端与 shared-layer 需分别补证。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 产品适用性：N/A；理由：本行承接专业资产授权合同，未建立独立玩家经济或发行承诺；范围：signed transfer 边界。native/wasm 与 shared-layer 完整联测仍需补实际证据。 |
| [创世、ceremony 与 QA](p2p-mainnet-security-governance-readiness.prd.md#创世、ceremony-与-qa) | [授权与创世](#readiness-auth-genesis) | rotation/revocation/failover 的实际证据与 ceremony 条件分别判定；无 QA pass 不升级。 | 按 [testing manual](../../../testing-manual.md#s4b：governance-registry-drill（l4-前置-l4）) 核对适用 registry drill 的 pass + block 证据；另逐项审读 PRD ceremony 条件，缺任一项保持阻断。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 产品适用性：N/A；理由：本行定义专业证据与安全阻断条件，不宣布玩家开放；范围：ceremony/readiness。真实 mint、production custody 与公开发布未覆盖。 |
