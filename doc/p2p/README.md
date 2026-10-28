# p2p 文档索引

产品层归属：`p2p` 是“权威世界基础设施”的分布式状态专业域权威，不形成并列产品入口。产品总导航见 `doc/product/README.md`，组合承诺见 `doc/product/world-infrastructure/prd.md`。

审计轮次: 14

## 从这里开始
- 想先理解 P2P / 主链 / DistFS / 节点奖励的总边界：`doc/p2p/prd.md`
- 想看当前活跃任务、阻断与最新完成项：GitHub Issue / GitHub Project。
- 想先进入 `node` 热点子域，并按奖励 / 复制 / PoS 时间 / 身份引导 / WASM 编译问题分流：`doc/p2p/node/README.md`
- 想先进入 phase / 增量文档密集的子域：先读 `doc/p2p/distfs/README.md`、`doc/p2p/observer/README.md`、`doc/p2p/distributed/README.md` 或 `doc/p2p/viewer-live/README.md`，再进入具体专题。
- 想区分 builtin Wasm identity 与共识代码 crate 收敛两个已完成专题：先读 `doc/p2p/consensus/README.md`。
- 想先进入主链安全、network tier、signer custody 或 `hosted_public_join` 玩家身份专题：`doc/p2p/blockchain/README.md`
- 想核对 HTTP 接纳、action ID、批次处置与 Explorer 查询一致性：进入[链 HTTP / Explorer 合同](blockchain/chain-http-submit-explorer-contract.prd.md)及其[系统设计](blockchain/chain-http-submit-explorer-contract.design.md)；Launcher 表现层另由对应 Explorer 专题负责。
- 想执行 formal `public_testnet` readiness 或从 deployment truth 重建 validator pair：分别进入 `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.runbook.md` 与 `doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md`；两者都是专业 operator authority，不升级 public-launch 或 mainnet claim。
- 想先看“没有公网 IP 也要成为正式节点”的主链级覆盖网络目标态：`doc/p2p/network/mainnet-private-reachability-architecture.prd.md`
- 想先看 P2P/DistFS/consensus/execution/observer 如何作为“链上大世界状态底座”单模块自闭环测试：`testing-manual.md#s9a链上大世界状态底座自闭环`
- 想先看 Token 分配、治理签名、理想化交易模型或 `OC -> LetAI Run OpenAPI quota` bridge：先读 `doc/p2p/token/README.md`；生产 signer 外部化再进入 `doc/p2p/blockchain/README.md`。
- 当前链上代币的正式产品名固定为“绿洲币 / Oasis Coin”；当前 runtime symbol/ticker 为 `OC`，公钥派生账户前缀为 `oc:pk:`；当前创世 `initial_supply` 已冻结为 `10,000,000,000 OC`。
- 想按子域或文件名继续下钻，而不是从长表里逐行找：`doc/p2p/prd.index.md`

## 入口
- PRD: `doc/p2p/prd.md`
- 设计总览: `doc/p2p/design.md`
- 标准执行入口: GitHub Issue / GitHub Project
- 文件级索引: `doc/p2p/prd.index.md`

## 入口分工
- `README.md` 只承担 landing page 职责：帮助读者从“总边界 / 当前执行 / 高频入口 / 精确索引”中选择入口，不重复长表索引内容。
- `prd.md` 是模块权威规格入口，适合先理解主链、共识、DistFS、节点、token 与 hosted player access 的统一边界。
- GitHub Issue / GitHub Project 是唯一的执行台账与历史证据入口；PRD、design、runbook 和 evidence 只保留稳定的专业合同与可复现操作证据。
- `node/README.md` 是当前最高密度热点子域 `node/` 的 canonical 入口，适合先按“奖励 / 复制 / PoS 时间 / 身份引导 / WASM 编译”分流，再进入具体专题。
- `distfs/README.md`、`observer/README.md`、`token/README.md`、`distributed/README.md` 与 `viewer-live/README.md` 是专题簇的折叠入口，适合先确认主题边界，再按 delta 或历史变更追溯。
- `prd.index.md` 是精确检索索引，适合已知专题名后按文件名直达，不适合作为第一次进入 p2p 模块时的首读入口。
- 高频专题承担主题真值：`p2p-mainnet-*` 负责主链安全与 readiness；`mainnet-private-reachability-architecture` 负责 mixed-topology 覆盖网络目标态；`p2p-hosted-world-player-access-and-session-auth` 负责 hosted player entry / hosted access 与会话鉴权（旧专题名保留作 trace，不表示多个玩家世界）；`hosted-public-join-managed-identity-custody` 负责邮箱 hosted login、托管 player signer 与自托管升级边界；token / signer 系列专题负责分配、签名交易与治理签名外部化。

## 活跃阅读面边界
- 当前页只保留 `what / where / next / risk` 所需入口，不再直接罗列近期专题长名单。
- 高频当前入口保留在 `prd.md`、GitHub Issue / GitHub Project、`node/README.md`、上述少量主题真值专题与 `prd.index.md`。
- 执行细节、runbook、release 补充材料与历史背景，改为从 `prd.index.md` 的定向检索入口进入。

## 模块职责
- 维护 P2P、共识、DistFS、节点奖励与网络桥接等核心链路口径。
- 维护“链上大世界状态底座”口径：P2P transport 只是传输层，模块自闭环必须同时覆盖 DistFS/blob closure、replication/gap sync/state sync、consensus/finality、execution record/receipt、observer/ops 与 API/viewer projection 的 claim boundary。
- 汇总 blockchain / distfs / node / observer / token / viewer-live / consensus / distributed / network 九类专题。
- 承接跨 runtime、launcher、viewer-live 的分布式运行与发布约束收口。
- 承接非全公网 mixed-topology、sentry/relay、overlay reachability 与多链型数据面适配的框架层口径。
- 承接 hosted player entry / hosted access、网页会话鉴权、public/control/signer 平面边界等跨模块 Web/P2P 口径。

## 主题目录
- `distfs/`：DistFS 设计与稳定性加固；默认先读 `distfs/README.md`。
- `node/`：节点能力、奖励、身份与复制链路；默认先读 `node/README.md`。
- `observer/`：观察者同步模式与可观测性；默认先读 `observer/README.md`。
- `blockchain/`：主链安全、network tier、hosted player identity 与 P2PFS 硬化；默认先读 `blockchain/README.md`。
- `token/`：主链 token 分配、正式命名、创世分桶、低流通、治理分发与 `OC -> LetAI Run OpenAPI` 服务额度桥接方案；默认先读 `token/README.md`。
- `viewer-live/`：已完成的 Viewer LLM 默认值与显式 observer/debug 回退变更；默认先读 `viewer-live/README.md`。
- `consensus/`：共识相关专题；默认先读 `consensus/README.md`。
- `distributed/`：分布式运行时专题；默认先读 `distributed/README.md`。
- `network/`：网络桥接专题。

## 热点子域导航
- `node/`：节点身份、奖励、复制、PoS 时间基线与执行验证；当前已补 `node/README.md` 作为热点子域入口。
- `distfs/`：DistFS 生产加固、路径索引、自愈与 bridge 集成；phase 组先从 `distfs/README.md` 折叠进入。
- `blockchain/`：主链安全、network tier、signer custody、hosted player access 与 release train；默认先读 `blockchain/README.md`。
- `observer/`：观察者同步模式、指标与可观测性；同步源增量组先从 `observer/README.md` 折叠进入。
- `token/`：创世分配、签名授权、治理分发与流通边界；默认先读 `token/README.md`。
- `network/`：reachability、mobile light client、runtime bridge 与 mixed-topology 约束。
- `distributed/`：分布式 runtime / consensus / hard split 路线；默认先读 `distributed/README.md`。
- `viewer-live/`：已完成 CLI 变更的追溯入口；现行观察服务边界先读 `viewer-live/README.md`，再回链模块 PRD 与 Viewer 手册。
- `consensus/`：共识实现与内建 wasm 身份口径；默认先读 `consensus/README.md`。

## 高密度提示
- 本页不维护容易漂移的文件数量或状态快照；当前模块库存、热点子目录与治理状态统一以 `./scripts/doc-inventory-report.sh` 为准。
- 子域数量只用于选择下一轮 focused follow-up；不要在本页扩展全量专题长表。
- `find` / `git ls-files` 只可作为本地探索辅助；它们不携带 inventory 状态，且可能因统计口径不同与正式报告不一致。
- 需要完整专题检索清单时，进入 `doc/p2p/prd.index.md`；进入 `node/`、`blockchain/`、`distfs/`、`observer/`、`token/` 时，先读对应子域 README 再继续下钻；需要 runbook / release 补充材料时，再按具体专题路径进入。

## 共享约定
- 模块根入口、专题落位与 README/legacy redirect 的共享规则统一以 `doc/engineering/doc-governance/doc-structure-standard.design.md` 为准。
- 模块行为、默认参数或跨模块分布式口径变化时，更新 `doc/p2p/prd.md` 和相关专题 design/runbook/evidence；任务状态、执行日志和可变历史只写入 GitHub Issue / GitHub Project。高频入口变化时，再同步回写本目录“从这里开始”、`doc/p2p/prd.index.md` 与相关热点子域入口（例如 `doc/p2p/node/README.md`）。
