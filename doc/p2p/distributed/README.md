# P2P 分布式运行时文档

本目录保留分布式运行时的导航边界。四组历史 PRD/design/project 已于 2026-07-28 完成语义回填并退役；当前规范、设计和追踪分别进入 P2P 根三件套，历史原文从 Git 与 GitHub task evidence 追溯。

## 从这里开始

| 需要回答的问题 | 首读专题 | 范围与状态 |
| --- | --- | --- |
| 分布式计算、内容寻址存储、复制与恢复边界是什么？ | `../prd.md` | “分布式运行时、PoS 与复制恢复合同”；不构成部署/readiness。 |
| crate 分层、协议归位与执行接线如何设计？ | `../design.md` | 根设计的网络、共识、存储、同步、执行与观测分层。 |
| PoS 时间控制面和 committed replay 的窄权威在哪里？ | `../../world-runtime/runtime/chain-pos-control-plane.prd.md`、`../consensus/consensus-code-consolidation-to-oasis7-consensus.prd.md` | 不等同完整以太坊信标链或 mainnet finality。 |
| 历史完成态和未来 gap 在哪里？ | `../GitHub Issue / GitHub Project` | 压缩 trace；Phase-C 专用 envelope/topic/driver 仍是 future gap，现有网络探测 gate 不在此缺口内。 |

## 阅读与维护边界

- 本 README 只做 successor 路由，不复制规格、验证命令或项目台账。
- 旧专题的 `[x]` 和历史命令只证明当时局部收口，不证明当前 public-testnet、mainnet、release readiness 或玩家可用。
- 当前 P2P 模块边界、设计与活跃状态以 `doc/p2p/{prd,design}.md` 为准。

## 退休审计

四组历史 triplet 已满足替代真值落盘、调用迁移和活跃引用清零条件后删除。保留本页是为了让旧目录入口稳定落到 successor，而不是继续维护第二套权威。
