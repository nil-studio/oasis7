# P2P Token 文档导航

本目录收敛主链 Token 的创世分配、交易授权、治理分发、理想化交易目标态和服务额度 bridge。首次阅读先按要解决的问题选择一组专题；不要把历史完成状态或目标态草案误读成当前发行、公开流通或 mainnet readiness 结论。

## 从这里开始

| 需要回答的问题 | 首读专题 | 边界 |
| --- | --- | --- |
| 当前 `Oasis Coin / 绿洲币` 的创世总量、低流通、早期贡献奖励和 freeze gate 是什么？ | `mainchain-token-genesis-and-contributor-reward.prd.md` | 当前创世与贡献奖励口径；实际地址绑定、mint 与 release readiness 仍以对应 project、freeze sheet 和 P2P 模块状态为准。 |
| 创世 bucket、controller slot、签名策略与尚待绑定项在哪里追溯？ | `mainchain-token-genesis-freeze-sheet.md` | formal freeze companion；不是已完成 production mint 的声明。 |
| 已有 runtime 的 Token 分配、治理 bridge 与 release 语义如何演进？ | `mainchain-token-allocation-mechanism.prd.md` | 已完成的基础分配机制与 release companion；不替代当前创世 freeze 口径。 |
| 已实现的地址绑定、治理 gate 与 treasury 分发语义在哪里？ | `mainchain-token-allocation-mechanism.prd.md` | 当前 runtime 内部 Token 分配 authority；不等于外部钱包、custody、公开分发或 release readiness。 |
| 签名交易授权与托管/自托管边界在哪里？ | `../blockchain/p2p-mainnet-security-governance-readiness.prd.md` | 当前 signed transfer contract；生产 signer custody 的模块级边界仍回到 `doc/p2p/blockchain/README.md`。 |
| 主链 Token 的 signed transaction metadata 与 Phase 2+ 边界是什么？ | `mainchain-token-ideal-transaction.prd.md` | 当前 Phase 1 metadata-only contract；不是实际 fee debit、sponsor 或 priority execution。 |
| `OC -> LetAI Run OpenAPI` quota bridge 如何部署、演练与回滚？ | `mainchain-token-newapi-quota-bridge.prd.md`，再读同名 `.runbook.md` | [后续开发 hold，非主开发路径](mainchain-token-newapi-quota-bridge.prd.md#letai-development-hold)；保留既有规格与 operator companion，不替代服务运行手册或免除未决订单义务。 |

## 阅读与维护边界

- 每组 `*.prd.md`、`*.design.md`、GitHub Issue / GitHub Project 分别保存规格、设计和执行证据；本页只承担首次分流，不复制其参数、命令或状态台账。
- 当前 P2P 模块总边界与活跃执行状态分别回到 `doc/p2p/prd.md` 与 GitHub Issue / GitHub Project；按精确文件名追溯使用 `doc/p2p/prd.index.md`。
- 历史 release 仍可能有模块、测试或审计互链，不能仅因日期较早或已完成而删除。理想交易的 2026-06-08 源三件套已由 `mainchain-token-ideal-transaction.{prd,design}.md` 吸收；历史过程从 Git 与 GitHub task evidence 追溯，不构成当前 authority。
- 新增 Token 专题时，先更新本页的分流/边界，再保留 `doc/p2p/prd.index.md` 的精确 triplet 行；共享目录规则以 `doc/engineering/doc-governance/doc-structure-standard.design.md` 为准。
