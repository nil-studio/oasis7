# game PRD 文件级索引

区域、治理与工业玩法阅读顺序：先从 `doc/product/README.md` 选择“世界规则与玩法系统”，再读对应产品专题；区域 charter、tenure 与公共融资下钻到 `PRD-GAME-017`，普通治理主体与 OC 权利边界下钻到 `PRD-GAME-019`，可编程区域设施下钻到 `PRD-GAME-016`，工业创建、跨区市场与结算下钻到 `PRD-GAME-018`。这些 game 专题分别拥有各自声明范围，不形成第五产品模块或并列产品入口。
审计轮次: 12

更新时间：2026-07-06

## 入口
- 模块 PRD：`doc/game/prd.md`
- 模块设计总览：`doc/game/design.md`
- gameplay 子域入口：`doc/game/gameplay/README.md`
- 核心玩法骨架专题：`doc/game/gameplay/gameplay-top-level-design.prd.md`（活跃基线与路由先看 `doc/game/prd.md`）

## 首读分流
- game 模块当前目标态与完成定义：读 `doc/game/prd.md`
- 当前推进事项、阻断与下一步：查看对应 GitHub Project task 与 issue evidence comments。
- gameplay 热点子域导览（无需先读完整文档长表）：读 `doc/game/gameplay/README.md`
- 核心玩法骨架：先读 `doc/game/prd.md` 确认活跃基线与路由，再读 `doc/game/gameplay/gameplay-top-level-design.prd.md`
- 首局与持续游玩的产品承诺：先读 `doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md`；exact gameplay 合同读 `doc/game/gameplay/gameplay-top-level-design.prd.md`，当前 verdict 由同候选 PR、实际验证记录与 `doc/testing/evidence/` 确认。
- 间接控制下的玩家控制感：先读 `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md` 与同名 product design；跨 runtime、Agent、Viewer/API 与 QA 的系统边界读 `doc/world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md`
- 成熟世界小玩家不必立刻依附 major power、仍能形成 leverage 的路线：先读 `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md`，再读 `doc/game/gameplay/gameplay-mature-world-progression-contract.prd.md`
- “1cm 物理世界”与当前非 Minecraft 式逐块玩法的边界：先读 `doc/product/world-rules-core-gameplay/prd.md` 的产品承诺，再读 `doc/game/gameplay/gameplay-top-level-design.prd.md` 的玩法合同
- 可编程区域设施作为中后期区域专业化能力的落地边界（非自由建造或任意 WASM 上传）：先读 `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.prd.md`
- 区域 charter、tenure 与公共融资的有界、可恢复且不封锁独立路线的玩家循环：先读 `doc/product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md`，再读 `doc/game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md`
- 普通治理主体聚合、委托、纠错与 OC 转让不可旁路事项权利的边界：先读 `doc/product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md` 的 `GCB-004/005`，再读 `doc/game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md`
- 访问模式、受控试玩与 release readiness：读 `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`；当前执行状态由 Git、PR、实际 CI 记录与对应 evidence 确认。
- 按文件名、专题或补充材料继续检索：使用下方热点子域导航与补充入口；当前文件库存统一以 `./scripts/doc-inventory-report.sh` 为准，本页不维护容易漂移的数量快照

## 热点子域导航
| 子域 | 适合回答的问题 |
| --- | --- |
| `gameplay/` 正式专题 | 核心玩法骨架、留存修复、preview/beta gate、claim economy、治理、agency 合同、mature-world 小玩家承接与可编程区域设施 |
| `gameplay/` 补充材料 | runbook、evidence、checklist 与跨角色执行留痕 |
| 模块根入口 | 模块目标态、执行台账、设计总览与文件级精确检索 |

## 活跃补充文档
- `doc/game/gameplay/README.md`：`gameplay/` 热点子域 landing page，适合先做簇级分流，再决定进入玩法骨架、留存、agency、preview/beta gate 或 economy/claim 专题。
- `doc/game/gameplay/gameplay-top-level-design.prd.md`：核心玩法骨架的专题入口；活跃基线与路由先看 `doc/game/prd.md`。
- `doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md`：首局、后引导、首次持续能力与失败恢复的产品承诺；专业玩法合同见 `doc/game/gameplay/gameplay-top-level-design.prd.md`，执行状态由对应 PR、实际验证记录与 `doc/testing/evidence/` 确认。
- `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md`：间接控制下的 accepted intent、主因果、打断重排与续玩恢复合同主入口。
- `doc/game/gameplay/gameplay-industrial-backpressure-recovery-contract.prd.md`：下游 stage/edge/buffer/terminal 容量不足时，上游暂停、降载、持有、合法改道、释放与等待的玩家恢复闭环。
- `doc/game/gameplay/gameplay-industrial-demand-change-disposition-contract.prd.md`：需求满足、取消、到期、未知或缩减后，accepted/WIP/in-transit/buffer-held/terminal-pending 既有工作的玩家处置闭环。
- `doc/game/gameplay/gameplay-industrial-maintenance-planned-downtime-contract.prd.md`：维护真值、计划停机、带风险运行、既有工作处置与 current/target 证据切线。
- `doc/game/gameplay/gameplay-industrial-recipe-lifecycle-decision-contract.prd.md`：配方六态的玩家选择、准入/退役、旧工作处置、successor 与 current/target 证据切线。
- `doc/game/gameplay/gameplay-industrial-raw-material-qualification-substitution-contract.prd.md`：原材料适用性失败后的换源、合法替代、换配方、因果隔离与恢复闭环。
- `doc/game/gameplay/gameplay-industrial-pipeline-service-window-contract.prd.md`：流水线 conditional service window、lease/expiry、hard/soft 迟到与电力连续性恢复合同。
- `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md`：mature-world 小玩家承接、受保护 first win、专业化与局部影响力的产品主入口；详细玩法合同见 `doc/game/gameplay/gameplay-mature-world-progression-contract.prd.md`。
- `doc/product/world-rules-core-gameplay/prd.md` 与 `doc/game/gameplay/gameplay-top-level-design.prd.md`：分别承载物理尺度/间接控制的产品承诺，以及玩法侧动作粒度与表现层夸张边界。
- `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`：访问模式、统一候选门禁与公开 claim 升阶入口。
- `doc/game/gameplay/gameplay-agent-claim-economy-contract.prd.md`：近期高频经济规则与 token 成本边界主入口。
- [组织连续性首版政策](gameplay/gameplay-organization-continuity-policy.prd.md)：`draft` 实施输入，明确有效活动、保护期限、接替资格、资产顺序与部分交接；技术合同由 Runtime 组织连续性设计承接，未启用运行时政策。
- `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.prd.md`：WASM-backed 可编程区域设施、micro_depot quote/receipt、upkeep 与区域专业化边界主入口。
- `doc/game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md`：`PRD-GAME-017` 区域 charter 提案/审查、双资格、退化恢复、tenure 长期规划、服务费/公共 levy 玩家合同主入口；产品承诺仍由 `governed-regional-capabilities-and-extensions.prd.md` 拥有。
- `doc/game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md`：`PRD-GAME-019` 普通治理主体聚合、锁定/委托/实际控制人封顶、最小公开审计、重复主体纠错/申诉、事项快照、OC 外部转让与单向非治理额度桥玩家合同；产品承诺仍由 `governed-common-decisions-and-constitutional-boundaries.prd.md` 的 `GCB-004/005` 拥有，其中 `GCB-005` 的事项快照/额度桥由 `AC-GAME-019-04` / `AC-GAME-019-05` 承接。
- `doc/game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md`：`PRD-GAME-018` 工业提案/模拟/试点/治理准入、创作者许可/开放、OC/世界资格分离、跨区发现/物理交付/escrow 里程碑与争议恢复玩家合同主入口；当前消费 [`GR-009..011`](../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)、[`SC31-009/AC-SC31-010/011`](../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009)、[`ES-004`](../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004) active 产品 authority；[紧急保供首版政策](gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#emergency-supply-policy-v1)进一步承接 ES-001 至 ES-004 的有界专业规则值，不复制产品正文或执行状态。
- `doc/game/gameplay/gameplay-agent-claim-restricted-grant-liveops-runbook-2026-03-29.md`：restricted grant 发放、撤销、过期与 incident 处理 runbook。

## 默认阅读面边界
- 本页首屏只负责分流，不再要求读者先顺扫所有 gameplay 专题三件套与补充材料。
- runbook、evidence、checklist 与仍承担当前追溯职责的补充材料继续保留可检索性，但默认不与主专题三件套同屏平铺成长名单；一次性 handoff brief 若已被 topic project / evidence / PR、实际 CI 与评审记录（Issue 按需） 覆盖，应退役删除。
- 完整活跃专题清单与补充入口继续保留在下方，用于精确文件名检索和互链可达性。

## 覆盖规则
- 纳入规则：纳入 `doc/game/gameplay/*.prd.md` 与同名 `*.design.md` 的稳定专题对；可变执行状态只存于 Git、PR 和实际验证记录。
- 活跃补充：仍被模块 PRD / 项目态直接引用、且承担当前阶段判断或执行入口职责的 runbook / handoff / evidence，可在“活跃补充文档”或补充入口表中定向列出。
- 排除规则：补充材料继续保留检索能力，但除非重新成为默认首读入口，否则不进入首屏长表。
- 按需进入：当 README 已经完成首读分流时，本页只承担精确检索与补充路由职责。

## 完整活跃专题清单（按文件名精确检索）
| 专题 PRD | 专题设计文档 | 专题项目文档 |
| --- | --- | --- |
| `doc/game/gameplay/gameplay-agent-claim-economy-contract.prd.md` | `doc/game/gameplay/gameplay-agent-claim-economy-contract.design.md` |
| `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md` | `doc/game/gameplay/gameplay-indirect-control-agency-contract.design.md` |
| `doc/game/gameplay/gameplay-industrial-backpressure-recovery-contract.prd.md` | PRD-only；本专题只承载玩家选择、节奏、机会成本与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-industrial-demand-change-disposition-contract.prd.md` | PRD-only；本专题只承载需求变化后的既有工作选择、机会成本与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-industrial-externality-contract.prd.md` | PRD-only；本专题只承载工业外部性的玩家可读影响、处置选择与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-industrial-factory-capability-lifecycle-contract.prd.md` | PRD-only；本专题只承载工厂能力生命周期的玩家选择、恢复与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-industrial-factory-site-selection-contract.prd.md` | PRD-only；本专题只承载选址与物流拓扑的玩家取舍、失败恢复与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-industrial-maintenance-planned-downtime-contract.prd.md` | PRD-only；本专题只承载维护/计划停机的玩家选择、证据切线与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-industrial-pipeline-service-window-contract.prd.md` | PRD-only；本专题只承载 service-window/power continuity 的玩家投影、机会成本、恢复与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-industrial-raw-material-qualification-substitution-contract.prd.md` | PRD-only；本专题只承载原材料适用性/替代的玩家投影、机会成本、恢复与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-industrial-recipe-lifecycle-decision-contract.prd.md` | PRD-only；本专题只承载配方生命周期的玩家投影、旧工作处置与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-industrial-representative-execution-walkthrough.prd.md` | PRD-only；本专题只承载代表性工业链的玩家顺序、取舍、恢复与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md` | 专业规则 PRD；承载工业/市场玩家合同及紧急保供首版政策值；wire、事务与恢复由 [保供系统设计](../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md)承接 |
| `doc/game/gameplay/gameplay-industrial-starter-completion-contract.prd.md` | PRD-only；本专题只承载首产物完成边界、失败恢复与下一步动力，不新增实现结构 |
| `doc/game/gameplay/gameplay-mature-world-progression-contract.prd.md` | PRD-only；本专题不新增接口、状态机、回滚或实现算法，后续引入实现结构时再补同名 design |
| `doc/game/gameplay/gameplay-top-level-design.prd.md` | `doc/game/gameplay/gameplay-top-level-design.design.md` |
| `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.prd.md` | `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.design.md` |
| `doc/game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md` | PRD-only；本专题只承载区域制度的玩家动作、机会成本、失败恢复、申诉与验收，不新增实现结构 |
| `doc/game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md` | PRD-only；本专题只承载普通治理主体、事项权利与 OC 分离的玩家动作、机会成本、失败恢复与验收，不新增实现结构 |
| [gameplay-organization-continuity-policy.prd.md](gameplay/gameplay-organization-continuity-policy.prd.md) | 专业政策 PRD；执行、事务与恢复由 [组织连续性设计](../world-runtime/runtime/organization-continuity-and-disposition.design.md)承接，不重复建立同名技术权威 |

## 历史 closure / provenance 入口
| 历史 closure 专题 | 当前追溯入口 |
| --- | --- |
| lifecycle rules、war/governance/crisis/meta、module-driven production 历史 closure（正文已退役） | 玩家合同与生产落地证据已收敛到 `doc/game/gameplay/gameplay-top-level-design.prd.md`、战争/政治数值基线及 `doc/world-runtime/prd.md#gameplay-生命周期协议边界`；历史审读见 Git history 与 GitHub task issue evidence。 |
| `gameplay-release-gap-closure-2026-02-21` | 正文已退役；历史内容从 Git history、core review logs 与 PR、实际 CI 与评审记录（Issue 按需） 追溯。 |
| gameplay release-production、runtime-governance 与 base-runtime/WASM split closure | 正文已吸收到 gameplay top-level、world-runtime/WASM 专业权威；历史过程从 Git history 与 PR 与实际验证记录 追溯。 |

上述 closure 均不作为 active gameplay truth；已完成语义收敛的正文直接退役，不保留 redirect 或占位文件，尚未完成逐文件迁移审计的旧 closure 仅保留为非首读 provenance。

## 运行 / 证据 / 交接补充入口
| 文档路径 | 类型 | 用途 |
| --- | --- | --- |
| `doc/game/gameplay/gameplay-agent-claim-restricted-grant-liveops-runbook-2026-03-29.md` | `runbook` | restricted grant 发放、撤销、过期与 incident runbook |
| `doc/game/gameplay/gameplay-longrun-p0-replay-rollback-runbook-2026-03-06.md` | `runbook` | longrun P0 replay rollback 处理 runbook |

## 说明
- 本索引用于保证模块专题文档在根入口文档树中可达。
- 文档配对规则：`*.prd.md` 原则上与同名 `*.design.md` 配对；仅承载 Why / What / Done、且不新增接口、状态机、回滚或实现算法的 PRD-only 专题必须在上表显式登记例外。任务状态、计划和历史 trace 由 Git、PR 与实际 CI 记录承载。
- authority 口径：`doc/game/prd.md` 是活跃基线与路由根入口；`doc/game/gameplay/gameplay-top-level-design.prd.md` 只拥有核心玩法骨架与 `PRD-GAME-012` 的专题细节，其他 topic 在各自声明范围内拥有细节。历史 ROUND-002 主从表述不构成现行优先级。
- 默认入口面先在 `README.md` / `prd.index.md` 收紧；只有当入口仍无法完成分流时，才进入下一轮路径级治理。
