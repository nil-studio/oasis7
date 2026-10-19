# game PRD

> 专业域 authority：本文件是 `game` 的活跃玩法基线与路由入口，拥有 PRD-ID、专题范围、活跃阅读路径和状态指针；专题在其声明范围内拥有详细玩法合同与验收。产品承诺仍由四模块 `doc/product/` 层拥有，不因本文件或专题而形成并列产品入口。`micro_depot` 等设施规则仍由 `game` 管理；跨域产品承诺由世界规则与玩法系统模块的 [`受治理的区域能力与扩展`](../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md) 专题汇总，确定性执行、最终性与恢复底座继续下钻到 [`权威世界基础设施`](../product/world-infrastructure/prd.md)。

审计轮次: 11

## 当前阶段代表性体验

当前交付目标是受控真实玩家通过有界授权的 Agent 在同一持久世界获得首产物、形成可恢复的持续能力，再次进入后继续原有目标。唯一代表性链为 `starter-industrial-smelter-to-assembler-v1`：合法建成 Smelter，以铁锭配方完成合法生产 cycle，在 owner-bound output ledger 形成首笔匹配正量铁锭结算，采用 `production_only`，随后只开放 Assembler 候选。详细边界见[首产物结算合同](gameplay/gameplay-industrial-starter-completion-contract.prd.md)。实现存在与同候选端到端通过分别表达；首产物、稳定能力、交付各自验收。

## 目标
- 作为 game 模块的活跃玩法基线入口，回答当前玩家体验目标、阶段口径、权威专题和完成定义。
- 保持 PRD-ID、专题文档、执行任务和验证证据可追踪，但不在根 PRD 重复展开每个专题的完整规格。
- 让新增 gameplay 变更能先判断应改根基线、专题 PRD、执行 project，还是仅补索引/证据。

## 范围
- 覆盖 game 模块当前玩家侧核心循环、progression、agency、经济/claim、preview/beta gate 与可玩性验收边界。
- 覆盖 PRD-ID 到 `doc/game/prd.index.md` 与高频专题 PRD 的路由关系。
- 不覆盖实现代码逐行说明、历史执行流水、专题完整 user story / matrix / decision log；这些内容保留在专题 `*.prd.md`、测试 evidence 与 GitHub task trace。

## 接口 / 数据
- PRD 主入口: `doc/game/prd.md`
- 可变执行状态: 对应 GitHub Project task 与 issue evidence comments
- 文件级索引: `doc/game/prd.index.md`
- 模块 landing page: `doc/game/README.md`
- gameplay 热点子域入口: `doc/game/gameplay/README.md`
- 核心玩法骨架: `doc/game/gameplay/gameplay-top-level-design.prd.md`
- 追踪主键: `PRD-GAME-xxx`
- 测试与发布参考: `testing-manual.md`
- 跨模块模式 taxonomy: `doc/product/player-entry-distribution/prd.md`

## 里程碑
- M1 (2026-03-03): 完成模块设计 PRD 主体重写与任务改造。
- M2 (2026-05-17): 形成 gameplay 热点子域入口，收口 retention、agency、scale 与 small-player lane 的高频首读路径。
- M3 (2026-06-21): 根 PRD 改为活跃玩法基线表，专题细节回到对应专题文档，降低根入口漂移风险。

## 风险
- 根 PRD 若重新堆叠专题细节，会再次和专题 PRD 争夺权威。
- 历史 gate / sample 若缺少时间戳和 evidence 指针，容易被误读为当前 blocker。
- `doc/world-simulator/*` 的 scenario / resource docs 会影响 gameplay，但不应替代 `doc/game` 的玩家侧 loop / progression 权威。

## 1. Executive Summary
- Problem Statement: game 模块的玩法、经济、preview/beta、agency 与小玩家成长线已拆成多份专题；根 PRD 需要保留当前基线和路由，而不是继续成为专题规格汇编。
- Proposed Solution: 根 PRD 只维护活跃 gameplay baseline、PRD-ID 路由、关键 gate 口径和跨模块权威边界；专题细节、历史执行与证据全部通过索引和 project trace 下钻。
- Success Criteria:
  - SC-1: 新增或变更 gameplay 能映射到一个 PRD-GAME-ID、专题 PRD 或明确的新增专题需求。
  - SC-2: 读者能在根 PRD 内判断当前玩家侧真值，再通过链接进入专题细节。
  - SC-3: 当前 stage / claim envelope / formal gate 口径不依赖历史流水解释。
  - SC-4: root PRD 不复制专题矩阵；专题文档仍保持可检索、可追踪、可验证。

## 2. Active Gameplay Baseline
| PRD-ID | 当前玩家侧真值 | 权威专题 / 入口 | 当前状态与验证口径 |
| --- | --- | --- | --- |
| PRD-GAME-001 | game 模块以玩法循环和玩家体验为主轴组织需求，不以功能清单平铺。 | `doc/game/gameplay/gameplay-top-level-design.prd.md` | 活跃基线；新增 gameplay 专题需回挂 `doc/game/prd.index.md` 与相关 topic project。 |
| PRD-GAME-002 | 规则层边界必须能映射到 runtime / agent / viewer 可验证语义，不能只停留在概念。 | `doc/game/gameplay/gameplay-top-level-design.prd.md`、`doc/world-runtime/prd.md` | gameplay 拥有玩家规则，runtime 拥有实现边界；具体行为仍由专业任务证据验证。 |
| PRD-GAME-003 | 发布前可玩性结论必须绑定证据、风险等级和 go/no-go 口径。 | `testing-manual.md`, `doc/playability_test_result/prd.md` | release / preview 相关结论必须回到 QA evidence。 |
| PRD-GAME-004 | micro-loop 要让玩家看见动作接受、推进、阻塞、反馈和下一步；Viewer 与 pure API 必须从同一权威状态给出状态感知的“现在做什么”：有效动作，以及合理但暂不可用动作的原因和解锁/恢复路径。cold start、进行中、重连和空/阻塞快照都必须保留有效决策，不能只停留在内部 snapshot 字段、空列表或协议猜测。 | `doc/game/gameplay/gameplay-top-level-design.prd.md`；`doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md` | 本行拥有 gameplay 验收语义；产品承诺与组合验收见首局专题。后续 runtime/viewer 必须以真实动作能力验证 parity，不得借此新增 snapshot 字段、UI 布局或改变动作可用性。 |
| PRD-GAME-005 | 分布式执行 / 治理能力是长期在线支撑，不是当前首局玩家主循环扩张许可。 | `doc/product/world-infrastructure/distributed-consensus-and-state-availability.prd.md`；`doc/product/world-infrastructure/deterministic-world-execution.prd.md`；`doc/world-runtime/prd.md`；`doc/p2p/prd.md` | game 只保留玩家治理体验与早期曝光边界；长期执行、共识和恢复由专业域拥有。 |
| PRD-GAME-006 | 长期在线能力需覆盖权威分层、回放/回滚、反滥用、经济闭环和可恢复的发布门禁。 | `doc/product/world-infrastructure/distributed-consensus-and-state-availability.prd.md`；`doc/product/world-infrastructure/deterministic-world-execution.prd.md`；`doc/world-runtime/prd.md`；`doc/p2p/prd.md`；`doc/testing/prd.md` | 作为长期世界产品与专业验证边界，不替代 player-facing gate，也不在 game 根冻结运维阈值。 |
| PRD-GAME-007 | FirstSessionLoop 之后必须有 PostOnboarding 阶段目标、阻塞、下一步承接；阶段成果具有完成边界、可归因后果与下一方向，但不是强制通关终局。正式入口维持一个当前主目标和低负担“继续”路径；预设目标只作为形成首个持续能力前的必要引导脊柱，之后降为可选世界状态模板。`branch_ready` 仅在阶段成果或玩家主动换向时，按当前状态提供扩张、稳定/恢复、专业化/服务中的 2 至 3 个可比较承诺。每项必须说明即时收益、实质不同的后续两个 beat、风险/锁定与下次会话第一动作；可回退项还要说明窗口、代价和保留/失去价值。 | `doc/game/gameplay/gameplay-top-level-design.prd.md`；`doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md` | 本行拥有 gameplay 验收语义，不冻结成本、字段或 UI；目标作用域、canonical 转译、资源/权限校验、共同治理与审计保持后台护栏，只有实质改变当前选择时才给出原因和替代路径，不构成逐动作确认。路线可达性仍须由真实权威状态证明。历史 gate 不构成当前 blocker，后续 runtime/viewer/QA 需用同一状态样例验证路线差异与回访承接。 |
| PRD-GAME-008 | pure API 是正式玩家访问模式之一，信息粒度、动作能力和持续游玩必须与 UI 等价；正式游玩要求 active LLM access。 | `doc/game/gameplay/gameplay-top-level-design.prd.md`；`doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md` | `--no-llm` 仅保留 observer/debug，不支撑正式可玩性或 parity 放行。 |
| PRD-GAME-009 | 阶段候选需要统一 release gate 和对外 claim envelope，不允许 topic-by-topic 拼凑升阶；当前公开阶段只以根 `README.md` 为准。 | `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`；`doc/testing/prd.md`；根 `README.md` | 历史 closed-beta 专题已完成迁移退役；阶段或 claim 变化必须由同候选证据、QA、产品决策与 LiveOps 同步共同支持。 |
| PRD-GAME-010 | limited preview 必须是 controlled builder-facing、可回流、可纠偏的真实执行闭环；信号按 `Blocking / Opportunity / Idea` 回流，claim drift 在同轮纠正。QA 可建议 block / conditional，producer 决定 continue / hold / reassess；gate pass 不自动升阶。 | `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`；Git、PR 与实际 CI 记录 | Round 1 已于 2026-07-28 关闭且无待处理 follow-up；历史发布、渠道 fallback 与信号证据从 Git history 和 GitHub issue `eng-cc/oasis7#48` 追溯。新复现或提案使用聚焦 issue / PR；公开阶段只看根 `README.md`。 |
| PRD-GAME-011 | agent claim 体现持续承诺：首个 claim 也非免费，但 slot-1 可按 runtime 规则使用 restricted starter funding。首个认领决策包必须同时说明候选用途/差异、非零 upfront cost、确认后的 upkeep runway、release/grace/reclaim 触发、恢复/重新选择和最佳等待/替代动作。当前 cold-start 路径可包含 `claim_first_agent -> claim_starter_oc -> first agent chat`，其中 starter OC 授予初始 liquid OC 并记录首聊解锁，不是免费 agent claim、claim/upkeep 资金或通用补贴。 | `doc/game/gameplay/gameplay-agent-claim-economy-contract.prd.md`；`doc/product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md` | restricted grant / upkeep / reclaim / audit 以专题与 runbook 为准；restricted starter claim balance 与 starter OC 必须分开，前者只能支持 non-zero slot-1 claim/upkeep 成本，后者只处理已存在 Agent 的首聊 gate。该口径不冻结成本、余额、字段或 runtime chat 语义；缺完整认领决策包标记 `first_claim_commitment_packet_missing`。 |
| PRD-GAME-012 | 10-minute trust gate、first capability gate 与 first 10/30-minute attraction/content-volume gate 分开判定；target coverage、motivation density 和 content volume 不得互相替代。早期 quote/preview 必须聚焦一个主要决策和一个主导 blocker/cost，可延后可恢复细节但必须提升损失、锁定、authority transfer、不可逆行动或恢复可用性变化，且不得省略权威成本或改写语义。各路线/机会、资源、排程、精炼、市场、调运、验证与能源预览仍须按专题说明各自取舍，而不是只靠执行后 receipt 或后台事件。 | `doc/game/gameplay/gameplay-top-level-design.prd.md`；产品承诺见 `doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md` | 本行拥有早期信息仲裁的 gameplay 验收语义，不冻结 UI/字段。当前 verdict、TASK-GAME-076 required/live 边界与历史 trace 由同候选 PR 与实际验证记录 和 `doc/testing/evidence/` 确认；缺仲裁或高后果信息被隐藏时标记 `early_preview_arbitration_missing`。 |
| PRD-GAME-013 | oasis7 采用真实厘米尺度，但当前玩家主路线仍是间接控制文明模拟；不得把 `1cm` 写成 Minecraft 式逐块直接操作承诺；玩家提出过细动作时，必须翻译成当前可玩的间接控制替代动作或说明无安全替代动作。 | `doc/game/gameplay/gameplay-top-level-design.prd.md`；[`doc/product/world-rules-core-gameplay/prd.md`](../product/world-rules-core-gameplay/prd.md) | 四层合同：厘米真值、coarse-grained 子系统、玩家动作粒度、表现层夸张；替代动作必须来自 canonical 动作面，否则要安全停止并说明下一次可决策点。具身 / block-editing 仅在强化间接控制主路线、具备专业域合同与验证并经显式跨域决策后才可进入候选原型；2026-07-08 `task_4ab03f9be0f847af9f36d963486055d5` 记录过细动作缺少可玩替代动作翻译的小缺陷。 |
| PRD-GAME-014 | 间接控制必须保留 agency：记忆驱动、社交、治理与冲突动作共用 causal-decision receipt，说明 accepted intent、Agent reason/evidence、stakes/expected consequence、alternative、interrupt/correction、earliest effective point 和 post-correction result；请求/提案接受不等同于权威规则应用或后果发生。社会事实、治理或冲突若影响玩家选择，仍须在同一因果链中提供可读的提交前后果与纠正/替代方向。 | `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md`；`doc/product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md` | 本行拥有 gameplay receipt/验收语义，不冻结 runtime 字段、治理规则或 UI。control-feeling 的 runtime/viewer/agent/QA 证据以专题 project 与 evidence 为准；缺跨动作 receipt 或混淆接受与权威应用时标记 `causal_decision_receipt_missing`。 |
| PRD-GAME-015 | mature-world 小玩家需要不依赖立即投靠 major power 的长期推荐轴：建立并守住可恢复能力 -> 服务区域需求 -> 有限且可审计的区域影响；它们可改道、重排或回退，不是强制职业树。有效阶段成果必须新增选择、恢复弹性、局部协调位置或区域用途，不能以库存、吞吐或重复次数冒充成长。文明尺度项目仅是受影响范围内经授权形成的自愿共同扩展，不是全体玩家的胜利条件或唯一有效路线。针对同一个 active goal，repair / rebuild / pivot 必须比较时间/阶段与资源成本、保留/失去价值、主要风险、推荐理由和独立 lane 可行性；只有独立路径确实不可行时才能把外部依赖列为有原因的受迫项。专业化选择前还必须展示第一单交付收益和本地需求匹配。 | `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md`；`doc/game/gameplay/gameplay-mature-world-progression-contract.prd.md`；[`generic mature-world recognition/opportunity leaf`](gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-recognition-opportunity)（承接 `REQ-WR-MW-004` / `MW-11`，不新增 PRD-ID 或 authority） | 本行拥有 gameplay 比较/验收语义，不冻结数值、状态或 UI。代表性 disruption 必须证明恢复后仍有独立成长或明确重评条件；不得把 sponsor/major-power 依赖当作默认解；认可/机会叶子仅适用于 mature-world，并以其权限重验、单次 receipt、失败恢复和 unresolved evidence gate 为准。 |
| PRD-GAME-016 | `micro_depot` 是第一个 WASM-backed 可编程区域设施：玩家通过小型、可审计、带 upkeep 的区域设施，改变一次 repair / logistics quote 并获得可追溯 receipt。 | `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.prd.md` | 区域专业化 / limited-scope regional influence 专题；不进入首 10 分钟新手循环，不开放自由建造、任意 WASM 上传或 global governance 权力；2026-07-08 `task_4ab03f9be0f847af9f36d963486055d5` 记录 install quote 缺少 break-even / ROI 判断。 |
| PRD-GAME-017 | 区域 charter、地点 tenure 与公共融资的玩家合同：玩家能提交有证据的 charter、理解双资格与有限权限、在退化时恢复或申诉、为 tenure 做可读长期规划，并区分服务费与有界公共 levy。 | `doc/game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md` | 承接产品 `GR-004` 至 `GR-008` 的玩家动作、机会成本、失败恢复、申诉和可玩性验收；不扩展 `PRD-GAME-015`、不取代 `PRD-GAME-016`，不冻结 runtime/P2P、数值、UI 或 release claim。 |
| PRD-GAME-018 | 工业能力从可审计提案、确定性模拟、受限试点走到有范围治理准入；跨区市场从带新鲜度的发现走到物理交付、目的地存储、escrow 里程碑与争议恢复；OC 链上转让与世界资格保持分离。 | `doc/game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md` | 消费 [`GR-009`](../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009) / [`AC-GR-009`](../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009)、[`GR-010`](../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010) / [`AC-GR-010`](../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-010)、[`GR-011`](../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011) / [`AC-GR-011`](../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-011)、[`SC31-009`](../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009) / [`AC-SC31-010`](../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-010) / [`AC-SC31-011`](../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-011)、[`ES-004`](../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004) / [`AC-ES-004`](../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-004) active 产品 authority；拥有玩家动作、机会成本、失败恢复与 parity 验收；[紧急保供首版政策](gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#emergency-supply-policy-v1)单一承接 ES-001 至 ES-004 的触发、配给、补偿和退出规则。不冻结其他配方、常态价格或 runtime/P2P 实现，不新增 release claim。 |
| PRD-GAME-019 | 普通治理主体聚合、锁定/委托/实际控制人封顶、最小公开审计、重复主体纠错/申诉，以及事项快照、OC 外部转让和单向非治理额度桥的玩家合同。 | `doc/game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md` | 承接产品 `GCB-004/005` 的玩家动作、取舍、失败恢复和 parity 验收；其中 `GCB-005` 的事项快照与单向非治理额度桥明确由 `AC-GAME-019-04` / `AC-GAME-019-05` 收口；不重复 `PRD-GAME-017/018`，不冻结身份/链上/runtime 参数、数值、UI 或 release claim。 |
| PRD-GAME-020 | 组织有效活动、通知保护、有限接替、资产处置与部分交接的首版专业政策。 | [组织连续性政策](gameplay/gameplay-organization-continuity-policy.prd.md) | 承接产品 [OC-001](../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001)、[OC-002](../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002)、[OC-003](../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003)；当前为 `draft` 首版设计输入，此处登记承接关系，不表示政策已启用；执行与恢复由 [Runtime 设计](../world-runtime/runtime/organization-continuity-and-disposition.design.md)承接，未声明业务实现或验收完成。 |

## 3. Player-Facing Authority Boundary
- `doc/game/prd.md` 是活跃 gameplay baseline 与路由的唯一根入口：它维护 PRD-ID、每个专题的声明范围、默认首读路径以及当前状态应去哪里确认；不复制专题的完整规格。
- `doc/game/gameplay/gameplay-top-level-design.prd.md` 是核心玩法骨架与 `PRD-GAME-012` early-retention 专业合同的专题 authority，不取得 game 模块级 authority。其范围内的详细合同优先；范围外的问题必须路由到相应专题或产品模块。
- 其他 topic `*.prd.md` 在各自明确声明的主题范围内同样拥有详细合同；topic detail 只在该范围内优先，不能覆盖根 PRD 的活跃路由/状态指针或产品层承诺。
- Git、PR 和实际验证记录 维护当前/近期执行状态、阻断、下一步和历史 trace；正式 evidence 决定验证结果，二者均不改写产品承诺或专题范围。
- `doc/game/README.md` 与 `doc/game/gameplay/README.md` 负责首读分流，不复制完整专题清单；`doc/game/prd.index.md` 负责完整文件级检索。
- 发生表述冲突时，先按上述范围判定 authority：产品承诺回产品模块，活跃基线/路由/状态入口回本文件，专题细节回声明专题，执行或验证结论回 project/evidence；范围仍不清楚时保留 blocker 并派发对应 owner slice，不能按更新时间或历史轮次推定优先级。
- `doc/world-simulator/scenario/*` 与 `doc/world-simulator/m4/*` 可定义 scenario、resource、industrial loop 和 deterministic support contracts；当它们影响玩家侧 progression、resource pressure 或 onboarding 时，应回指 `doc/game` 对玩家体验口径的权威。
- resource terminology 需要区分 built-in runtime resource truth 与 module-defined gameplay material taxonomy；如需改名或改变玩家承诺，必须由 `producer_system_designer` 与 `repository_health_engineer` 共同收口。
- 跨模块资源 provenance 的产品层路由见 [`doc/product/world-rules-core-gameplay/prd.md`](../product/world-rules-core-gameplay/prd.md#resource-model-and-cross-module-provenance)：根 PRD 不定义数值、汇率、runtime 字段或余额，只把通用资源、受限 claim/upkeep support、liquid starter OC 与设施/材料记录分别路由到声明的专业合同。

## 4. Technical Specifications
- Architecture Overview: game 模块定义玩家侧玩法目标、循环、阶段和验收口径；runtime / world-simulator / viewer / agent 提供实现、观测与验证支撑。
- Integration Points:
  - `doc/game/gameplay/README.md`
  - `doc/game/gameplay/gameplay-top-level-design.prd.md`
  - `doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md`
  - `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md`
  - `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md`
  - `doc/game/gameplay/gameplay-mature-world-progression-contract.prd.md`
  - `doc/game/gameplay/gameplay-industrial-maintenance-planned-downtime-contract.prd.md`
  - `doc/game/gameplay/gameplay-industrial-recipe-lifecycle-decision-contract.prd.md`
  - `doc/game/gameplay/gameplay-industrial-raw-material-qualification-substitution-contract.prd.md`
  - `doc/game/gameplay/gameplay-industrial-pipeline-service-window-contract.prd.md`
  - `doc/product/world-rules-core-gameplay/prd.md`（产品承诺）与 `doc/game/gameplay/gameplay-top-level-design.prd.md`（玩法合同）
  - `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`
  - `doc/game/gameplay/gameplay-agent-claim-economy-contract.prd.md`
  - `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.prd.md`
  - `doc/game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md`
  - `doc/game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md`
  - `doc/game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md`
  - `doc/game/gameplay/gameplay-top-level-design.prd.md`（early-retention 与生产排程报价专业合同）
- Edge Cases & Error Handling:
  - 若根 PRD 与专题 PRD 冲突，按第 3 节的声明范围路由；不得用根入口摘要覆盖专题细节，也不得用专题细节夺取根入口的活跃路由或状态职责。
  - 若 evidence / project 状态与 PRD baseline 冲突，先保留 blocker 口径并派发对应 owner slice，不直接改写玩家承诺。
  - 若 world-simulator 文档描述资源/工业细节但未说明其 gameplay 权威边界，应补 authority note 或回指 game 专题。
- Non-Functional Requirements:
  - NFR-GAME-1: gameplay 变更必须可追踪到 PRD-ID、专题 PRD 或 `.pm` task trace。
  - NFR-GAME-2: formal playability / preview / release claims 必须绑定 fresh evidence，不得复用已标为 historical baseline 的旧样本。
  - NFR-GAME-3: 根入口不得复制专题完整矩阵；新增细节优先进入专题文档，再从根入口增加一行路由。
  - NFR-GAME-4: 玩家侧 current truth 必须能在 `doc/game/prd.md`、对应专题、Git、PR 与实际 CI 记录 和 evidence 之间闭环检索。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP: 保持当前 `limited playable technical preview` claim envelope 与 active gameplay baseline。
  - v1.1: 按真实 preview feedback 更新 `PRD-GAME-010`、QA verdict 和 producer decision；并用 `TASK-GAME-076` 区分 `progression_pass`、`motivation_density_pass`、`content_volume_weak/pass` 与最终 `attraction_weak/pass`。
  - v1.2: 若 resource terminology 继续产生误读，单独收口 built-in resource truth vs module gameplay taxonomy。
- Technical / Documentation Risks:
  - 风险-1: 历史流水再次进入根 PRD，导致根入口变重。
  - 风险-2: topic PRD 更新后未同步根 baseline 行，导致首读口径过期。
  - 风险-3: scenario / m4 支撑文档被误读为独立 gameplay promise。

## 6. Validation & Decision Record
- Test Plan & Traceability:
  - 精确检索完整专题: `doc/game/prd.index.md`
  - 当前执行与 blocker: 对应 GitHub Project task 与 issue evidence comments
  - gameplay topic routing: `doc/game/gameplay/README.md`
  - release / preview / playability evidence: `testing-manual.md`, `doc/playability_test_result/`, `doc/testing/evidence/`
- Root PRD maintenance check:
  - 新增 PRD-GAME-ID 时，必须同步 `doc/game/prd.index.md` 和相关 topic project。
  - 改变默认首读路径时，必须同步 `doc/game/README.md` 与 `doc/game/gameplay/README.md`。
  - 改变当前 stage / claim envelope / gate verdict 时，必须同步 Git、PR 与实际 CI 记录 与同候选 evidence。
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-GAME-ROOT-001 | 根 PRD 维护 active gameplay baseline + topic links | 根 PRD 继续复制所有专题 user story / matrix / decision log | 降低权威漂移，保留专题可追踪性。 |
| DEC-GAME-ROOT-002 | 当前 stage 继续维持 `internal_playable_alpha_late`，claim envelope 为 `limited playable technical preview` | 用 unified gate pass 直接升级 closed beta candidate | 现有 producer 决策仍要求真实 preview 执行和回流闭环。 |
| DEC-GAME-ROOT-003 | resource / scenario 支撑文档需回指 game 玩家侧权威 | 让 m4 / scenario 文档独立定义玩家 progression promise | 防止资源压力、onboarding 与工业 loop 多头漂移。 |
