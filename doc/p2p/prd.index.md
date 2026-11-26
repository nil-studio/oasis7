# p2p PRD 文件级索引

审计轮次: 14

更新时间：2026-10-13

## 入口
- 模块 PRD：`doc/p2p/prd.md`
- 模块设计总览：`doc/p2p/design.md`
- 模块标准执行入口：GitHub Issue / GitHub Project

## 首读分流
- 想先回答模块在做什么、主链和 DistFS 的统一边界是什么：先读 `doc/p2p/prd.md`
- 想先回答当前在推进什么、哪些安全/签名/覆盖网络任务仍在推进：查看 GitHub Issue / GitHub Project。
- 想先回答 P2P、DistFS、共识、执行与 observer 如何作为“链上大世界状态底座”单模块自闭环测试：先读 `testing-manual.md#s9a链上大世界状态底座自闭环`
- 想先进入 `node` 热点子域，并按奖励 / 复制 / PoS 时间 / 身份引导 / WASM 编译问题分流：先读 `doc/p2p/node/README.md`
- 想区分 builtin Wasm identity 与共识代码 crate 收敛两个已完成专题：先读 `doc/p2p/consensus/README.md`
- 想先看主链安全、network tier、hosted player access / `hosted_public_join` 接入或托管身份/托管密钥：先读 `doc/p2p/blockchain/README.md`；mixed-topology reachability 再进入 `network/` 子域
- 想核对 HTTP 提交身份、接受后处置、交易状态与 P0 查询：先读[链 HTTP / Explorer 专业需求](blockchain/chain-http-submit-explorer-contract.prd.md)和[系统设计](blockchain/chain-http-submit-explorer-contract.design.md)。
- 想先看 Token 创世分配、交易授权、治理分发、理想化交易或服务额度 bridge：先读 `doc/p2p/token/README.md`
- 想继续按子域或文件名下钻：使用下方热点子域导航，再跳到对应清单区域

## 热点子域导航
| 子域 | 适合回答的问题 |
| --- | --- |
| `node/` | 节点奖励、身份、复制、PoS 时间基线与执行验证 |
| `distfs/` | DistFS 生产加固、路径索引、自愈与 runtime/bridge 集成；默认先读 `doc/p2p/distfs/README.md` |
| `blockchain/` | 链 HTTP / Explorer、主链安全、network tier、mainnet readiness、signer custody、hosted player access；默认先读 `doc/p2p/blockchain/README.md` |
| `observer/` | 观察者同步模式、指标与可观测性；默认先读 `doc/p2p/observer/README.md` |
| `token/` | 创世分配、签名授权、治理分发与流通边界；默认先读 `doc/p2p/token/README.md` |
| `network/` | reachability、mobile light client、runtime bridge 与 mixed-topology |
| `distributed/` | 分布式 runtime / consensus / hard split 路线；首读 `doc/p2p/distributed/README.md` |
| `viewer-live/` | Viewer LLM 默认与 observer/debug-only `--no-llm` 边界；默认先读 `doc/p2p/viewer-live/oasis7-viewer-live-decision-mode.prd.md` |
| `consensus/` | 共识实现与内建 wasm 身份口径；默认先读 `doc/p2p/consensus/README.md` |

## 当前补充阅读面
- `doc/engineering/governance/environment-lanes-and-inventory-2026-05-29.md`：项目级 test/prod 环境分层、hosted-login 云上清单与 `testnet/mainnet` claim boundary。
- `doc/p2p/node/README.md`：`node/` 热点子域 landing page，按奖励、复制、PoS 时间、身份引导与 WASM 编译分流读者。
- `doc/p2p/blockchain/README.md`：`blockchain/` 高密度子域 landing page，按现行 network tier、mainnet-grade security、hosted player identity 与历史 P2PFS hardening 分流读者。
- `doc/p2p/blockchain/hosted-player-access-operator-runbook.md`：hosted player join 分享、信任面隔离、session 处置、claim freeze 与最小披露的稳定 operator 入口；不定义产品 claim 或 custody 实现。
- `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.runbook.md`：formal `public_testnet` 从规格骨架进入候选状态前的 companion checklist/runbook。
- `doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md`：formal `public_testnet` governed bootstrap operator path，定义四节点重建输入、deployment truth、hard rules 与 evidence 闭包；不表示 `public_testnet` 已 live 或 ready。
- `testing-manual.md#s9a链上大世界状态底座自闭环`：P2P transport、DistFS/blob closure、replication/gap sync/state sync、consensus/finality、execution record/receipt、observer/ops 與 API/viewer projection 的模块自闭环测试入口；用于区分 `module_required/module_full/integration_required/release_full` claim boundary。
- `doc/testing/evidence/legacy-shared-devnet-provenance-2026-07-26.md`：已退役 `shared_devnet` rehearsal 的历史 rollback/provenance 入口；不等于 formal `public_testnet` / `mainnet` readiness。
- `doc/testing/evidence/README.md`：QA evidence landing page，负责 public-testnet readiness evidence / claims-boundary / mixed-topology / legacy shared-network 证据的当前入口与归档边界。
- `doc/p2p/token/README.md`：Token 专题簇 landing page，按创世冻结、分配机制、授权、目标态与 quota bridge 分流；再按需进入理想交易设计或 quota bridge runbook。
- [`doc/p2p/consensus/bft-minimal-signer-tcb.design.md`](consensus/bft-minimal-signer-tcb.design.md)：BFT 受保护签名器 `< 4 KiB` 最小可信基（Minimal TCB）与私有保留存储/流量账/轮次时钟解耦的中期目标设计；不替代现行 `wire=3`（`#5518`）交付合同。

## 默认阅读面边界
- 本页首屏只负责分流，不再要求读者从第一行开始顺扫完整长表。
- README 不再平铺“近期专题”；完整清单继续保留在下方，用于精确文件名检索和互链可达性。
- `node/README.md`、`blockchain/README.md`、`distfs/README.md`、`observer/README.md`、`token/README.md`、`viewer-live/README.md` 与 `consensus/README.md` 负责专题簇的首读分流；完整长表继续由本页保留。
- runbook、release 补充材料与历史说明继续保留可检索性，但不进入模块默认首屏。

## 状态语义
- `draft`：待评审方案；纳入索引只保证可检索，不代表已采纳、已实现或公开可用。
- `active`：当前执行真值以 GitHub Issue / GitHub Project 为准；仓库文档不维护任务状态段。
- `canonical`：模块规格、设计总览、热点子域入口、当前补充阅读面和专题主文档；这些文档可长期保留在默认可达路径中。
- `completed`：已完成但仍需按文件名可达的专题三件套；保留在下方检索清单，不自动进入默认首读面。
- `historical` / `evidence-snapshot`：历史审计清单、旧轮次证据、release 补充材料与 legacy runbook；只作为追溯证据或兼容入口，不作为当前执行状态或可删除依据。

## 覆盖规则（ROUND-005 统一）
- 纳入规则：纳入 `doc/p2p/**` 下所有 `*.prd.md` 及其同名 `*.design.md`；任务映射保留在 GitHub。
- 当前补充阅读面：`*.runbook.md` 与仍被当前模块 PRD / 项目态直接引用的 supporting spec，可在“当前补充阅读面”区定向列出，但不并入下方三件套长表。
- 排除规则：不纳入 `doc/devlog/**` 与非 PRD 配对文档（如 `*.release.md` 补充材料）。
- 历史入口：根目录旧 PRD 跳转文件已删除；旧版本从 Git history 与 GitHub task evidence 追溯，不作为当前入口。
- 当前规格入口为本目录 `prd.md`；任务状态与执行证据读对应 GitHub Issue / Project。

## 折叠阅读层（主从/增量组）
| 子域 | 默认先读 | 折叠的增量/追溯入口 |
| --- | --- | --- |
| `distfs/` production hardening | `doc/p2p/distfs/README.md` -> `doc/p2p/distfs/distfs-production-hardening.prd.md` | 历史 phase1~phase9 已语义收敛；原 phase 身份仅在历史 audit/review 文字和 Git history 中保留，不作为当前入口或 readiness 依据 |
| `distfs/` distributed resilience | `doc/p2p/distfs/README.md` -> `doc/p2p/distfs/distfs-distributed-resilience.prd.md` | 异构 provider 选择、无单机完整依赖、自愈控制/轮询与受限 NodeRuntime 接线统一收敛；NodeRuntime 仍仅是依赖齐备时运行、失败不阻断 tick 的增量边界 |
| `observer/` sync source | `doc/p2p/observer/README.md` -> `doc/p2p/observer/observer-sync-source-mode.prd.md` | DHT 组合链路已吸收进默认权威 |
| `observer/` sync mode metrics | `doc/p2p/observer/README.md` -> `doc/p2p/observer/observer-sync-mode-runtime-metrics.prd.md` | runtime bridge 与 observability 已吸收进默认权威 |
| `viewer-live/` decision mode | `doc/p2p/viewer-live/README.md` -> `doc/p2p/viewer-live/oasis7-viewer-live-decision-mode.prd.md` | 两组 2026-02 源三件套已语义回填并退役；历史变更从 Git 与 GitHub task evidence 追溯 |

## 完整专题检索清单（按文件名精确检索）
| 专题 PRD | 专题设计文档 | 任务追溯 |
| --- | --- | --- |
| `doc/p2p/blockchain/p2p-blockchain-p2pfs-hardening.prd.md` | `doc/p2p/blockchain/p2p-blockchain-p2pfs-hardening.design.md` | GitHub Issue / GitHub Project |
| [chain-http-submit-explorer-contract.prd.md](blockchain/chain-http-submit-explorer-contract.prd.md) | [chain-http-submit-explorer-contract.design.md](blockchain/chain-http-submit-explorer-contract.design.md) | [历史 Issue #4450：文档承接后关闭，非实现完成](https://github.com/nil-studio/oasis7/issues/4450#issuecomment-6126251006) |
| `doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.prd.md` | `doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md` | `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.design.md` | GitHub Issue / GitHub Project |
| `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.prd.md` | `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md` | `doc/p2p/blockchain/hosted-public-join-managed-identity-custody.design.md` | GitHub Issue / GitHub Project |
| [hosted-public-join-key-based-onboarding.prd.md](blockchain/hosted-public-join-key-based-onboarding.prd.md) | [hosted-public-join-key-based-onboarding.design.md](blockchain/hosted-public-join-key-based-onboarding.design.md) | P0 账号接入合同已采用；P1 原生密钥未发布；实现、部署与验收按 Git / PR / 实际 CI 与评审记录分别核验 |
| `doc/p2p/consensus/builtin-wasm-identity-consensus.prd.md` | `doc/p2p/consensus/builtin-wasm-identity-consensus.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/consensus/consensus-code-consolidation-to-oasis7-consensus.prd.md` | `doc/p2p/consensus/consensus-code-consolidation-to-oasis7-consensus.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/distfs/distfs-feedback-ledger-and-replication.prd.md` | `doc/p2p/distfs/distfs-feedback-ledger-and-replication.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/distfs/distfs-distributed-resilience.prd.md` | `doc/p2p/distfs/distfs-distributed-resilience.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/distfs/distfs-production-hardening.prd.md` | `doc/p2p/distfs/distfs-production-hardening.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/network/p2p-mobile-light-client-authoritative-state.prd.md` | `doc/p2p/network/p2p-mobile-light-client-authoritative-state.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/network/mainnet-private-reachability-architecture.prd.md` | `doc/p2p/network/mainnet-private-reachability-architecture.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/network/readme-p1-network-production-hardening.prd.md` | `doc/p2p/network/readme-p1-network-production-hardening.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/node/node-contribution-points.prd.md` | `doc/p2p/node/node-contribution-points.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/node/node-identity-replication-contract.prd.md` | `doc/p2p/node/node-identity-replication-contract.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/node/node-redeemable-power-asset.prd.md` | `doc/p2p/node/node-redeemable-power-asset.design.md` | GitHub Issue / GitHub Project |
| `doc/world-runtime/runtime/chain-pos-control-plane.prd.md` | `doc/world-runtime/runtime/chain-pos-control-plane.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/node/node-triad-operations-observability.prd.md` | `doc/p2p/node/node-triad-operations-observability.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/observer/observer-sync-mode-runtime-metrics.prd.md` | `doc/p2p/observer/observer-sync-mode-runtime-metrics.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/observer/observer-sync-source-mode.prd.md` | `doc/p2p/observer/observer-sync-source-mode.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/token/mainchain-token-allocation-mechanism.prd.md` | `doc/p2p/token/mainchain-token-allocation-mechanism.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/token/mainchain-token-ideal-transaction.prd.md` | `doc/p2p/token/mainchain-token-ideal-transaction.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md` | `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/token/mainchain-token-newapi-quota-bridge.prd.md` | `doc/p2p/token/mainchain-token-newapi-quota-bridge.design.md` | GitHub Issue / GitHub Project |
| `doc/p2p/viewer-live/oasis7-viewer-live-decision-mode.prd.md` | `doc/p2p/viewer-live/oasis7-viewer-live-decision-mode.design.md` | GitHub Issue / GitHub Project |

## 发布说明文档（release，补充材料）
| 发布说明 | 对应专题 |
| --- | --- |
| `doc/p2p/node/node-redeemable-power-asset-audit-hardening.release.md` | `doc/p2p/node/node-redeemable-power-asset.prd.md`（历史 AHA provenance） |

## 说明
- 本索引用于保证模块专题文档在根入口文档树中可达。
- 文档配对规则：`*.prd.md` 与同名 `*.design.md`；可变任务历史和状态由 GitHub Issue / GitHub Project 承担。
- `*.release.md` 为发布补充材料，不参与 PRD/design 配对规则。
- `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism` 另有执行 companion：`doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.runbook.md`。
- `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism` 另有 governed bootstrap operator path：`doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md`。
- `doc/p2p/token/mainchain-token-newapi-quota-bridge` 另有执行 companion：`doc/p2p/token/mainchain-token-newapi-quota-bridge.runbook.md`。
- Observer source 专业权威：DHT 增量专题（`PRD-P2P-MIG-109-001`）已合并进 `observer-sync-source-mode.{prd,design}.md`，原 ID 由继任专题 PRD-ID 保留，任务历史写入 GitHub，源文件已删除。
- Ideal transaction 专业权威：2026-06-08 理想交易升级三件套已合并进 `mainchain-token-ideal-transaction.{prd,design}.md`；`PRD-P2P-ITX-001/002/003` 由继任专题 PRD-ID 保留，任务历史写入 GitHub，历史过程只从 Git 与 GitHub task evidence 追溯。
- Observer metrics 专业权威：统计桥接（`PRD-P2P-MIG-106-001`）与策略可观测性（`PRD-P2P-MIG-107-001`）已合并进 `observer-sync-mode-runtime-metrics.{prd,design}.md`，原 ID 由继任专题 PRD-ID 保留，任务历史写入 GitHub，源文件已删除。
- NodePoints 专业权威：当前规则见 [`node-contribution-points.prd.md`](node/node-contribution-points.prd.md) 及其设计文档。runtime closure（`PRD-P2P-MIG-090-001`）仍保留为 [`world-runtime integration draft`](../world-runtime/integration/node-contribution-points-runtime-closure.md)，尚未完整吸收到当前专业合同，也不证明当前实现或 readiness；multi-node closure test（`PRD-P2P-MIG-089-001`）的有效语义已合并进当前三件套，原源文件已删除。原 ID 映射由继任专题 PRD-ID 保留，任务历史写入 GitHub，历史过程从 Git history 与 GitHub task evidence 追溯。
- Node reward 专业权威：storage pool（`PRD-P2P-MIG-102-001`）与 uptime reward（`PRD-P2P-MIG-103-001`）已合并进 `node-contribution-points.*`；native settlement 由 Token allocation authority 承接；builtin materialization 由 WASM deterministic-build pipeline 承接。旧 reward leader/failover 完成态无当前实现，按根 PRD 负向边界退役。15 个源文件已删除，历史过程只从 Git 与 GitHub task evidence 追溯。
- Node network 专业权威：net stack unification（`PRD-P2P-MIG-105-001`）与 wasm32/libp2p compile guard（`PRD-P2P-MIG-104-001`）的有效语义已合并进 `doc/p2p/network/readme-p1-network-production-hardening.{prd,design}.md`，原 ID 映射由继任专题 PRD-ID 保留，任务历史写入 GitHub，源文件已删除。
- Redeemable power asset 专业权威：审计加固（`PRD-P2P-MIG-096-001`）与签名治理（`PRD-P2P-MIG-097-001`）已合并进 `node-redeemable-power-asset.{prd,design}.md`，原 ID 由继任专题 PRD-ID 保留，任务历史写入 GitHub，源文件已删除；历史 AHA release companion 保留并重定向到当前权威。
- DistFS distributed resilience：`distfs-distributed-resilience` 是异构 provider 兼容/选择、无单机完整依赖、分布覆盖、自愈控制/轮询与 NodeRuntime 接线的唯一主入口。NodeRuntime 接线仍受“依赖齐备才运行、缺依赖跳过、单轮失败不阻断 tick”约束，不能被误述为全局自治恢复或 readiness 结论。
- DistFS production hardening：`distfs-production-hardening` 是 Phase 1-9 的唯一当前专业权威；历史 phase 文件名只保留为 audit/review provenance 和 Git history，后续删除必须保留该身份而不得改写为重复的当前路径。
- DistFS feedback ledger and replication：`distfs-feedback-ledger-and-replication` 是公开 feedback 的 append-only 账本、签名/nonce、announce/fetch 复制和 NodeRuntime 有界接线的唯一当前专业 authority。三个 2026-03 feedback 源三件套已删除；其追溯仅保留在 Git 与 `.pm` task evidence，且不构成 current entry、consensus/finality 或 readiness 依据。
- DistFS 基础接口与 builtin artifact：早期 builtin storage/API、标准 FileStore、runtime path-index、observer/bootstrap 与 net runtime-bridge 六组完成态三件套已语义合并并删除。FileStore/current blob 回收由 `distfs-production-hardening` 承接；builtin artifact hydrate/hash/loader 由 `doc/world-runtime/wasm/wasm-deterministic-build-pipeline.*` 承接；dormant path-index 与空 `runtime_bridge` compatibility feature 的负向边界由 `doc/p2p/{prd,design}.md` 承接。历史过程只从 Git 与 GitHub task evidence 追溯。
