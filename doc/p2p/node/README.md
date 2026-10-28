# `p2p/node` 热点子域入口

更新时间: 2026-07-28

## 从这里开始
- 想确认 P2P/DistFS/consensus/execution/observer 的整体闭环、测试层级与 claim boundary：先读 `testing-manual.md#s9a链上大世界状态底座自闭环`；本页只负责 node 子域入口。
- 想确认节点奖励、贡献分、资产与结算口径：先读 `node-contribution-points.prd.md`、`node-redeemable-power-asset.prd.md`，主链 Token bridge 再读 `../token/mainchain-token-allocation-mechanism.prd.md`
- 想确认节点身份引导、复制链路、net stack、signer binding 与 DistFS 节点网络闭环：先读 `node-identity-replication-contract.prd.md`
- 想确认 PoS 时间、slot clock 与控制面对齐：先读 `../../world-runtime/runtime/chain-pos-control-plane.prd.md`
- 想确认 `wasm32/libp2p` 编译约束：先读 `../network/readme-p1-network-production-hardening.prd.md`；想确认 builtin wasm materialization/fallback：先读 `../../world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md`
- 想确认本机 + 2 ECS 三节点的完整监控入口、resource + chain + traffic + wasm 统一证据，以及模块级优化点：先读 `node-triad-operations-observability.prd.md`
- 想精确找某份专题文档，而不是按问题阅读：回到 `../prd.index.md`

## 入口分工
- 当前页只承担 `node/` 子目录 landing page 职责，不复制完整长表。
- node 专题的执行校验、奖励、复制或监控 green 结果只能作为链上大世界状态底座的组件证据；模块级闭环以 S9A 的 `module_required / module_full / integration_required / release_full` 分层为准。
- `../README.md` 是 `p2p` 模块级 landing page，负责跨 `blockchain / distfs / node / observer / token / network / distributed / consensus / viewer-live` 分流。
- `../prd.index.md` 是 `p2p` 模块完整文件级索引，适合已知主题后按文件名查找。

## 高密度提示
- 当前子域属于 `p2p` 模块最高密度热点路径之一；本页的目标是压缩首读路径，而不是按文件数维护专题清单。
- 需要当前文件数量和 inventory 状态时，以仓库根目录运行 `./scripts/doc-inventory-report.sh` 为准；`find` / `git ls-files` 仅作为本地探索辅助，不作为正式 inventory 口径。

## 首读主题簇

### 1. 奖励、资产与结算
- 首读入口:
  - `node-contribution-points.prd.md`
  - `node-redeemable-power-asset.prd.md`
  - `../token/mainchain-token-allocation-mechanism.prd.md`
- 适合问题:
  - 节点奖励怎么计、贡献分如何结算
  - 可赎回 power asset 与治理签名阶段如何拆分
  - 奖励、执行验证与原生交易结算的关系是什么
- 说明: runtime closure 草案仍保留在 [world-runtime integration draft](../../world-runtime/integration/node-contribution-points-runtime-closure.md)，尚未完整吸收到当前专业合同，也不证明当前实现或 readiness；当前节点贡献积分规则见 [`node-contribution-points.prd.md`](node-contribution-points.prd.md)。multi-node closure test、storage-pool、uptime 增量、redeemable power asset 的 audit/signature 增量及 native settlement 专题的有效语义已合并进稳定权威，相关源文件已删除；builtin fallback 转入 WASM pipeline。历史 reward leader/failover 完成态因无当前实现而退役，不构成现行能力。

### 2. 身份、复制、网络与 signer binding
- 首读入口:
  - `node-identity-replication-contract.prd.md`
- 适合问题:
  - 节点 keypair bootstrap、复制链路与 signer binding 的当前边界是什么
  - DistFS 节点复制、network injection 和历史 libp2p migration 的现行合同在哪里
  - 共识 signer binding、复制摄取顺序与恢复硬化需要看哪里

### 3. PoS 时间与控制面对齐
- 首读入口:
  - `../../world-runtime/runtime/chain-pos-control-plane.prd.md`
- 适合问题:
  - slot/epoch 真实时钟驱动的现行口径是什么
  - 槽内 tick phase 和 proposal pacing 怎么理解
  - runtime / launcher / script 控制面参数应该看哪份专题

### 4. WASM 编译与兼容护栏
- 首读入口:
  - `../network/readme-p1-network-production-hardening.prd.md`
  - `../../world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md`
- 适合问题:
  - `wasm32/libp2p` 的编译守卫和限制是什么
  - builtin wasm materialization、hash 校验与 fallback 边界要看哪里

### 5. 三节点监控与运行证据
- 首读入口:
  - `node-triad-operations-observability.prd.md`
- 适合问题:
  - 现在三节点哪个 CPU 高、内存紧不紧、磁盘吃了多少
  - chain status、traffic、wasm 和宿主机资源怎样统一采样
  - 哪个热点更像是 control-plane / replication / consensus / transactions / wasm 自身的问题
  - real-env triad evidence 现在的 canonical 监控命令是什么

## 定向检索边界
- 已知准确文件名时，直接回 `../prd.index.md`；本页不替代完整索引。
- 查找某个奖励阶段、closure test、audit hardening 或 release 说明时，允许直接进相应 supporting spec，但不要把它们重新当作默认首读入口。
- 如果某个主题已经出现“主文档 + 增量子文档”的主从关系，应优先进入主文档，而不是从子文档倒推现行口径。

## 维护约定
- 新增 `node/` 专题后，若改变了默认首读路径，应同步更新本页。
- 本页只维护簇级入口，不维护完整文件清单。
- 若未来 `node/` 内部继续分裂出更高密度簇，再另开簇内治理专题，而不是把本页扩写成长表。
- 本页承接 2026-04-17 路径落位专题形成的长期抽象；一次性 `p2p-node-path-governance` 三件套已退役，实施过程从 git history 与 GitHub task evidence 追溯，不再作为 live 规则入口。
- 只有当本页无法在首屏完成问题分流、主题簇出现相互重叠，或 `node/` 需要物理迁移/专题合并时，才触发新的 bounded 治理任务；普通专题增补继续在本页维护，不复建一次性落位三件套。
