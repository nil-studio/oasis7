# Gameplay 普通治理主体与 OC 权利边界合同

- `PRD-ID`：`PRD-GAME-019`
- 上层产品映射：本合同承接普通共同决策与宪制边界产品专题的 [`REQ-WR-GCB-004`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004) / [`AC-WR-GCB-004`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#ac-wr-gcb-004) 与 [`REQ-WR-GCB-005`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005) / [`AC-WR-GCB-005`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#ac-wr-gcb-005)。旧 GG-2/GG-3 仅作为迁移 provenance，不再拥有 active 产品或 gameplay authority。
- 主题 authority：本文件拥有普通治理主体聚合、资格比较、委托/纠错/申诉，以及事项范围内 OC 权利与外部转让分离的玩家动作、取舍、失败恢复和可玩性验收。
- 相邻 authority：[`PRD-GAME-017`](gameplay-regional-charter-tenure-funding-contract.prd.md) 继续拥有区域 charter 的双资格、tenure 与公共融资；[`PRD-GAME-018`](gameplay-industrial-creation-and-cross-region-market-contract.prd.md) 继续拥有工业/市场场景的 OC 与世界资格隔离，并仅消费 GR-011 的工业/世界资格边界；本文承接 GCB-005 的普通事项快照与单向非治理额度桥，具体由 [`AC-GAME-019-04`](#ac-game-019-04) / [`AC-GAME-019-05`](#ac-game-019-05) 收口；[`PRD-GAME-014`](gameplay-indirect-control-agency-contract.prd.md) 继续拥有治理动作的 causal-decision receipt。本文不把相邻专题的局部语义扩展为普通治理全局权力。
- 专业边界：`world-runtime` 拥有权威资格、事项状态、snapshot、退出/解除与 receipt 执行；`p2p` / blockchain authority 拥有身份、签名、OC 分布式状态与安全边界；`doc/testing/prd.md` 与 QA 拥有组合验证和当前结论。
- 设计适用性：`simple-topic-exemption`（`PRD-only-sufficient`）。本文只冻结 Why / What / Done、玩家循环、机会成本、失败恢复和专业验收，不新增 API、schema、状态机、链上交易、身份技术、数值参数或 UI 布局。
- 当前执行：可变任务状态与当前实现证据由 Git、PR、实际 CI 和评审记录 拥有；本文不宣称当前已实现、已平衡、已可玩、已发布或已具备 release readiness。

## 1. 目标与范围

普通治理对玩家而言是一条有边界的共同经营循环：玩家先确认“谁在代表我、凭什么有资格、影响到哪一件事”，再决定锁定、委托、纠错或退出。外部 OC 转让与游戏内事项权利是两条不同的事实链；玩家必须能在同一决策面上分辨它们的收益、限制、失败原因和下一步。

本合同覆盖六个玩家侧问题：

1. 如何把经验证账户的已批准身份、managed signer 与 external wallet bindings 理解为一个治理主体，而不把个人数据暴露为公开控制面。
2. 如何比较事项范围内的锁定资格、可撤回委托和实际控制人封顶，理解拆分账户或循环转移不能扩大普通治理影响。
3. 如何读取公开委托、最终计票、结果和纠错因果，同时知道哪些控制关系只以最小化私下断言参与资格判断。
4. 如何对疑似重复主体发起 challenge，并沿 review、适用 merge/revocation、计票 correction 与 appeal 恢复可归因的结果。
5. 如何区分外部 OC 转让、事项范围内 lock/snapshot、退出/解除绑定与已确认的游戏内权利，避免把持有或转账当作治理效果。
6. 如何把单向 `OC -> LetAI Run quota` 额度桥理解为非治理服务额度，避免以额度、`token_key` 或服务消费旁路资格或权重。

普通治理不是首局或独立成长的强制门槛，也不是玩家获得世界主权的 progression 奖励。它提供可审计的有限选择、纠错能力和持续参与理由；不改变普通治理白名单、宪制保护或既有区域/工业专题边界。

## 2. 玩家循环与动作语义

所有预览和检查都是只读的，不预留 OC、资格、代表权或世界效果。玩家确认后，权威执行必须按当前事项范围和新鲜事实重验；状态漂移、权限不足或专业 authority 缺失时只能显示待决、过期、拒绝或 `incomplete / unknown / blocked`，不得用默认值补齐。

| 阶段 | 玩家动作与比较 | 玩家获得的即时价值 | 主要成本、失败与恢复 |
| --- | --- | --- | --- |
| 1. 治理主体检查 | `inspect_governance_subject`：查看已批准绑定的聚合范围、当前可公开审计的委托关系和需要最小私下控制关系断言的资格边界 | 知道哪些身份/签名/钱包属于同一治理主体，且不会把个人资料误当作公开治理数据 | 绑定不完整、冲突或过期时不能进入普通资格；补证、等待复核或保留当前未决状态，不自动创建第二主体 |
| 2. 资格比较 | `compare_governance_qualification`：比较事项范围内的锁定 OC、可撤回委托、实际控制人封顶、授权来源、当前窗口与本地/区域附加条件 | 看见“为什么有资格/为什么不足”、本次事项的作用域和可继续的下一步；不把 OC 持有量直接看成世界权力 | 只满足单一条件、拆分账户、循环转移或控制关系冲突时拒绝扩大影响；补足适用条件、改走独立路线或放弃本次事项 |
| 3. 代表权选择 | `delegate_with_scope`、`revoke_delegation`：在明确事项范围内委托、查看代表影响和撤回边界 | 以有限代表权参与普通事项，同时保留何时生效、何时失效和撤回后的下一次有效窗口 | 委托超范围、已过有效窗口或当前授权失效时不产生部分代表权；保留原 receipt，重提或等待新的有效窗口 |
| 4. 重复主体纠错 | `challenge_duplicate_subject`、`inspect_review`、`request_correction_or_appeal`：对疑似重复主体查看理由、处理状态和适用的 merge/revocation 结果 | 纠错不会抹掉历史；玩家能看到原授权、计票、receipt、纠错理由与当前有效结果之间的因果链 | challenge 可能被驳回、待补证或只限制未来影响；不得借 challenge 复制投票、冻结无关主体或追溯重写已确认结果 |
| 5. 事项权利快照 | `inspect_issue_lock_snapshot`：查看该事项的锁定、snapshot、有效窗口、退出/解除条件和当前是否已形成游戏内权利 | 把“可参与本事项”与“拥有 OC/获得世界资产”区分开，并知道何时退出仍可影响后续决策 | 快照过期、退出已生效或 authority 缺失时只能重评/重提；不沿用旧缓存，不承诺未来事项权利 |
| 6. 外部资产与服务桥 | `transfer_oc`、`inspect_oc_receipt`、`inspect_quota_bridge`：分别查看外部 OC receipt、事项权利状态与服务额度状态 | 玩家能比较外部转让、治理参与和服务消费各自的条件与用途 | OC 转让不自动取得/保留治理权；额度桥不产生治理权、赎回或权重；任一侧失败只保留该侧事实与独立恢复路径 |

玩家完成循环的标准不是“看到余额”“提交了委托”或“额度已消费”，而是能回答：我代表哪个主体、这次事项的权利凭什么成立、什么事实会使它失效、已确认的结果保留什么、下一次应补证/纠错/退出还是重提。提案、投票、排队、链上 receipt 或本地缓存均不能单独表示游戏内治理效果已经生效。

## 3. 普通治理主体与资格选择

### 3.1 聚合与公平

普通治理将经验证游戏账户的已批准玩家身份、managed signer 和 external wallet bindings 视为一个治理主体。公开玩家面只需展示完成当前决策所需的治理主体、委托关系、最终计票/结果和纠错因果；高影响资格可能需要消费最小化的私下控制关系断言，但不得把邮箱、设备、密钥、钱包控制细节或其他非必要个人数据转为公开控制面。

玩家比较锁定 OC、事项范围内可撤回委托和实际控制人聚合后的影响上限时，必须看到它们如何共同形成当前资格，以及持续运营、维护或交付等相邻条件何时属于另一条专业规则。拆分账户、关联组织、managed signer 重复绑定或短期循环转移不能制造额外普通治理影响；如果聚合或授权事实不完整，结果是未知/阻塞，不是系统猜测的安全资格。

### 3.2 委托、纠错与申诉

委托是明确作用域内的代表权转交，不是所有权转让、无限治理授权或永久控制。玩家可读预览必须说明事项、范围、当前有效窗口、代表影响、撤回/失效条件和不能改变的保护边界。重复主体 challenge 必须形成可追踪处理链：受理或拒绝理由、review 状态、适用的 merge/revocation、计票 correction、appeal 与最终有效结果；纠错保留历史，不产生双计或第二次世界效果。

纠错失败时，玩家仍应得到补证、等待复核、申诉、撤回委托或回到独立事项路径中的适用下一步。纠错请求、重复提交、重连、Agent retry、旧 receipt replay 不得复制代表权、计票、奖励或资格窗口。

## 4. OC 转让、事项快照与额度桥

### 4.1 外部转让与游戏内权利分离

外部 OC 可以转让；转让只改变外部链上所有权/结算事实。游戏内普通治理权只能来自该事项范围内可审计的锁定、snapshot 与明确退出/解除绑定过程。玩家必须能分别读取外部 OC receipt、事项锁定/snapshot/退出状态以及游戏内权利的当前结果；任何一方没有 authority 时保持 `incomplete / unknown / blocked`。

进入有效窗口后，后续外部转让、反向转移或解除绑定不能追溯改票、重复投票或让同一权利同时受两个控制关系行使。事项权利的成功不能把 OC 变成不可转让资产，OC 转让的成功也不能把世界资源、区域权利、生产/治理资格或行动结果写成自动获得。

### 4.2 单向非治理额度桥

现有 operator-managed `OC -> LetAI Run quota` 桥只提供声明范围内的服务额度。玩家可以查看额度来源、服务用途、消费状态和下一次可复查点，但不能把额度理解为 OC 兑换、自动提现、AMM、治理权或世界资产。

`token_key`、额度数量或服务消费不得影响普通治理资格、控制人聚合、权重、事项快照或已确认结果；反向地，游戏内资源、资格和治理 receipt 也不自动铸造、转让或赎回 OC。消费失败、额度过期或 bridge authority 缺失时保留原外部/游戏内事实，返回服务侧补证、等待或独立替代路径，不产生治理副作用。

## 5. 专业验收

### <a id="ac-game-019-01"></a>AC-GAME-019-01：治理主体聚合不因绑定拆分而扩大影响

给定一个账户包含多个已批准身份、managed signer 或 external wallet binding，或多个账户/组织通过拆分、重复绑定和循环转移表达同一实际控制关系，玩家能看到聚合后的治理主体、适用授权范围和当前影响边界。拆分、短期转移和旧缓存不增加普通治理影响；绑定冲突或 authority 缺失时结果为 `incomplete / unknown / blocked`，不生成猜测资格。

### <a id="ac-game-019-02"></a>AC-GAME-019-02：锁定、委托与实际控制人封顶形成可读资格取舍

给定一个普通事项，玩家能比较事项范围内的线性锁定 OC、可撤回委托、实际控制人聚合上限、授权来源和当前窗口，并知道哪些相邻本地贡献条件属于区域专业合同。仅满足单一条件、持有 OC、短时到访、历史头衔或资本集中不能取得无限控制；条件不足时显示原因、保留事实与补证/等待/独立路线。

### <a id="ac-game-019-03"></a>AC-GAME-019-03：公开审计和重复主体纠错保留隐私与历史

给定公开委托、最终计票/结果或重复主体 challenge，玩家能读取必要的治理关系、处理状态、纠错理由和最终有效结果，但看不到非必要个人数据。review、适用 merge/revocation、计票 correction 和 appeal 形成一条可追踪链；任何纠错、驳回、重连或 replay 都不抹除原授权、投票和 receipt，也不产生第二次治理效果。

### <a id="ac-game-019-04"></a>AC-GAME-019-04：事项快照阻止转让后的追溯改票与双投

给定一次外部 OC 转让或反向转移、一个已进入有效窗口的普通事项，以及退出/解除绑定请求，玩家能区分外部资产 receipt、事项范围内锁定/snapshot、退出状态和游戏内权利结果。有效窗口后的转让或解除不追溯改票、不产生双投、不让同一权利同时在两个控制关系下生效；快照过期或 authority 缺失时只能重评、过期、拒绝或保持待决。

### <a id="ac-game-019-05"></a>AC-GAME-019-05：单向额度桥不旁路普通治理

给定 OC→quota 服务额度绑定、`token_key` 或服务消费记录，玩家能分辨额度的服务用途与治理事项的锁定/snapshot 权利。额度不能兑换 OC、提高治理资格/权重、改变控制人聚合、制造游戏内治理效果或替代事项授权；额度桥或其 authority 缺失时不得用默认值补齐。

### <a id="ac-game-019-06"></a>AC-GAME-019-06：跨 surface 失败恢复保持因果与 exactly-once

Viewer、pure API 与 Agent 面对同一权威快照时，三者对治理主体、资格 blocker、委托范围、challenge 状态、事项快照、外部转让、额度桥、`next_action` 和 `next_recheck` 给出等义解释。提交前预览不产生效果；授权/范围漂移、重复提交、重连、Agent retry、并发 challenge 和 replay 至多保留一份可归因结果，并提供补证、撤回、重提、等待、申诉或独立服务路径。本文仅定义玩法验收目标，不是当前实现或 release 证据。

## 6. 权威边界与验证切线

| 语义 | 本合同拥有 | 其他 authority |
| --- | --- | --- |
| 玩家动作与取舍 | 治理主体检查、资格比较、委托/撤回、challenge、纠错/申诉、事项快照检查、外部转让与额度桥的语义分离 | 产品 GCB-004/005 拥有世界承诺与保护边界 |
| 区域与工业相邻玩法 | 只引用本合同对普通治理的边界，不重新定义区域双资格、tenure、levy、工业准入或市场交付 | `PRD-GAME-017`、`PRD-GAME-018` |
| 因果反馈 | 要求治理动作提供可读 accepted / blocked / corrected / applied / not-applied 的下一步和 receipt 因果 | `PRD-GAME-014` 拥有 reusable causal-decision receipt |
| 执行事实 | 不定义字段、公式、阈值、状态机、快照算法、链上交易、签名/custody、额度结算或 rollback | `world-runtime`、`p2p` / blockchain authority 与相应 system design |
| 表达与发布判断 | 规定玩家应能比较的事实、代价、失败恢复和 parity 目标，不给 release verdict | Viewer/API/Agent 负责表达实现；QA 与 `doc/testing/prd.md` 负责验证和当前结论 |

## 7. 玩法风险与验证期待

- `sybil_split`：多个账户、组织、signer 或钱包绑定被误认为多个独立主体，放大影响；required smoke 必须有聚合与循环转移负例。
- `privacy_overreach`：为了防重复控制而公开过多个人或控制关系数据；验证应证明公开审计足够完成决策，同时保持最小化私下断言。
- `delegation_lock_in`：委托范围、撤回窗口或代表结果不可读，玩家无法恢复自己的下一步；验证应覆盖委托失效、撤回和重提。
- `retroactive_vote`：转让、反向转移、解除绑定或旧 snapshot 被错误当成新权利；验证应覆盖非追溯、非双投与 exactly-once。
- `quota_governance_bypass`：额度、`token_key` 或服务消费被误写成资格、权重或治理效果；必须有桥成功但治理资格不变的负例。
- `early_gate_drift`：普通治理被包装成首局或独立成长的必经门槛；本合同只支持中长期、可退出的共同决策参与，不改变 `PRD-GAME-012` 的早期 gate。

`test_tier_required` 应覆盖六项 AC 的正常、权威缺失、范围漂移、重复提交和恢复样例；`test_tier_full` 再覆盖跨节点身份/签名授权、持久化、并发 challenge、snapshot/replay、跨 surface parity 和额度桥负例。上述是玩法验收期待，不是当前测试通过或当前可用性声明。

## 8. Non-goals

- 不定义锁定比例/期限、权重、阈值、控制人计算、身份聚合技术、隐私机制、投票/委托 schema、snapshot/unbonding 算法或任何经济参数。
- 不实现账户绑定、治理投票、challenge/review、merge/revocation、correction、appeal、链上转账、signer/custody、额度结算、runtime/P2P 状态机或 Viewer/API 控件。
- 不把 OC 持有、外部转让、事项通过、投票支持、链上 receipt、额度消费或本合同本身写成游戏内权利、当前功能、主网、发行或公开 claim。
- 不复制 `PRD-GAME-017` 的区域制度、不复制 `PRD-GAME-018` 的工业/市场结算，也不改变四个产品模块、既有目录 slug 或 Product PRD-ID。

## 9. 全量语义追踪

| Gameplay AC | 产品要求 | 专业协作者 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| AC-GAME-019-01 / 02 / 03 | [`REQ-WR-GCB-004`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004) / [`AC-WR-GCB-004`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#ac-wr-gcb-004) | `producer_system_designer`、`runtime_engineer`、`blockchain_ops_engineer`、`qa_engineer` | 治理主体聚合、资格/委托/控制人上限、最小公开审计、重复主体纠错/申诉与历史 receipt 的跨 surface 证据 | `test_tier_required` + `test_tier_full` |
| AC-GAME-019-04 / 05 | [`REQ-WR-GCB-005`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005) / [`AC-WR-GCB-005`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#ac-wr-gcb-005) | `producer_system_designer`、`runtime_engineer`、`blockchain_ops_engineer`、`qa_engineer` | OC 外部转让、事项锁定/snapshot/退出隔离、非追溯/非双投和单向非治理 quota bridge 负例 | `test_tier_required` + `test_tier_full` |
| AC-GAME-019-06 | GCB-004 / GCB-005 与 [`PRD-GAME-014`](gameplay-indirect-control-agency-contract.prd.md) | `runtime_engineer`、`viewer_engineer`、`agent_engineer`、`qa_engineer` | 同一权威快照下的玩家状态、失败恢复、receipt 因果与 exactly-once parity | `test_tier_required` + `test_tier_full` |
