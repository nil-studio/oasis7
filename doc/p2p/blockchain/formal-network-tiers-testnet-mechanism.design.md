# oasis7 正式网络分层与 testnet 机制（设计文档）

- 对应需求文档: `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 1
## 设计目标
- 明确本地、测试、正式三套目标网络的分层模型；历史 `shared_devnet` 只保留 legacy/rehearsal evidence 语义，不再兼任 testnet。
- 提供一个 repo-owned、机器可读的 `network_tier_manifest` skeleton，后续 runtime / liveops / QA 都可以围绕同一字段集接线。
- 明确这轮只做 spec + skeleton，不做 live `public_testnet` / `mainnet` 激活。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [合法 authority profiles](../prd.md#p2p-authority-profiles) | 正式配置须消费同世界合法激活证明，版本有效或声明 active 均不能自授运行权威。 | [正式激活接收目标](#formal-controlled-activation-receiver) | P2P 正式证明与历史；Runtime、launcher、readiness 消费者；QA | 待实现 v3；v1 兼容和 v2 planned 拒绝边界不变，不证明部署或资产资格。 |
| [网络分层验收](formal-network-tiers-testnet-mechanism.prd.md#tier-model-acceptance) | 定义 `local_devnet -> public_testnet -> mainnet` 三层 operator/runtime 模型；`shared_devnet` 只作为 legacy/rehearsal evidence，不作为目标 test 环境或玩家世界。 | [分层模型](#分层模型) | `producer_system_designer` 维护 tier 语义；runtime manifest 消费者按同一分层接线。 | 不把 tier 解释为多个玩家世界，也不以 shared rehearsal 声明 public availability。 |
| [Manifest 字段验收](formal-network-tiers-testnet-mechanism.prd.md#tier-manifest-fields-acceptance) | 分别固定 `public_testnet` 与 `mainnet` manifest 的必需字段及其差异，特别是 reset、faucet、validator admission 和 required gates。 | [Manifest Schema](#manifest-schema) | `producer_system_designer` 维护字段语义；`runtime_engineer` 消费 manifest；主网安全专题提供 `MAINNET-1~4`。 | 本设计不激活 live `public_testnet` 或 mainnet，也不存放私密凭据。 |
| [Skeleton 与手册验收](formal-network-tiers-testnet-mechanism.prd.md#tier-skeleton-manual-acceptance) | 提供 manifest create/validate、smoke、三类 example manifests 和 testing-manual 入口；手册入口是执行索引。 | [Repo-Owned Skeleton](#repo-owned-skeleton) | `runtime_engineer` 维护技术入口；`qa_engineer` 复核测试边界；testing manual 是消费者入口。 | skeleton、example 与本地 smoke 不证明 live 网络或当前 readiness。 产品适用性：N/A；理由：本行只承接 repo-owned 验证工具和手册入口，不定义玩家能力；范围：AC-5/6 的脚本、样例和手册验收，技术与测试义务保留。 |
| [测试价值声明验收](formal-network-tiers-testnet-mechanism.prd.md#tier-test-value-acceptance) | `public_testnet` 的资产与 faucet 仅属 rehearsal/test surface；不得据此作 mainnet 价值承诺。 | [关键规则](#关键规则) | `producer_system_designer` 维护 tier policy；`qa_engineer` / `liveops_community` 复核 claims。 | 不定义产品价格、资产价值或新的外部 claim。 |
| [候选证据与公开边界验收](formal-network-tiers-testnet-mechanism.prd.md#tier-verdict-evidence-acceptance) | 当前 verdict 由 companion project/runbook 与 readiness evidence 汇总；checklist 固定 required lanes、owners、evidence、阻断条件、命令与 claim boundary。 | [与现有专题的关系](#与现有专题的关系) | companion runbook、readiness script、`qa_engineer` 与 `liveops_community` 是外部依赖。 | 本设计不替代 candidate/window 证据，也不授权部署、发布或 readiness claim。 |
| [NFR-P2P-TIER-1 / NFR-P2P-TIER-5](formal-network-tiers-testnet-mechanism.prd.md#tier-manifest-encoding-and-public-fields) | manifest 使用可追踪的 ASCII JSON，并且只包含 public refs、bundle refs 与 policy 字段，不包含密钥、助记词或私密 operator 信息。 | [Manifest Schema](#manifest-schema) | manifest schema/validator 与 example manifests。 | 不设计密钥管理、custody 或 operator credential 注入机制。 产品适用性：N/A；理由：本行仅验证配置序列化与不得保存凭据的专业约束，不定义玩家承诺；范围：NFR-P2P-TIER-1/5；技术责任和验证方法不豁免。 |
| [NFR-P2P-TIER-2 / NFR-P2P-TIER-3](formal-network-tiers-testnet-mechanism.prd.md#tier-policy-semantic-validation) | validate 显式拒绝 tier 语义冲突；mainnet 不得带 resettable、guarded faucet 或 testnet value semantics。 | [关键规则](#关键规则) | `network-tier-manifest.sh` 的语义校验；`qa_engineer` 复核阻断样例。 | 不将 schema validation 等同于运行中网络状态或 mainnet readiness。 |
| [NFR-P2P-TIER-4](formal-network-tiers-testnet-mechanism.prd.md#tier-public-availability-evidence-boundary) | 在 live `public_testnet` 未成立时，不把示例、skeleton 或 `shared_devnet` rehearsal 当作 public availability evidence。 | [与现有专题的关系](#与现有专题的关系) | 当前 public status 由根 `README.md` 管理；证据边界由 formal runbook 和 QA/LiveOps 维护。 | 历史 rehearsal 结论不更新当前公开状态或发布安排。 |

## 当前 schema 与目标解耦边界

v1 `network_tier_manifest.rs` 保留 `local_devnet=preview+ephemeral`、`public_testnet=testnet+resettable`、`mainnet=production+frozen` 兼容校验；下文旧 schema 与组合规则适用于 v1，不代表唯一合法保留组合，也不证明持久单权威已实现。

目标在既有 manifest 中分别表达网络环境/发行阶段、世界生命周期/保留承诺、合法提交 authority profile/激活版本，以及资产价值/faucet/结算资格。

受限 preview 的正式世界可承诺持久保留合法身份、设施、材料、资格及来源；生产结算资格由经济规则与发行条件独立决定，修改 tier 不升值测试奖励，也不能因此清空已承诺世界。

`frozen + preview` 只是需求组合；`token_policy.reset_policy` 不足以代表全世界保留，字段归属、兼容迁移、示例与 readiness 消费者须随实现共同闭合，不能仅放宽枚举后宣称保证成立。

固定 `world_id`、`chain_id`、`genesis_hash` 与可演进软件/runtime manifest/authority 分开；后者只能按同世界历史合法升级。已有保留承诺时沿既有身份接续，隔离 local/dev 不并入长期世界。正式提交 profile、持久性和交接验收见 [P2P 合同](../prd.md#p2p-authority-profiles)；v2 planned 配置不授予运行权威。

### v2 planned 配置接线

承接 [AC-P2P-TIER-PLANNED-001/002](formal-network-tiers-testnet-mechanism.prd.md#p2p-tier-planned-world-policy)：schema validator 按版本分别校验，v2 显式分开 release、world 和 authority policy，token 只拥有经济语义；拒绝混合版本字段与双 reset 真值。v1 消费路径不改变。

配置验证返回 schema-valid 后，运行启动入口另行拒绝 planned/unsupported authority，且该检查必须早于 registry import 或 execution world 写入。readiness 与 exit review 输出未激活 blocker，旧 proof producer 拒绝 v2，不能将其套入现有三节点 threshold 运行路径。status 若呈现声明，应同时呈现 runtime-supported=false；声明不是已生效世界状态。

验证沿用 `network-tier-manifest-smoke.sh` 与 Rust manifest/CLI/launcher 回归：同一 v2 fixture 的合法计划声明、缺 policy、未知 profile/version、live、双 reset、v1 混入 v2 policy，以及旧 lanes 全通过仍不能放行。实际提交证明、fencing、独立故障域持久确认和恢复不在这个 schema 变更的通过结论中。

<a id="formal-controlled-activation-receiver"></a>
### 正式受控权威的 manifest 接收目标

后继正式配置采用新 `oasis7.network_tier_manifest.v3`，消费
[正式受控权威 v1](../design.md#des-p2p-formal-controlled-authority-v1) 的激活与提交证明。
本节规定待实现的版本边界；当前代码仍拒绝 v3，v2 永远保留 planned 含义，
不能将已有 v2 文件中的状态改成 live/active 后启动兼容 prototype。

v3 保留 v2 的 release/world/token 分工，`authority_policy` 要求
`profile=controlled_single_authority`、`profile_version=1`、`activation=proof_required`，
并增加必需的 `formal_authority_ref` 内容 digest 与 `minimum_commit_ref`。
前者定位合法 activation/history 证明，后者限定同世界可接受历史的最低提交锚；
两者都不是可由配置构造的 ActiveAuthority。世界身份必须与 `world_policy.world_id`、
`chain_id`、`runtime_refs.genesis_ref` 和独立认证的 issuer/genesis 一致。
未知或混用版本字段、缺字段、null、错误 profile/version 和证明不匹配均拒绝。

启动与 readiness 分两步处理：先在 registry import、业务状态写入或对外服务之前验证
schema、原证明字节、合法历史与工件；取得证明的方式和大小/缺失处理沿 P2P 主责合同。
合法历史至多支持 verified read-only；当前 epoch/head、双端持久与 signer/storage 隔离
也成立后，才可报告可写就绪。配置、schema-valid、工程 scope 或普通服务签名都不能
替代这两步。已持久的最低 head/epoch 锚优先于较旧配置，不能靠换 manifest 回退历史。
首次 activation 控制记录仍由 P2P 的独立创世授权分支建立；此消费者不生成 issuer 或世界。

launcher、节点、WorldService 和 readiness 必须在同候选支持对应版本后共同接线；
旧消费者拒绝 v3，正式消费者不降级成 v1/工程证明。只读可验证、可写就绪、实际部署和
公开发行分别表达；网络 tier、保留承诺与 token 价值/faucet/结算资格继续独立判断。
本版本不通过 authority 激活重发资产或升格测试奖励，也不解除实际部署暂缓。

## 分层模型
Network tier 是统一持久大世界的运行/验证载体分层，不是玩家可见的多个世界模型。

| Tier | 目标 | 可见性 | 价值语义 | reset/faucet | 当前状态 |
| --- | --- | --- | --- | --- | --- |
| `local_devnet` | 本地开发 / 单人验证 | `local_only` | `preview` | 可重置 / 无正式 faucet | 现有开发态 |
| `public_testnet` | 对外公开 rehearsal / validator 候选演练 | `public` | `testnet` | 可重置 / guarded faucet | 已有 rehearsal / governed-bootstrap 证据，尚非 live candidate |
| `mainnet` | 正式价值网络 | `public` | `production` | 不可重置 / 无 faucet | 本轮只建 skeleton |

`shared_devnet` 不再作为目标 tier；旧文档和证据中出现时，只按 legacy 共享开发网络 / rehearsal evidence 追溯。

## Manifest Schema
- 顶层字段:
  - `schema_version`
  - `tier`
  - `status`
  - `network_id`
  - `chain_id`
- `runtime_refs`:
  - `release_candidate_bundle_ref`
  - `genesis_ref`
  - `bootstrap_peer_ref`
- `endpoint_policy`:
  - `rpc_ref`
  - `explorer_ref`
  - `faucet_ref`
- `validator_policy`:
  - `governance_mode`
  - `validator_admission`
  - `target_validator_count`
  - `allow_observer_nodes`
- `token_policy`:
  - `symbol`
  - `faucet_mode`
  - `reset_policy`
  - `value_semantics`
- `claims_policy`:
  - `allowed_claims`
  - `denied_claims`
- `promotion_policy`:
  - `promote_from`
  - `required_gates`
- `evidence_refs`

## 关键规则
- 规则-1: `shared_devnet` 是 legacy/rehearsal evidence，不是目标 test 环境，也不等于 `public_testnet`。
- 规则-2: `public_testnet` 必须显式具备 public RPC、explorer、guarded faucet 与 reset policy。
- 规则-3: `mainnet` 必须显式具备 `faucet_mode=none`、`reset_policy=frozen`、`value_semantics=production`，并把 `MAINNET-1~4` 固定到 `required_gates`。
- 规则-4: 这轮新增的 manifest validate 既检查字段存在性，也检查 tier 语义组合，避免字段齐全却混用 testnet/mainnet 语义。
- 规则-5: `public_testnet` 且 `validator_policy.allow_observer_nodes=true` 时，observer/read-only fetch 准入采用开放签名策略：任意具备有效 requester 签名的节点可以读取 commit/blob 并完成 observer 同步，不需要手动刷新 validator 的 fetch requester allowlist。
- 规则-6: validator/remote writer 准入仍由 validator set、governance registry 或显式 writer allowlist 控制；observer 自动入网不得授予 gossip write、共识签名或 validator 身份。

## 与现有专题的关系
- `doc/testing/evidence/legacy-shared-devnet-provenance-2026-07-26.md` 保留历史 `shared_devnet/staging/canary` rollback/provenance；它不是 `public_testnet` promotion gate，也不证明 release/mainnet readiness。历史 fallback evidence 边界见当前 companion runbook；实际节点恢复步骤仍依据 governed-bootstrap runbook 和 deployment truth。
- `p2p-mainnet-security-governance-readiness`：
  - 承载 `MAINNET-1~4` 的安全、治理、创世阻断条件。
  - 是 `mainnet manifest` 的 required-gates 专业输入，不是 mainnet 激活结论。
- 根 `README.md` 与产品层公开口径分册：
  - 根 README 负责当前公开状态，`doc/product/player-entry-distribution/release-communications-and-public-claims.prd.md` 负责长期沟通生命周期。
  - 本专题只把 tier 级 claims boundary 接到 manifest schema，不把历史 allowlist/denylist 或 gate 结果升级为当前公开状态。

## Repo-Owned Skeleton
- `scripts/network-tier-manifest.sh`
  - `create`: 生成一份 `network_tier_manifest`。
  - `validate`: 校验字段完整性与 tier 语义组合。
- `scripts/network-tier-manifest-smoke.sh`
  - 验证 create/validate 主路径。
  - 验证目标 example manifests；public-testnet rehearsal 使用独立 canonical template。
- Example manifests:
  - `doc/testing/templates/network-tier-public-testnet-rehearsal.example.json`
  - `doc/testing/templates/network-tier-public-testnet.example.json`
  - `doc/testing/templates/network-tier-mainnet.example.json`

<a id="被否决方案"></a>
## 决策与被否决方案
- 否决-1: 直接把现有 legacy `shared_devnet` 改名为 `testnet`；这会把非目标测试环境误写成目标环境。
  - 原因: 这会把内部 shared access / partial rehearsal 与 public availability 混在一起。
- 否决-2: 先做 live public testnet，再考虑 manifest。
  - 原因: 没有统一 manifest，后续 runtime / QA / liveops 无法共享同一套 tier 真值。
- 否决-3: `mainnet` 继续沿用 testnet schema，不额外加语义校验。
  - 原因: 正式价值网络必须把 `no faucet / frozen reset / mainnet gates` 变成机器可判定约束。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [合法 authority profiles](../prd.md#p2p-authority-profiles) | [正式激活接收目标](#formal-controlled-activation-receiver) | 配置版本不自授 active，合法历史与当前可写条件分开。 | 后继实现沿 [testing-manual.md Network Tiers section](../../../testing-manual.md#network-tiers-validation-evidence) 增加同候选 schema/startup/launcher/readiness 合同回归：v1/v2 原边界、合法 v3 证明、缺/错 proof、混版、低于已存 head、错误 genesis/epoch 与不满足双端条件；断言失败先于 registry/业务写入，历史可读不代签可写。 | 固定候选、manifest/proof 原字节、逐消费者 verdict 与无提前写入断言。 | 现有 smoke/手册是入口基线，尚不覆盖 v3；新协议、独立故障域和正式开放未实施/未验收。 |
| [网络分层验收](formal-network-tiers-testnet-mechanism.prd.md#tier-model-acceptance) | [分层模型](#分层模型) | 在本地 example manifests 中分别核对三层语义和 legacy shared boundary；不得以 shared rehearsal 推导 public availability。 | 对 repository examples 运行 `./scripts/network-tier-manifest-smoke.sh`；命令入口与边界见 [testing-manual.md Network Tiers section](../../../testing-manual.md#network-tiers-validation-evidence)。 | smoke exit/result 与本地 example manifest。 | 不证明 live `public_testnet`、mainnet 或玩家世界状态。 |
| [Manifest 字段验收](formal-network-tiers-testnet-mechanism.prd.md#tier-manifest-fields-acceptance) | [Manifest Schema](#manifest-schema) | 分别验证 public-testnet 与 mainnet 的必需 manifest 字段，并检查 reset/faucet/validator/gate 字段组合。 | 对三类 example manifests 运行 `./scripts/network-tier-manifest-smoke.sh`，并用手册中的 `./scripts/network-tier-manifest.sh validate --manifest <manifest>` 复核；入口见 [testing-manual.md Network Tiers section](../../../testing-manual.md)。 | 命令退出结果和被校验的 manifest。 | 不证明 manifest 指向的 bundle、genesis 或 peers 当前可用。 |
| [Skeleton 与手册验收](formal-network-tiers-testnet-mechanism.prd.md#tier-skeleton-manual-acceptance) | [Repo-Owned Skeleton](#repo-owned-skeleton) | 验证 create/validate、smoke 和三类 example manifest 入口可复用；manual 只索引当前命令。 | 执行 `./scripts/network-tier-manifest-smoke.sh`，并核对 [testing-manual.md Network Tiers section](../../../testing-manual.md) 中的 canonical commands。 | smoke 输出及当前手册命令段。 | 不证明部署、节点健康或 live readiness。 产品适用性：N/A；理由：本行只承接 repo-owned 验证工具和手册入口，不定义玩家能力；范围：AC-5/6 的脚本、样例和手册验收，技术与测试义务保留。 |
| [测试价值声明验收](formal-network-tiers-testnet-mechanism.prd.md#tier-test-value-acceptance) | [关键规则](#关键规则) | 对 public-testnet manifest 的 token/faucet/reset/claims 字段做本地核对，确保 test surface 不被写成 mainnet 价值承诺。 | 依据 [testing-manual.md Network Tiers section](../../../testing-manual.md) 执行 manifest validate，并检查选定 manifest 的 `token_policy` 与 `claims_policy`。 | 被核对 manifest 与 validate 输出。 | 不证明实际外部沟通已发布或任何资产价值。 |
| [候选证据与公开边界验收](formal-network-tiers-testnet-mechanism.prd.md#tier-verdict-evidence-acceptance) | [与现有专题的关系](#与现有专题的关系) | 对选定 candidate 的 manifest 与 required-lane TSV 运行 readiness；只有 current evidence 才能支撑 current verdict。 | 按 [testing-manual.md Network Tiers section](../../../testing-manual.md#network-tiers-validation-evidence) 执行 `./scripts/network-tier-public-testnet-readiness.sh --manifest <manifest> --lanes-tsv <lanes.tsv>`；输入必须属于同一 candidate/window。 | candidate-scoped manifest、lane TSV 与 readiness JSON。 | 示例或历史 rehearsal 输出不证明当前 live state、mainnet 或 public launch。 |
| [NFR-P2P-TIER-1 / NFR-P2P-TIER-5](formal-network-tiers-testnet-mechanism.prd.md#tier-manifest-encoding-and-public-fields) | [Manifest Schema](#manifest-schema) | 检查生成及示例 JSON 的 ASCII 编码和字段范围；不得出现 private keys、mnemonics 或 operator private login information。 | 使用手册中的 manifest validate 命令并人工检查 selected JSON；入口见 [testing-manual.md Network Tiers section](../../../testing-manual.md#network-tiers-validation-evidence)。 | 被检查的 JSON 文件与 validate 输出。 | 不证明密钥管理或 custody 实现安全。 产品适用性：N/A；理由：本行仅验证配置序列化与不得保存凭据的专业约束，不定义玩家承诺；范围：NFR-P2P-TIER-1/5；技术责任和验证方法不豁免。 |
| [NFR-P2P-TIER-2 / NFR-P2P-TIER-3](formal-network-tiers-testnet-mechanism.prd.md#tier-policy-semantic-validation) | [关键规则](#关键规则) | 验证 tier 语义冲突与 mainnet 的 no-reset/no-faucet 负例保持阻断。 | 运行 `./scripts/network-tier-manifest-smoke.sh`，并按 [testing-manual.md Network Tiers section](../../../testing-manual.md) 使用 manifest validate 命令。 | smoke 退出结果及语义冲突样例输出。 | 不证明 mainnet activation 或 live network configuration。 |
| [NFR-P2P-TIER-4](formal-network-tiers-testnet-mechanism.prd.md#tier-public-availability-evidence-boundary) | [与现有专题的关系](#与现有专题的关系) | 人工核对 claims boundary 是否把 `shared_devnet` 及 example/skeleton 状态限制为历史或 rehearsal evidence。 | 核对 [testing-manual.md Network Tiers section](../../../testing-manual.md#network-tiers-validation-evidence) 的 canonical boundary，并以当前 runbook/README 确认状态入口。 | 当前文档引用与对应 candidate/window evidence。 | 不证明外部发布内容或 live availability。 |
