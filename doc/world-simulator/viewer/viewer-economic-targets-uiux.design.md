# Viewer 经济与工业条件目标 UI/UX 设计

- Professional owner: `game_visual_interaction_designer`；Integration owner: `tpm`。
- 状态：专业呈现与交互补充；源证据与目标稿分开。尚未完成的图稿关系保持明确缺口，不是功能已发布声明。
- 固定源：PR #5277 `113868200d1f91bef1008babb689bd257f241428`；产品/Viewer源与审读main `5cc5c2b8a84c22f80cfa9ee16d893eaf6deedb08` 对应subtree相同。
- 配套：[派生目录](viewer-full-function-coverage-catalog.md)、[既有页面总览](viewer-page-module-design-2026-06-18.design.md#detailed-uiux-handoff)。目录不是产品authority。

## 1. 问题、目标与非目标

本设计把已有源的字段、动作、条件及未解决的目标合同分开，使玩家能读清观察对象、当前权限、下一步与原结果。具体引用仅承接表中适用义务；组件存在、图稿数量、accepted和设计覆盖都不等于世界效果或运行验收。

不新增玩法/认证/治理能力，不改变后端权限、报价或结算语义；不代替产品PRD、实际服务capability或真实试玩。

## 2. 上游约束与相关角色

产品语义由各行唯一上游声明拥有；Viewer负责呈现、交互与失败/恢复信息，不接管identity、runtime、world或治理authority。现有品牌和终端层级继续适用。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [root-sc20-jobdispose](../../product/world-rules-core-gameplay/prd.md#root-sc20-jobdispose)；Trace refs: O7-REQ-eacc6e2c-a75b-4c2b-b32b-f24e15832f95 | 条件目标读面：未消费 hold, WIP, in-transit, buffer-held；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc20-jobdispose](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc20-jobdispose) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc20-jobcontinue](../../product/world-rules-core-gameplay/prd.md#root-sc20-jobcontinue)；Trace refs: O7-REQ-15abc2d8-2dc7-43ed-bb20-895dc11e8452 | 条件目标读面：原料 A → join, 原料 B → join, join → WIP, WIP → transit → buffer；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc20-jobcontinue](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc20-jobcontinue) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc21-batch](../../product/world-rules-core-gameplay/prd.md#root-sc21-batch)；Trace refs: O7-REQ-93f437ea-48eb-4f07-9af2-751faac09ec7 | 条件目标读面：原料 A → join, 原料 B → join, join → WIP, WIP → transit → buffer；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc21-batch](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-batch) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc21-join](../../product/world-rules-core-gameplay/prd.md#root-sc21-join)；Trace refs: O7-REQ-6e7590d6-923d-407a-93df-eef373cccaea | 条件目标读面：原料 A → join, 原料 B → join, join → WIP, WIP → transit → buffer；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc21-join](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-join) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc21-capacity](../../product/world-rules-core-gameplay/prd.md#root-sc21-capacity)；Trace refs: O7-REQ-7f40ee7a-192f-4579-a69d-9946a6e4d5bf | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc21-capacity](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-capacity) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc21-service](../../product/world-rules-core-gameplay/prd.md#root-sc21-service)；Trace refs: O7-REQ-a3562631-35fa-4581-86e3-589e530ad253 | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc21-service](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-service) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc21-diagnostic](../../product/world-rules-core-gameplay/prd.md#root-sc21-diagnostic)；Trace refs: O7-REQ-3373ec45-3ce8-4539-8bdd-f85acca2c252 | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc21-diagnostic](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-diagnostic) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc21-quantum](../../product/world-rules-core-gameplay/prd.md#root-sc21-quantum)；Trace refs: O7-REQ-baa1e6b2-1a46-47a3-b75c-a0e9cf8b154c | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc21-quantum](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-quantum) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc21-fanout](../../product/world-rules-core-gameplay/prd.md#root-sc21-fanout)；Trace refs: O7-REQ-885f0bc7-d049-49f2-bff4-a372ee06489c | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc21-fanout](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-fanout) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc21-backpressure](../../product/world-rules-core-gameplay/prd.md#root-sc21-backpressure)；Trace refs: O7-REQ-65e2849e-3b51-4440-a5dd-78cdd7c0692a | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc21-backpressure](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-backpressure) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc21-cutover](../../product/world-rules-core-gameplay/prd.md#root-sc21-cutover)；Trace refs: O7-REQ-38521519-d028-4768-abc5-723e239192c4 | 条件目标读面：未消费 hold, WIP, in-transit, buffer-held；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc21-cutover](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-cutover) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc21-terminal](../../product/world-rules-core-gameplay/prd.md#root-sc21-terminal)；Trace refs: O7-REQ-012b7904-2468-48c6-996d-14698aa7f798 | 条件目标读面：未消费 hold, WIP, in-transit, buffer-held；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [root-sc21-terminal](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-terminal) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc22-maintchoice](../../product/world-rules-core-gameplay/prd.md#root-sc22-maintchoice)；Trace refs: O7-REQ-cfbe031b-99ce-419b-a3b4-bc63e066e085 | 条件目标读面：maintain_before_run, run_at_risk / reduce_load, upgrade / reconfigure, retire_and_rebuild；当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。 | [root-sc22-maintchoice](viewer-economic-targets-uiux.design.md#t-eco-02-root-sc22-maintchoice) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc22-maintunknown](../../product/world-rules-core-gameplay/prd.md#root-sc22-maintunknown)；Trace refs: O7-REQ-2684e207-9424-41df-93a5-80bd8f541251 | 条件目标读面：maintain_before_run, run_at_risk / reduce_load, upgrade / reconfigure, retire_and_rebuild；当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。 | [root-sc22-maintunknown](viewer-economic-targets-uiux.design.md#t-eco-02-root-sc22-maintunknown) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc25-factorychoice](../../product/world-rules-core-gameplay/prd.md#root-sc25-factorychoice)；Trace refs: O7-REQ-8a215d9f-cefc-4d06-82f3-935c44454558 | 条件目标读面：maintain_before_run, run_at_risk / reduce_load, upgrade / reconfigure, retire_and_rebuild；当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。 | [root-sc25-factorychoice](viewer-economic-targets-uiux.design.md#t-eco-02-root-sc25-factorychoice) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc25-factorydispose](../../product/world-rules-core-gameplay/prd.md#root-sc25-factorydispose)；Trace refs: O7-REQ-aa02140c-4e3d-411e-97ba-28ad19830c5b | 条件目标读面：未消费 hold, WIP, in-transit, buffer-held；当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。 | [root-sc25-factorydispose](viewer-economic-targets-uiux.design.md#t-eco-02-root-sc25-factorydispose) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc25-factorystate](../../product/world-rules-core-gameplay/prd.md#root-sc25-factorystate)；Trace refs: O7-REQ-df369b38-4338-4207-b6e0-60cfba3f12cb | 条件目标读面：maintain_before_run, run_at_risk / reduce_load, upgrade / reconfigure, retire_and_rebuild；当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。 | [root-sc25-factorystate](viewer-economic-targets-uiux.design.md#t-eco-02-root-sc25-factorystate) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc23-buildquote](../../product/world-rules-core-gameplay/prd.md#root-sc23-buildquote)；Trace refs: O7-REQ-4507fa71-95c4-404a-969a-c15c08dbeccc | 条件目标读面：site / location / owner, access / chunk / active, construction inputs, owner-held electricity；builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。 | [root-sc23-buildquote](viewer-economic-targets-uiux.design.md#t-eco-03-root-sc23-buildquote) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc23-buildpower](../../product/world-rules-core-gameplay/prd.md#root-sc23-buildpower)；Trace refs: O7-REQ-8931e5de-af90-4c6a-8c2e-d21930e1a4ef | 条件目标读面：start-only sink, held-through-completion/arrival, boundary-revalidated conditional, non-reserved；builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。 | [root-sc23-buildpower](viewer-economic-targets-uiux.design.md#t-eco-03-root-sc23-buildpower) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc29-sitecompare](../../product/world-rules-core-gameplay/prd.md#root-sc29-sitecompare)；Trace refs: O7-REQ-982fc046-c8dc-43d1-bc0c-8b6bd3e0244b | 条件目标读面：site / location / owner, access / chunk / active, construction inputs, owner-held electricity；builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。 | [root-sc29-sitecompare](viewer-economic-targets-uiux.design.md#t-eco-03-root-sc29-sitecompare) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc29-siteadvisory](../../product/world-rules-core-gameplay/prd.md#root-sc29-siteadvisory)；Trace refs: O7-REQ-328d5c5c-24d4-4bf6-bfd5-d1f968c9fdd2 | 条件目标读面：site / location / owner, access / chunk / active, construction inputs, owner-held electricity；builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。 | [root-sc29-siteadvisory](viewer-economic-targets-uiux.design.md#t-eco-03-root-sc29-siteadvisory) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc24-recipestate](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipestate)；Trace refs: O7-REQ-e912cdd3-0222-4a88-b920-983a4118a6d1 | 条件目标读面：pending_validation, validated_pending_admission, active / restricted, retiring；观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。 | [root-sc24-recipestate](viewer-economic-targets-uiux.design.md#t-eco-04-root-sc24-recipestate) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc24-recipeversion](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipeversion)；Trace refs: O7-REQ-70481f2f-9b55-4b99-afde-b4d451843d1d | 条件目标读面：pending_validation, validated_pending_admission, active / restricted, retiring；观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。 | [root-sc24-recipeversion](viewer-economic-targets-uiux.design.md#t-eco-04-root-sc24-recipeversion) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc24-recipedispose](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipedispose)；Trace refs: O7-REQ-df2f1a58-f435-4e86-b6c1-810f13f8267d | 条件目标读面：pending_validation, validated_pending_admission, active / restricted, retiring；观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。 | [root-sc24-recipedispose](viewer-economic-targets-uiux.design.md#t-eco-04-root-sc24-recipedispose) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc27-discover](../../product/world-rules-core-gameplay/prd.md#root-sc27-discover)；Trace refs: O7-REQ-d0c8d71e-dae0-405f-8b01-c6d573eaf719 | 条件目标读面：candidate A, candidate B, candidate C, 代表性档案；观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。 | [root-sc27-discover](viewer-economic-targets-uiux.design.md#t-eco-04-root-sc27-discover) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117 | 条件目标读面：candidate A, candidate B, candidate C, 代表性档案；观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。 | [root-sc28-profile](viewer-economic-targets-uiux.design.md#t-eco-04-root-sc28-profile) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc30-subchoice](../../product/world-rules-core-gameplay/prd.md#root-sc30-subchoice)；Trace refs: O7-REQ-8a79f47e-f132-4467-8d09-d0ceacaac868 | 条件目标读面：same_material_source, profile_substitute, new_recipe, decision → conversion WIP；当前输入/lot owner 与受权执行者；替代 decision 权限不自动扩展至 conversion/cutover。 | [root-sc30-subchoice](viewer-economic-targets-uiux.design.md#t-eco-05-root-sc30-subchoice) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc30-subsettle](../../product/world-rules-core-gameplay/prd.md#root-sc30-subsettle)；Trace refs: O7-REQ-300c6cbb-49f5-4858-a749-309991d5a5d0 | 条件目标读面：same_material_source, profile_substitute, new_recipe, decision → conversion WIP；当前输入/lot owner 与受权执行者；替代 decision 权限不自动扩展至 conversion/cutover。 | [root-sc30-subsettle](viewer-economic-targets-uiux.design.md#t-eco-05-root-sc30-subsettle) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [root-sc30-subcause](../../product/world-rules-core-gameplay/prd.md#root-sc30-subcause)；Trace refs: O7-REQ-84ea0d88-f33b-4ac1-9722-ca4892df9152 | 条件目标读面：same_material_source, profile_substitute, new_recipe, decision → conversion WIP；当前输入/lot owner 与受权执行者；替代 decision 权限不自动扩展至 conversion/cutover。 | [root-sc30-subcause](viewer-economic-targets-uiux.design.md#t-eco-05-root-sc30-subcause) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-SC31-001](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-001)；Trace refs: O7-REQ-bfd4b2f4-6b21-4c4a-961d-387b220e762f | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [req-sc31-001](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-001) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-SC31-002](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-002)；Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060 | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [req-sc31-002](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-002) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-SC31-003](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-003)；Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0 | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [req-sc31-003](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-003) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-SC31-004](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-004)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [req-sc31-004](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-004) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-SC31-005](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-005)；Trace refs: O7-REQ-62129528-2238-4733-8b8e-d226dc40943d | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [req-sc31-005](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-005) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-SC31-006](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-006)；Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [req-sc31-006](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-006) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-SC31-007](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-007)；Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [req-sc31-007](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-007) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-SC31-008](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-008)；Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3 | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [req-sc31-008](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-008) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-SC31-009](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009)；Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | 条件目标读面：需求发现, 运输 / arrival, 接收验收, ownership / escrow；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [req-sc31-009](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-009) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001)；Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | 条件目标读面：危机证据, authorization, 冻结分配, 数量追踪；beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。 | [req-wr-es-001](viewer-economic-targets-uiux.design.md#t-eco-07-req-wr-es-001) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-002)；Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7 | 条件目标读面：危机证据, authorization, 冻结分配, 数量追踪；beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。 | [req-wr-es-002](viewer-economic-targets-uiux.design.md#t-eco-07-req-wr-es-002) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-003)；Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1 | 条件目标读面：危机证据, authorization, 冻结分配, 数量追踪；beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。 | [req-wr-es-003](viewer-economic-targets-uiux.design.md#t-eco-07-req-wr-es-003) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004)；Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3 | 条件目标读面：危机证据, authorization, 冻结分配, 数量追踪；beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。 | [req-wr-es-004](viewer-economic-targets-uiux.design.md#t-eco-07-req-wr-es-004) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-AOS-003](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-003)；Trace refs: O7-REQ-f247bdc7-f4ae-4893-a35e-6840fab7f465 | 条件目标读面：Agent 经济转让, 费用 / levy, 有限机会, 长期经济审计；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [req-wr-aos-003](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-aos-003) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 条件目标读面：只读 quote, 确认 / 预留, 开工 / 完成, 交付；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [req-wr-aos-004](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-aos-004) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | 条件目标读面：Agent 经济转让, 费用 / levy, 有限机会, 长期经济审计；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [req-wr-aos-005](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-aos-005) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-MW-003](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-003)；Trace refs: O7-REQ-7e2cd84b-9145-4e2f-b3c2-f9c451cec6c5 | 条件目标读面：Agent 经济转让, 费用 / levy, 有限机会, 长期经济审计；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [req-wr-mw-003](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-mw-003) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026 | 条件目标读面：Agent 经济转让, 费用 / levy, 有限机会, 长期经济审计；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [req-wr-mw-004](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-mw-004) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5 | 条件目标读面：Agent 经济转让, 费用 / levy, 有限机会, 长期经济审计；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [req-wr-mw-005](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-mw-005) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |

## 3. 当前状态、目标状态与差距

`implemented_in_frozen_source` 只表示固定源码有实际消费者；条件显示不是未实现。`target_only` 只表示已有产品义务的设计目标；未确认专用Viewer入口不证明全仓后端不存在。QA与禁用占位不进入普通玩家当前功能分母。图中的示例数值不是真实世界结果。

## 4. 边界与结构

E09 的表格是放大的条件目标字段展开；[平板 shell 组合稿](assets/ui-ux/15-tablet-text-reflow.svg#tablet)展示相同替代方案主题在按需 Command 详情中的主次：世界/观察 B、原操作 A、Next Move 和关闭路径共存。此稿只画“方案是否可执行、能力与提交方式待确认”的组合，不表示已确认没有可行方案，不证明全部 E01–E12 主题、所有状态或响应式完整；技术枚举留在展开依据中，玩家入口称“查看当前替代方案”。

World / Targets / Command 是三个主Dock；Search在Targets内，Diagnostics/More为次级。Bevy是唯一世界renderer；图稿占位不是JS世界回退。当前主体、观察目标、模型连接、账号、session、admission、control分别表述。

## 5. 关键交互流程

先读对象与可用条件，再编辑草稿/请求预估，只有实际capability允许时才显式提交。pending/unknown保留原操作身份；切换观察对象不改写原回执。关闭弹层返回invoker，焦点/IME不得误触提交。具体feature条款优先于此共同顺序。

## 6. 条件目标的字段、动作与状态


以下读面是目标合同呈现；当前窄源关系和未来完整义务分开。

<a id="t-eco-01-root-sc20-jobdispose"></a>
### 条款 T-ECO-01 / root-sc20-jobdispose
- 唯一上游：[root-sc20-jobdispose](../../product/world-rules-core-gameplay/prd.md#root-sc20-jobdispose)；Trace refs: O7-REQ-eacc6e2c-a75b-4c2b-b32b-f24e15832f95。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：未消费 hold / hold / release / replan：依 profile / 数量、原 reservation；WIP / finish / pause / rework / salvage：依 profile / 已消费量、损失、parent receipt；in-transit / finish / hold / return / reroute：依 profile / 原 route、destination、损耗；buffer-held / hold / handoff / conversion：依 profile / custody、容量占用；terminal-pending / 原承诺 / hold / return：依 profile / recipient、admission、未结义务
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：展开处置差异；确认换线（禁用：缺执行合同）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E03-cutover-terminal.svg)；`normal_frame`；区域 `R1 / old work disposition`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc20-jobcontinue"></a>
### 条款 T-ECO-01 / root-sc20-jobcontinue
- 唯一上游：[root-sc20-jobcontinue](../../product/world-rules-core-gameplay/prd.md#root-sc20-jobcontinue)；Trace refs: O7-REQ-15abc2d8-2dc7-43ed-bb20-895dc11e8452。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：原料 A → join / 已到达 · applicability 待核 / 到达 receipt ≠ 可消费；原料 B → join / missing_parent · 数量待权威 / atomic kit 齐套前不 sink；join → WIP / 未开始 · 不计稳定进度 / staged 仅支持整 parent slot；WIP → transit → buffer / 各自保留原 ledger / lineage / 容量与 custody 独立核验；buffer → terminal / 生产已结算仍可待交付 / 匹配 delivery receipt 才减需求
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看原请求（只读目标）；续接（合同能力待发布）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E01-work-graph.svg)；`normal_frame`；区域 `R1/R3 identity`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-work-graph-and-recipe-lifecycle.design.md](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#industrial-identity)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc21-batch"></a>
### 条款 T-ECO-01 / root-sc21-batch
- 唯一上游：[root-sc21-batch](../../product/world-rules-core-gameplay/prd.md#root-sc21-batch)；Trace refs: O7-REQ-93f437ea-48eb-4f07-9af2-751faac09ec7。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：原料 A → join / 已到达 · applicability 待核 / 到达 receipt ≠ 可消费；原料 B → join / missing_parent · 数量待权威 / atomic kit 齐套前不 sink；join → WIP / 未开始 · 不计稳定进度 / staged 仅支持整 parent slot；WIP → transit → buffer / 各自保留原 ledger / lineage / 容量与 custody 独立核验；buffer → terminal / 生产已结算仍可待交付 / 匹配 delivery receipt 才减需求
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看原请求（只读目标）；续接（合同能力待发布）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E01-work-graph.svg)；`normal_frame`；区域 `rows 1–2 batch applicability`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc21-join"></a>
### 条款 T-ECO-01 / root-sc21-join
- 唯一上游：[root-sc21-join](../../product/world-rules-core-gameplay/prd.md#root-sc21-join)；Trace refs: O7-REQ-6e7590d6-923d-407a-93df-eef373cccaea。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：原料 A → join / 已到达 · applicability 待核 / 到达 receipt ≠ 可消费；原料 B → join / missing_parent · 数量待权威 / atomic kit 齐套前不 sink；join → WIP / 未开始 · 不计稳定进度 / staged 仅支持整 parent slot；WIP → transit → buffer / 各自保留原 ledger / lineage / 容量与 custody 独立核验；buffer → terminal / 生产已结算仍可待交付 / 匹配 delivery receipt 才减需求
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看原请求（只读目标）；续接（合同能力待发布）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E01-work-graph.svg)；`normal_frame`；区域 `rows 1–3 canonical join`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-work-graph-and-recipe-lifecycle.design.md](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#industrial-join)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc21-capacity"></a>
### 条款 T-ECO-01 / root-sc21-capacity
- 唯一上游：[root-sc21-capacity](../../product/world-rules-core-gameplay/prd.md#root-sc21-capacity)；Trace refs: O7-REQ-7f40ee7a-192f-4579-a69d-9946a6e4d5bf。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：stage slot / available / held / consumed：待权威单位 / full / partial / deferred / denied；edge throughput / 请求 / 合法 / 已分配 / executable 分开 / unmet / residual 不计产出；buffer / finite limit / occupied / held / peak / 满额：上游 hold 或拒绝；terminal / owner / recipient / admission / held / 生产及时 ≠ 终端按期；service window / start / target / end：world tick / none / soft / hard；未预留不保证
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看 reservation chain；续期（权限和能力待核）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E02-capacity-window.svg)；`normal_frame`；区域 `rows 1–2 allocation`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-work-graph-and-recipe-lifecycle.design.md](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#industrial-allocation)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc21-service"></a>
### 条款 T-ECO-01 / root-sc21-service
- 唯一上游：[root-sc21-service](../../product/world-rules-core-gameplay/prd.md#root-sc21-service)；Trace refs: O7-REQ-a3562631-35fa-4581-86e3-589e530ad253。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：stage slot / available / held / consumed：待权威单位 / full / partial / deferred / denied；edge throughput / 请求 / 合法 / 已分配 / executable 分开 / unmet / residual 不计产出；buffer / finite limit / occupied / held / peak / 满额：上游 hold 或拒绝；terminal / owner / recipient / admission / held / 生产及时 ≠ 终端按期；service window / start / target / end：world tick / none / soft / hard；未预留不保证
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看 reservation chain；续期（权限和能力待核）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E02-capacity-window.svg)；`normal_frame`；区域 `row 5 service window`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-work-graph-and-recipe-lifecycle.design.md](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#industrial-window)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc21-diagnostic"></a>
### 条款 T-ECO-01 / root-sc21-diagnostic
- 唯一上游：[root-sc21-diagnostic](../../product/world-rules-core-gameplay/prd.md#root-sc21-diagnostic)；Trace refs: O7-REQ-3373ec45-3ce8-4539-8bdd-f85acca2c252。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：stage slot / available / held / consumed：待权威单位 / full / partial / deferred / denied；edge throughput / 请求 / 合法 / 已分配 / executable 分开 / unmet / residual 不计产出；buffer / finite limit / occupied / held / peak / 满额：上游 hold 或拒绝；terminal / owner / recipient / admission / held / 生产及时 ≠ 终端按期；service window / start / target / end：world tick / none / soft / hard；未预留不保证
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看 reservation chain；续期（权限和能力待核）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E02-capacity-window.svg)；`normal_frame`；区域 `R1 attribution`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-work-graph-and-recipe-lifecycle.design.md](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#industrial-diagnostic)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc21-quantum"></a>
### 条款 T-ECO-01 / root-sc21-quantum
- 唯一上游：[root-sc21-quantum](../../product/world-rules-core-gameplay/prd.md#root-sc21-quantum)；Trace refs: O7-REQ-baa1e6b2-1a46-47a3-b75c-a0e9cf8b154c。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：stage slot / available / held / consumed：待权威单位 / full / partial / deferred / denied；edge throughput / 请求 / 合法 / 已分配 / executable 分开 / unmet / residual 不计产出；buffer / finite limit / occupied / held / peak / 满额：上游 hold 或拒绝；terminal / owner / recipient / admission / held / 生产及时 ≠ 终端按期；service window / start / target / end：world tick / none / soft / hard；未预留不保证
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看 reservation chain；续期（权限和能力待核）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E02-capacity-window.svg)；`text_spec_only`；区域 `row 2 / R3 quantum`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc21-fanout"></a>
### 条款 T-ECO-01 / root-sc21-fanout
- 唯一上游：[root-sc21-fanout](../../product/world-rules-core-gameplay/prd.md#root-sc21-fanout)；Trace refs: O7-REQ-885f0bc7-d049-49f2-bff4-a372ee06489c。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：stage slot / available / held / consumed：待权威单位 / full / partial / deferred / denied；edge throughput / 请求 / 合法 / 已分配 / executable 分开 / unmet / residual 不计产出；buffer / finite limit / occupied / held / peak / 满额：上游 hold 或拒绝；terminal / owner / recipient / admission / held / 生产及时 ≠ 终端按期；service window / start / target / end：world tick / none / soft / hard；未预留不保证
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看 reservation chain；续期（权限和能力待核）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E02-capacity-window.svg)；`text_spec_only`；区域 `R3 output bundle`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc21-backpressure"></a>
### 条款 T-ECO-01 / root-sc21-backpressure
- 唯一上游：[root-sc21-backpressure](../../product/world-rules-core-gameplay/prd.md#root-sc21-backpressure)；Trace refs: O7-REQ-65e2849e-3b51-4440-a5dd-78cdd7c0692a。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：stage slot / available / held / consumed：待权威单位 / full / partial / deferred / denied；edge throughput / 请求 / 合法 / 已分配 / executable 分开 / unmet / residual 不计产出；buffer / finite limit / occupied / held / peak / 满额：上游 hold 或拒绝；terminal / owner / recipient / admission / held / 生产及时 ≠ 终端按期；service window / start / target / end：world tick / none / soft / hard；未预留不保证
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看 reservation chain；续期（权限和能力待核）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E02-capacity-window.svg)；`normal_frame`；区域 `row 3 buffer`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-work-graph-and-recipe-lifecycle.design.md](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#industrial-backpressure-terminal)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc21-cutover"></a>
### 条款 T-ECO-01 / root-sc21-cutover
- 唯一上游：[root-sc21-cutover](../../product/world-rules-core-gameplay/prd.md#root-sc21-cutover)；Trace refs: O7-REQ-38521519-d028-4768-abc5-723e239192c4。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：未消费 hold / hold / release / replan：依 profile / 数量、原 reservation；WIP / finish / pause / rework / salvage：依 profile / 已消费量、损失、parent receipt；in-transit / finish / hold / return / reroute：依 profile / 原 route、destination、损耗；buffer-held / hold / handoff / conversion：依 profile / custody、容量占用；terminal-pending / 原承诺 / hold / return：依 profile / recipient、admission、未结义务
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：展开处置差异；确认换线（禁用：缺执行合同）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E03-cutover-terminal.svg)；`normal_frame`；区域 `rows 1–5`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-work-graph-and-recipe-lifecycle.design.md](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#industrial-cutover)；不是当前Viewer DTO开放证明。

<a id="t-eco-01-root-sc21-terminal"></a>
### 条款 T-ECO-01 / root-sc21-terminal
- 唯一上游：[root-sc21-terminal](../../product/world-rules-core-gameplay/prd.md#root-sc21-terminal)；Trace refs: O7-REQ-012b7904-2468-48c6-996d-14698aa7f798。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。。
- 字段：未消费 hold / hold / release / replan：依 profile / 数量、原 reservation；WIP / finish / pause / rework / salvage：依 profile / 已消费量、损失、parent receipt；in-transit / finish / hold / return / reroute：依 profile / 原 route、destination、损耗；buffer-held / hold / handoff / conversion：依 profile / custody、容量占用；terminal-pending / 原承诺 / hold / return：依 profile / recipient、admission、未结义务
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：展开处置差异；确认换线（禁用：缺执行合同）。
- 状态规范：pending：无 root/顺位/隐式 hold；查原 intent；partial kit：已消费 parent、未消费 hold、remaining 分列；capacity deferred：显示原接受身份、policy reason、next recheck；commit_unknown：查询原 root / receipt，不重建请求；terminal failure：保留 production 与有限占用，不自动退款。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E03-cutover-terminal.svg)；`normal_frame`；区域 `row 5 / R2`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-work-graph-and-recipe-lifecycle.design.md](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#industrial-backpressure-terminal)；不是当前Viewer DTO开放证明。

<a id="t-eco-02-root-sc22-maintchoice"></a>
### 条款 T-ECO-02 / root-sc22-maintchoice
- 唯一上游：[root-sc22-maintchoice](../../product/world-rules-core-gameplay/prd.md#root-sc22-maintchoice)；Trace refs: O7-REQ-cfbe031b-99ce-419b-a3b4-bc63e066e085。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。。
- 字段：maintain_before_run / 追加资源、停机时延：待权威 / 维护范围、失败损失与撤回性；run_at_risk / reduce_load / 仍占用、预期产出：待权威 / 只有 profile 明示 at-risk 才可选；upgrade / reconfigure / 新能力 / recipe fit / throughput / fresh owner、power、物流、terminal；retire_and_rebuild / 旧工作五 bucket 逐项处置 / 不继承旧 W / reservation；defer / current capability / 保留原任务与未结算状态 / 不创建维护或能力效果
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看五类旧工作；确认变更（禁用：专业合同未闭合）。
- 状态规范：maintenance_not_tracked：不推断安全；无受支持选择：动作禁用，保留当前能力读面；reconfiguration_pending / reconfiguring：不等于能力生效；unknown：原 change identity 核对，不重提；完成：重新读取前后状态与单次 disposition。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E04-maintenance-retrofit.svg)；`normal_frame`；区域 `rows 1–2 / row 5`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-02-root-sc22-maintunknown"></a>
### 条款 T-ECO-02 / root-sc22-maintunknown
- 唯一上游：[root-sc22-maintunknown](../../product/world-rules-core-gameplay/prd.md#root-sc22-maintunknown)；Trace refs: O7-REQ-2684e207-9424-41df-93a5-80bd8f541251。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。。
- 字段：maintain_before_run / 追加资源、停机时延：待权威 / 维护范围、失败损失与撤回性；run_at_risk / reduce_load / 仍占用、预期产出：待权威 / 只有 profile 明示 at-risk 才可选；upgrade / reconfigure / 新能力 / recipe fit / throughput / fresh owner、power、物流、terminal；retire_and_rebuild / 旧工作五 bucket 逐项处置 / 不继承旧 W / reservation；defer / current capability / 保留原任务与未结算状态 / 不创建维护或能力效果
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看五类旧工作；确认变更（禁用：专业合同未闭合）。
- 状态规范：maintenance_not_tracked：不推断安全；无受支持选择：动作禁用，保留当前能力读面；reconfiguration_pending / reconfiguring：不等于能力生效；unknown：原 change identity 核对，不重提；完成：重新读取前后状态与单次 disposition。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E04-maintenance-retrofit.svg)；`normal_frame`；区域 `R2 / R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-02-root-sc25-factorychoice"></a>
### 条款 T-ECO-02 / root-sc25-factorychoice
- 唯一上游：[root-sc25-factorychoice](../../product/world-rules-core-gameplay/prd.md#root-sc25-factorychoice)；Trace refs: O7-REQ-8a215d9f-cefc-4d06-82f3-935c44454558。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。。
- 字段：maintain_before_run / 追加资源、停机时延：待权威 / 维护范围、失败损失与撤回性；run_at_risk / reduce_load / 仍占用、预期产出：待权威 / 只有 profile 明示 at-risk 才可选；upgrade / reconfigure / 新能力 / recipe fit / throughput / fresh owner、power、物流、terminal；retire_and_rebuild / 旧工作五 bucket 逐项处置 / 不继承旧 W / reservation；defer / current capability / 保留原任务与未结算状态 / 不创建维护或能力效果
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看五类旧工作；确认变更（禁用：专业合同未闭合）。
- 状态规范：maintenance_not_tracked：不推断安全；无受支持选择：动作禁用，保留当前能力读面；reconfiguration_pending / reconfiguring：不等于能力生效；unknown：原 change identity 核对，不重提；完成：重新读取前后状态与单次 disposition。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E04-maintenance-retrofit.svg)；`normal_frame`；区域 `rows 3–5`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-02-root-sc25-factorydispose"></a>
### 条款 T-ECO-02 / root-sc25-factorydispose
- 唯一上游：[root-sc25-factorydispose](../../product/world-rules-core-gameplay/prd.md#root-sc25-factorydispose)；Trace refs: O7-REQ-aa02140c-4e3d-411e-97ba-28ad19830c5b。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。。
- 字段：未消费 hold / hold / release / replan：依 profile / 数量、原 reservation；WIP / finish / pause / rework / salvage：依 profile / 已消费量、损失、parent receipt；in-transit / finish / hold / return / reroute：依 profile / 原 route、destination、损耗；buffer-held / hold / handoff / conversion：依 profile / custody、容量占用；terminal-pending / 原承诺 / hold / return：依 profile / recipient、admission、未结义务
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：展开处置差异；确认换线（禁用：缺执行合同）。
- 状态规范：maintenance_not_tracked：不推断安全；无受支持选择：动作禁用，保留当前能力读面；reconfiguration_pending / reconfiguring：不等于能力生效；unknown：原 change identity 核对，不重提；完成：重新读取前后状态与单次 disposition。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E03-cutover-terminal.svg)；`text_spec_only`；区域 `rows 1–5`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-02-root-sc25-factorystate"></a>
### 条款 T-ECO-02 / root-sc25-factorystate
- 唯一上游：[root-sc25-factorystate](../../product/world-rules-core-gameplay/prd.md#root-sc25-factorystate)；Trace refs: O7-REQ-df369b38-4338-4207-b6e0-60cfba3f12cb。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。。
- 字段：maintain_before_run / 追加资源、停机时延：待权威 / 维护范围、失败损失与撤回性；run_at_risk / reduce_load / 仍占用、预期产出：待权威 / 只有 profile 明示 at-risk 才可选；upgrade / reconfigure / 新能力 / recipe fit / throughput / fresh owner、power、物流、terminal；retire_and_rebuild / 旧工作五 bucket 逐项处置 / 不继承旧 W / reservation；defer / current capability / 保留原任务与未结算状态 / 不创建维护或能力效果
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看五类旧工作；确认变更（禁用：专业合同未闭合）。
- 状态规范：maintenance_not_tracked：不推断安全；无受支持选择：动作禁用，保留当前能力读面；reconfiguration_pending / reconfiguring：不等于能力生效；unknown：原 change identity 核对，不重提；完成：重新读取前后状态与单次 disposition。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E04-maintenance-retrofit.svg)；`normal_frame`；区域 `R1`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-03-root-sc23-buildquote"></a>
### 条款 T-ECO-03 / root-sc23-buildquote
- 唯一上游：[root-sc23-buildquote](../../product/world-rules-core-gameplay/prd.md#root-sc23-buildquote)；Trace refs: O7-REQ-4507fa71-95c4-404a-969a-c15c08dbeccc。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。。
- 字段：site / location / owner / site_available：仅示例状态 / site_unknown：缺 authority；access / chunk / active / 按权威版本核对 / unknown 不等于公开访问；construction inputs / kind / quantity / ledger / before / after / 不足须原子拒绝；owner-held electricity / exact factory_id profile / revision / 不能用全世界电量付款；recipe topology（advisory） / 未 pin recipe：路线/损耗是估计 / 此漂移不单独阻止建厂
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较成本（只读目标）；BuildFactory（禁用：未核发布能力）。
- 状态规范：site_unknown：不能猜 access / co-location；site_blocked：展示权限/准备度/缺料根因；quote expired：保留旧 quote，重新报价；advisory drift：未 pin recipe 时不据此阻断建厂；pending build：不是发电或 recipe-ready。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E05-build-sites.svg)；`normal_frame`；区域 `rows 1–4 / R1`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-03-root-sc23-buildpower"></a>
### 条款 T-ECO-03 / root-sc23-buildpower
- 唯一上游：[root-sc23-buildpower](../../product/world-rules-core-gameplay/prd.md#root-sc23-buildpower)；Trace refs: O7-REQ-8931e5de-af90-4c6a-8c2e-d21930e1a4ef。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。。
- 字段：start-only sink / 接受时 owner-held electricity 单次消费 / 不是 completion hold；held-through-completion/arrival / 按 profile 持有、消费、释放 / 未发布能力时不可选择；boundary-revalidated conditional / 声明的边界重新验证 / 当前余额不是未来保障；non-reserved / 明确 power-best-effort / 不是 hard service window；PowerPlant output boundary / construction complete 或 activation 唯一选定 / quote / pending 不发电
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看 profile 来源；切换 mode（禁用：非用户自定）。
- 状态规范：site_unknown：不能猜 access / co-location；site_blocked：展示权限/准备度/缺料根因；quote expired：保留旧 quote，重新报价；advisory drift：未 pin recipe 时不据此阻断建厂；pending build：不是发电或 recipe-ready。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E06-build-power.svg)；`normal_frame`；区域 `rows 1–5`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-03-root-sc29-sitecompare"></a>
### 条款 T-ECO-03 / root-sc29-sitecompare
- 唯一上游：[root-sc29-sitecompare](../../product/world-rules-core-gameplay/prd.md#root-sc29-sitecompare)；Trace refs: O7-REQ-982fc046-c8dc-43d1-bc0c-8b6bd3e0244b。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。。
- 字段：site / location / owner / site_available：仅示例状态 / site_unknown：缺 authority；access / chunk / active / 按权威版本核对 / unknown 不等于公开访问；construction inputs / kind / quantity / ledger / before / after / 不足须原子拒绝；owner-held electricity / exact factory_id profile / revision / 不能用全世界电量付款；recipe topology（advisory） / 未 pin recipe：路线/损耗是估计 / 此漂移不单独阻止建厂
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较成本（只读目标）；BuildFactory（禁用：未核发布能力）。
- 状态规范：site_unknown：不能猜 access / co-location；site_blocked：展示权限/准备度/缺料根因；quote expired：保留旧 quote，重新报价；advisory drift：未 pin recipe 时不据此阻断建厂；pending build：不是发电或 recipe-ready。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E05-build-sites.svg)；`normal_frame`；区域 `two site columns`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-03-root-sc29-siteadvisory"></a>
### 条款 T-ECO-03 / root-sc29-siteadvisory
- 唯一上游：[root-sc29-siteadvisory](../../product/world-rules-core-gameplay/prd.md#root-sc29-siteadvisory)；Trace refs: O7-REQ-328d5c5c-24d4-4bf6-bfd5-d1f968c9fdd2。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。。
- 字段：site / location / owner / site_available：仅示例状态 / site_unknown：缺 authority；access / chunk / active / 按权威版本核对 / unknown 不等于公开访问；construction inputs / kind / quantity / ledger / before / after / 不足须原子拒绝；owner-held electricity / exact factory_id profile / revision / 不能用全世界电量付款；recipe topology（advisory） / 未 pin recipe：路线/损耗是估计 / 此漂移不单独阻止建厂
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较成本（只读目标）；BuildFactory（禁用：未核发布能力）。
- 状态规范：site_unknown：不能猜 access / co-location；site_blocked：展示权限/准备度/缺料根因；quote expired：保留旧 quote，重新报价；advisory drift：未 pin recipe 时不据此阻断建厂；pending build：不是发电或 recipe-ready。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E05-build-sites.svg)；`normal_frame`；区域 `row 5 / R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-04-root-sc24-recipestate"></a>
### 条款 T-ECO-04 / root-sc24-recipestate
- 唯一上游：[root-sc24-recipestate](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipestate)；Trace refs: O7-REQ-e912cdd3-0222-4a88-b920-983a4118a6d1。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。。
- 字段：pending_validation / 禁止 / 历史异常保留并核 authority；validated_pending_admission / 禁止 / 验证通过不产生运行权；active / restricted / fresh 校验 / 仅允许 scope / accepted、WIP、transit 分别核验；retiring / 禁止 / buffer、terminal-pending 也需处置；retired / 禁止 / 仅历史读取；不能重新生产
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看 version 与授权；生命周期转换（禁用：无发布能力）。
- 状态规范：candidate_unknown：证据未齐，不同于无候选；recipe missing：解释能力与配方缺口；restricted：仅明确 scope 可排程；retiring：逐六 bucket 处置；retired：历史只读；successor 不自动迁移。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E08-recipe-lifecycle.svg)；`normal_frame`；区域 `rows 1–5`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-work-graph-and-recipe-lifecycle.design.md](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#recipe-lifecycle)；不是当前Viewer DTO开放证明。

<a id="t-eco-04-root-sc24-recipeversion"></a>
### 条款 T-ECO-04 / root-sc24-recipeversion
- 唯一上游：[root-sc24-recipeversion](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipeversion)；Trace refs: O7-REQ-70481f2f-9b55-4b99-afde-b4d451843d1d。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。。
- 字段：pending_validation / 禁止 / 历史异常保留并核 authority；validated_pending_admission / 禁止 / 验证通过不产生运行权；active / restricted / fresh 校验 / 仅允许 scope / accepted、WIP、transit 分别核验；retiring / 禁止 / buffer、terminal-pending 也需处置；retired / 禁止 / 仅历史读取；不能重新生产
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看 version 与授权；生命周期转换（禁用：无发布能力）。
- 状态规范：candidate_unknown：证据未齐，不同于无候选；recipe missing：解释能力与配方缺口；restricted：仅明确 scope 可排程；retiring：逐六 bucket 处置；retired：历史只读；successor 不自动迁移。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E08-recipe-lifecycle.svg)；`normal_frame`；区域 `R2/R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-04-root-sc24-recipedispose"></a>
### 条款 T-ECO-04 / root-sc24-recipedispose
- 唯一上游：[root-sc24-recipedispose](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipedispose)；Trace refs: O7-REQ-df2f1a58-f435-4e86-b6c1-810f13f8267d。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。。
- 字段：pending_validation / 禁止 / 历史异常保留并核 authority；validated_pending_admission / 禁止 / 验证通过不产生运行权；active / restricted / fresh 校验 / 仅允许 scope / accepted、WIP、transit 分别核验；retiring / 禁止 / buffer、terminal-pending 也需处置；retired / 禁止 / 仅历史读取；不能重新生产
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看 version 与授权；生命周期转换（禁用：无发布能力）。
- 状态规范：candidate_unknown：证据未齐，不同于无候选；recipe missing：解释能力与配方缺口；restricted：仅明确 scope 可排程；retiring：逐六 bucket 处置；retired：历史只读；successor 不自动迁移。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E08-recipe-lifecycle.svg)；`normal_frame`；区域 `old work column / R1`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-work-graph-and-recipe-lifecycle.design.md](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)；不是当前Viewer DTO开放证明。

<a id="t-eco-04-root-sc27-discover"></a>
### 条款 T-ECO-04 / root-sc27-discover
- 唯一上游：[root-sc27-discover](../../product/world-rules-core-gameplay/prd.md#root-sc27-discover)；Trace refs: O7-REQ-d0c8d71e-dae0-405f-8b01-c6d573eaf719。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。。
- 字段：candidate A / pinned recipe / factory capability / candidate_available（示意）；candidate B / 缺 batch quality / custody / candidate_unknown，不补零；candidate C / 路线或 terminal 不适用 / candidate_blocked，保留原因；代表性档案 / 两类原料、cycle、power、主/副产物 / 任何必需 authority 缺失 → incomplete；下一用途 / production-only / terminal-admission / 选择 ≠ 排程 ≠ 生产 ≠ 交付
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：打开完整档案；排程（禁用：仅条件目标）。
- 状态规范：candidate_unknown：证据未齐，不同于无候选；recipe missing：解释能力与配方缺口；restricted：仅明确 scope 可排程；retiring：逐六 bucket 处置；retired：历史只读；successor 不自动迁移。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E07-recipe-discovery.svg)；`normal_frame`；区域 `rows 1–3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-04-root-sc28-profile"></a>
### 条款 T-ECO-04 / root-sc28-profile
- 唯一上游：[root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。。
- 字段：candidate A / pinned recipe / factory capability / candidate_available（示意）；candidate B / 缺 batch quality / custody / candidate_unknown，不补零；candidate C / 路线或 terminal 不适用 / candidate_blocked，保留原因；代表性档案 / 两类原料、cycle、power、主/副产物 / 任何必需 authority 缺失 → incomplete；下一用途 / production-only / terminal-admission / 选择 ≠ 排程 ≠ 生产 ≠ 交付
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：打开完整档案；排程（禁用：仅条件目标）。
- 状态规范：candidate_unknown：证据未齐，不同于无候选；recipe missing：解释能力与配方缺口；restricted：仅明确 scope 可排程；retiring：逐六 bucket 处置；retired：历史只读；successor 不自动迁移。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E07-recipe-discovery.svg)；`text_spec_only`；区域 `row 4 / R2`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[industrial-resource-flow-contract.design.md](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)；不是当前Viewer DTO开放证明。

<a id="t-eco-05-root-sc30-subchoice"></a>
### 条款 T-ECO-05 / root-sc30-subchoice
- 唯一上游：[root-sc30-subchoice](../../product/world-rules-core-gameplay/prd.md#root-sc30-subchoice)；Trace refs: O7-REQ-8a79f47e-f132-4467-8d09-d0ceacaac868。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前输入/lot owner 与受权执行者；替代 decision 权限不自动扩展至 conversion/cutover。。
- 字段：same_material_source / 新 ledger/path/owner/custody 重验 / 不继承旧 hold 或顺位；profile_substitute / 准确 input edge / 比例及单位 / quantum / unknown 不默认为 1:1；new_recipe / 全部输入、输出、power、terminal 重验 / 新 candidate；原 W 不继承；decision → conversion WIP / 明确授权上限、已 sink、remaining / start receipt 不是产物；conversion settlement / 实际输出/副产物/损耗/residual / matching receipt 才成为 join 来源
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看当前替代方案（对应 R-GAME-21 读面）；确认替代（禁用：执行能力待核）。
- 状态规范：no_legal_substitute：完整集合检查后才确定；unknown/blocked：尚不能判定合法集合；expired decision：重新读候选，不沿用旧许可；conversion WIP：已 sink / remaining；未完成；converted：仅实际 conversion receipt；不增加原 W/reward。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E09-substitution.svg)；`normal_frame`；区域 `rows 1–3 / R1`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[recipe-substitution-decision-and-conversion.design.md](../../world-runtime/runtime/recipe-substitution-decision-and-conversion.design.md#substitution-candidates)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[recipe-substitution-decision-and-conversion.design.md](../../world-runtime/runtime/recipe-substitution-decision-and-conversion.design.md#substitution-model)；不是当前Viewer DTO开放证明。

<a id="t-eco-05-root-sc30-subsettle"></a>
### 条款 T-ECO-05 / root-sc30-subsettle
- 唯一上游：[root-sc30-subsettle](../../product/world-rules-core-gameplay/prd.md#root-sc30-subsettle)；Trace refs: O7-REQ-300c6cbb-49f5-4858-a749-309991d5a5d0。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前输入/lot owner 与受权执行者；替代 decision 权限不自动扩展至 conversion/cutover。。
- 字段：same_material_source / 新 ledger/path/owner/custody 重验 / 不继承旧 hold 或顺位；profile_substitute / 准确 input edge / 比例及单位 / quantum / unknown 不默认为 1:1；new_recipe / 全部输入、输出、power、terminal 重验 / 新 candidate；原 W 不继承；decision → conversion WIP / 明确授权上限、已 sink、remaining / start receipt 不是产物；conversion settlement / 实际输出/副产物/损耗/residual / matching receipt 才成为 join 来源
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看当前替代方案（对应 R-GAME-21 读面）；确认替代（禁用：执行能力待核）。
- 状态规范：no_legal_substitute：完整集合检查后才确定；unknown/blocked：尚不能判定合法集合；expired decision：重新读候选，不沿用旧许可；conversion WIP：已 sink / remaining；未完成；converted：仅实际 conversion receipt；不增加原 W/reward。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E09-substitution.svg)；`normal_frame`；区域 `rows 4–5 / R2`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[recipe-substitution-decision-and-conversion.design.md](../../world-runtime/runtime/recipe-substitution-decision-and-conversion.design.md#substitution-decision)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[recipe-substitution-decision-and-conversion.design.md](../../world-runtime/runtime/recipe-substitution-decision-and-conversion.design.md#substitution-conversion)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[recipe-substitution-decision-and-conversion.design.md](../../world-runtime/runtime/recipe-substitution-decision-and-conversion.design.md#substitution-persistence)；不是当前Viewer DTO开放证明。

<a id="t-eco-05-root-sc30-subcause"></a>
### 条款 T-ECO-05 / root-sc30-subcause
- 唯一上游：[root-sc30-subcause](../../product/world-rules-core-gameplay/prd.md#root-sc30-subcause)；Trace refs: O7-REQ-84ea0d88-f33b-4ac1-9722-ca4892df9152。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前输入/lot owner 与受权执行者；替代 decision 权限不自动扩展至 conversion/cutover。。
- 字段：same_material_source / 新 ledger/path/owner/custody 重验 / 不继承旧 hold 或顺位；profile_substitute / 准确 input edge / 比例及单位 / quantum / unknown 不默认为 1:1；new_recipe / 全部输入、输出、power、terminal 重验 / 新 candidate；原 W 不继承；decision → conversion WIP / 明确授权上限、已 sink、remaining / start receipt 不是产物；conversion settlement / 实际输出/副产物/损耗/residual / matching receipt 才成为 join 来源
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看当前替代方案（对应 R-GAME-21 读面）；确认替代（禁用：执行能力待核）。
- 状态规范：no_legal_substitute：完整集合检查后才确定；unknown/blocked：尚不能判定合法集合；expired decision：重新读候选，不沿用旧许可；conversion WIP：已 sink / remaining；未完成；converted：仅实际 conversion receipt；不增加原 W/reward。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E09-substitution.svg)；`normal_frame`；区域 `row 3 / R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[recipe-substitution-decision-and-conversion.design.md](../../world-runtime/runtime/recipe-substitution-decision-and-conversion.design.md#substitution-causality)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[recipe-substitution-decision-and-conversion.design.md](../../world-runtime/runtime/recipe-substitution-decision-and-conversion.design.md#substitution-conversion)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[recipe-substitution-decision-and-conversion.design.md](../../world-runtime/runtime/recipe-substitution-decision-and-conversion.design.md#substitution-persistence)；不是当前Viewer DTO开放证明。

<a id="t-eco-06-req-sc31-001"></a>
### 条款 T-ECO-06 / req-sc31-001
- 唯一上游：[REQ-SC31-001](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-001)；Trace refs: O7-REQ-bfd4b2f4-6b21-4c4a-961d-387b220e762f。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。。
- 字段：target / committed / 各自由目标与承诺 authority 发布 / 整批 / 减量 / 补料：依 profile；produced / 仅 production receipt 的实际量 / 保留 produced / undelivered；delivery / terminal settled / 只统计匹配目标的 receipt / 不匹配的交付不减少需求；remaining / shortage / 缺证显示 demand_unknown / 受支持时 parent-linked 补产；matched / surplus / 盈余仍有 owner 与有限存储 / 停止 / 延期；不自动倾销
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较计划（只读目标）；确认补产（禁用：合同待闭合）。
- 状态规范：demand_unknown / not_tracked：不是无限需求；accepted：有限承诺，不是 produced；produced/undelivered：目标 remaining 不减少；partial settled：匹配数量一次减少，保留 shortage；satisfied：停止旧计划；surplus 不自动出售；unknown：保留原目标和 receipt，重连不补产。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E10-demand-goal.svg)；`normal_frame`；区域 `rows 1–5 / R1–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-06-req-sc31-002"></a>
### 条款 T-ECO-06 / req-sc31-002
- 唯一上游：[REQ-SC31-002](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-002)；Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。。
- 字段：target / committed / 各自由目标与承诺 authority 发布 / 整批 / 减量 / 补料：依 profile；produced / 仅 production receipt 的实际量 / 保留 produced / undelivered；delivery / terminal settled / 只统计匹配目标的 receipt / 不匹配的交付不减少需求；remaining / shortage / 缺证显示 demand_unknown / 受支持时 parent-linked 补产；matched / surplus / 盈余仍有 owner 与有限存储 / 停止 / 延期；不自动倾销
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较计划（只读目标）；确认补产（禁用：合同待闭合）。
- 状态规范：demand_unknown / not_tracked：不是无限需求；accepted：有限承诺，不是 produced；produced/undelivered：目标 remaining 不减少；partial settled：匹配数量一次减少，保留 shortage；satisfied：停止旧计划；surplus 不自动出售；unknown：保留原目标和 receipt，重连不补产。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E10-demand-goal.svg)；`normal_frame`；区域 `rows 1–5 / R1–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[gameplay-top-level-design.prd.md](../../game/gameplay/gameplay-top-level-design.prd.md#25-前期工业引导成就闭环)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-06-req-sc31-003"></a>
### 条款 T-ECO-06 / req-sc31-003
- 唯一上游：[REQ-SC31-003](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-003)；Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。。
- 字段：target / committed / 各自由目标与承诺 authority 发布 / 整批 / 减量 / 补料：依 profile；produced / 仅 production receipt 的实际量 / 保留 produced / undelivered；delivery / terminal settled / 只统计匹配目标的 receipt / 不匹配的交付不减少需求；remaining / shortage / 缺证显示 demand_unknown / 受支持时 parent-linked 补产；matched / surplus / 盈余仍有 owner 与有限存储 / 停止 / 延期；不自动倾销
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较计划（只读目标）；确认补产（禁用：合同待闭合）。
- 状态规范：demand_unknown / not_tracked：不是无限需求；accepted：有限承诺，不是 produced；produced/undelivered：目标 remaining 不减少；partial settled：匹配数量一次减少，保留 shortage；satisfied：停止旧计划；surplus 不自动出售；unknown：保留原目标和 receipt，重连不补产。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E10-demand-goal.svg)；`normal_frame`；区域 `rows 1–5 / R1–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-06-req-sc31-004"></a>
### 条款 T-ECO-06 / req-sc31-004
- 唯一上游：[REQ-SC31-004](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-004)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。。
- 字段：target / committed / 各自由目标与承诺 authority 发布 / 整批 / 减量 / 补料：依 profile；produced / 仅 production receipt 的实际量 / 保留 produced / undelivered；delivery / terminal settled / 只统计匹配目标的 receipt / 不匹配的交付不减少需求；remaining / shortage / 缺证显示 demand_unknown / 受支持时 parent-linked 补产；matched / surplus / 盈余仍有 owner 与有限存储 / 停止 / 延期；不自动倾销
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较计划（只读目标）；确认补产（禁用：合同待闭合）。
- 状态规范：demand_unknown / not_tracked：不是无限需求；accepted：有限承诺，不是 produced；produced/undelivered：目标 remaining 不减少；partial settled：匹配数量一次减少，保留 shortage；satisfied：停止旧计划；surplus 不自动出售；unknown：保留原目标和 receipt，重连不补产。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E10-demand-goal.svg)；`normal_frame`；区域 `rows 1–5 / R1–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-06-req-sc31-005"></a>
### 条款 T-ECO-06 / req-sc31-005
- 唯一上游：[REQ-SC31-005](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-005)；Trace refs: O7-REQ-62129528-2238-4733-8b8e-d226dc40943d。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。。
- 字段：target / committed / 各自由目标与承诺 authority 发布 / 整批 / 减量 / 补料：依 profile；produced / 仅 production receipt 的实际量 / 保留 produced / undelivered；delivery / terminal settled / 只统计匹配目标的 receipt / 不匹配的交付不减少需求；remaining / shortage / 缺证显示 demand_unknown / 受支持时 parent-linked 补产；matched / surplus / 盈余仍有 owner 与有限存储 / 停止 / 延期；不自动倾销
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较计划（只读目标）；确认补产（禁用：合同待闭合）。
- 状态规范：demand_unknown / not_tracked：不是无限需求；accepted：有限承诺，不是 produced；produced/undelivered：目标 remaining 不减少；partial settled：匹配数量一次减少，保留 shortage；satisfied：停止旧计划；surplus 不自动出售；unknown：保留原目标和 receipt，重连不补产。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E10-demand-goal.svg)；`normal_frame`；区域 `rows 1–5 / R1–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-06-req-sc31-006"></a>
### 条款 T-ECO-06 / req-sc31-006
- 唯一上游：[REQ-SC31-006](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-006)；Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。。
- 字段：target / committed / 各自由目标与承诺 authority 发布 / 整批 / 减量 / 补料：依 profile；produced / 仅 production receipt 的实际量 / 保留 produced / undelivered；delivery / terminal settled / 只统计匹配目标的 receipt / 不匹配的交付不减少需求；remaining / shortage / 缺证显示 demand_unknown / 受支持时 parent-linked 补产；matched / surplus / 盈余仍有 owner 与有限存储 / 停止 / 延期；不自动倾销
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较计划（只读目标）；确认补产（禁用：合同待闭合）。
- 状态规范：demand_unknown / not_tracked：不是无限需求；accepted：有限承诺，不是 produced；produced/undelivered：目标 remaining 不减少；partial settled：匹配数量一次减少，保留 shortage；satisfied：停止旧计划；surplus 不自动出售；unknown：保留原目标和 receipt，重连不补产。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E10-demand-goal.svg)；`normal_frame`；区域 `rows 1–5 / R1–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-06-req-sc31-007"></a>
### 条款 T-ECO-06 / req-sc31-007
- 唯一上游：[REQ-SC31-007](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-007)；Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。。
- 字段：target / committed / 各自由目标与承诺 authority 发布 / 整批 / 减量 / 补料：依 profile；produced / 仅 production receipt 的实际量 / 保留 produced / undelivered；delivery / terminal settled / 只统计匹配目标的 receipt / 不匹配的交付不减少需求；remaining / shortage / 缺证显示 demand_unknown / 受支持时 parent-linked 补产；matched / surplus / 盈余仍有 owner 与有限存储 / 停止 / 延期；不自动倾销
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较计划（只读目标）；确认补产（禁用：合同待闭合）。
- 状态规范：demand_unknown / not_tracked：不是无限需求；accepted：有限承诺，不是 produced；produced/undelivered：目标 remaining 不减少；partial settled：匹配数量一次减少，保留 shortage；satisfied：停止旧计划；surplus 不自动出售；unknown：保留原目标和 receipt，重连不补产。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E10-demand-goal.svg)；`normal_frame`；区域 `rows 1–5 / R1–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../testing/prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-06-req-sc31-008"></a>
### 条款 T-ECO-06 / req-sc31-008
- 唯一上游：[REQ-SC31-008](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-008)；Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。。
- 字段：target / committed / 各自由目标与承诺 authority 发布 / 整批 / 减量 / 补料：依 profile；produced / 仅 production receipt 的实际量 / 保留 produced / undelivered；delivery / terminal settled / 只统计匹配目标的 receipt / 不匹配的交付不减少需求；remaining / shortage / 缺证显示 demand_unknown / 受支持时 parent-linked 补产；matched / surplus / 盈余仍有 owner 与有限存储 / 停止 / 延期；不自动倾销
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：比较计划（只读目标）；确认补产（禁用：合同待闭合）。
- 状态规范：demand_unknown / not_tracked：不是无限需求；accepted：有限承诺，不是 produced；produced/undelivered：目标 remaining 不减少；partial settled：匹配数量一次减少，保留 shortage；satisfied：停止旧计划；surplus 不自动出售；unknown：保留原目标和 receipt，重连不补产。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E10-demand-goal.svg)；`normal_frame`；区域 `rows 1–5 / R1–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../testing/prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-06-req-sc31-009"></a>
### 条款 T-ECO-06 / req-sc31-009
- 唯一上游：[REQ-SC31-009](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009)；Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2。
- 状态：源设计 `部分覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。。
- 字段：需求发现 / 同目标用途、数量、期限与 recipient / 接受订单或取得独占权；运输 / arrival / source、route、destination、实际损耗 / storage acceptance / 产权结算；接收验收 / 规格/数量/用途/有权 recipient / escrow 无条件释放；ownership / escrow / 匹配条件和各方 receipt / production receipt 自动付款；partial / dispute / unknown / 保留已交付、待验、未结金额 / 全额退款或整单完成
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：展开收据链；请求结算（禁用：未核发布能力）。
- 状态规范：demand_unknown / not_tracked：不是无限需求；accepted：有限承诺，不是 produced；produced/undelivered：目标 remaining 不减少；partial settled：匹配数量一次减少，保留 shortage；satisfied：停止旧计划；surplus 不自动出售；unknown：保留原目标和 receipt，重连不补产。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E11-cross-region.svg)；`normal_frame`；区域 `rows 1–5 / R1–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[cross-region-delivery-and-escrow-settlement.design.md](../../world-runtime/runtime/cross-region-delivery-and-escrow-settlement.design.md#des-wr-cr-001)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[cross-region-delivery-and-escrow-settlement.design.md](../../world-runtime/runtime/cross-region-delivery-and-escrow-settlement.design.md#des-wr-cr-002)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[gameplay-industrial-creation-and-cross-region-market-contract.prd.md](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-04)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[industrial-resource-flow-contract.prd.md](../m4/industrial-resource-flow-contract.prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../p2p/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../testing/prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-07-req-wr-es-001"></a>
### 条款 T-ECO-07 / req-wr-es-001
- 唯一上游：[REQ-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001)；Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1。
- 状态：源设计 `已覆盖`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。。
- 字段：危机证据 / normal / effective coverage 分开 / 样本、义务和来源不可混合；authorization / issuer、scope、starts/expires、policy、review / 缺失 / 过期不准新承诺；冻结分配 / request / need / snapshot / requested maxima / 不靠重复请求插队；数量追踪 / committed / transit / received / fulfilled / released / lost 单列；两维结果 / allocated / partial / denied / expired + stage / allocated 不等于已履行
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看正常市场 q06；申请 / 复核（禁用：权限未核）。
- 状态规范：normal：进入既有市场 quote；无紧急权力；authorization expired：不新分配；旧在途有限履约；partial allocation + in-transit：两维并列；denied：保留原因与合法复核入口；appeal：不自动改变原结算；stale/unverified：不把缺失 receipt 写成已履行。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E12-emergency.svg)；`normal_frame`；区域 `authority and quantity table / R1`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[emergency-supply-authorization-and-settlement.design.md](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-001)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-07-req-wr-es-002"></a>
### 条款 T-ECO-07 / req-wr-es-002
- 唯一上游：[REQ-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-002)；Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。。
- 字段：危机证据 / normal / effective coverage 分开 / 样本、义务和来源不可混合；authorization / issuer、scope、starts/expires、policy、review / 缺失 / 过期不准新承诺；冻结分配 / request / need / snapshot / requested maxima / 不靠重复请求插队；数量追踪 / committed / transit / received / fulfilled / released / lost 单列；两维结果 / allocated / partial / denied / expired + stage / allocated 不等于已履行
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看正常市场 q06；申请 / 复核（禁用：权限未核）。
- 状态规范：normal：进入既有市场 quote；无紧急权力；authorization expired：不新分配；旧在途有限履约；partial allocation + in-transit：两维并列；denied：保留原因与合法复核入口；appeal：不自动改变原结算；stale/unverified：不把缺失 receipt 写成已履行。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E12-emergency.svg)；`normal_frame`；区域 `authority and quantity table / R2`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[emergency-supply-authorization-and-settlement.design.md](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-002)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../testing/prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-07-req-wr-es-003"></a>
### 条款 T-ECO-07 / req-wr-es-003
- 唯一上游：[REQ-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-003)；Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。。
- 字段：危机证据 / normal / effective coverage 分开 / 样本、义务和来源不可混合；authorization / issuer、scope、starts/expires、policy、review / 缺失 / 过期不准新承诺；冻结分配 / request / need / snapshot / requested maxima / 不靠重复请求插队；数量追踪 / committed / transit / received / fulfilled / released / lost 单列；两维结果 / allocated / partial / denied / expired + stage / allocated 不等于已履行
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看正常市场 q06；申请 / 复核（禁用：权限未核）。
- 状态规范：normal：进入既有市场 quote；无紧急权力；authorization expired：不新分配；旧在途有限履约；partial allocation + in-transit：两维并列；denied：保留原因与合法复核入口；appeal：不自动改变原结算；stale/unverified：不把缺失 receipt 写成已履行。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E12-emergency.svg)；`normal_frame`；区域 `authority and quantity table / R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[emergency-supply-authorization-and-settlement.design.md](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-003)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../p2p/prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-07-req-wr-es-004"></a>
### 条款 T-ECO-07 / req-wr-es-004
- 唯一上游：[REQ-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004)；Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3。
- 状态：源设计 `已覆盖`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。。
- 字段：危机证据 / normal / effective coverage 分开 / 样本、义务和来源不可混合；authorization / issuer、scope、starts/expires、policy、review / 缺失 / 过期不准新承诺；冻结分配 / request / need / snapshot / requested maxima / 不靠重复请求插队；数量追踪 / committed / transit / received / fulfilled / released / lost 单列；两维结果 / allocated / partial / denied / expired + stage / allocated 不等于已履行
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看正常市场 q06；申请 / 复核（禁用：权限未核）。
- 状态规范：normal：进入既有市场 quote；无紧急权力；authorization expired：不新分配；旧在途有限履约；partial allocation + in-transit：两维并列；denied：保留原因与合法复核入口；appeal：不自动改变原结算；stale/unverified：不把缺失 receipt 写成已履行。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E12-emergency.svg)；`normal_frame`；区域 `authority and quantity table / R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[emergency-supply-authorization-and-settlement.design.md](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-004)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[gameplay-industrial-creation-and-cross-region-market-contract.prd.md](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-05)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../p2p/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../testing/prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-08-req-wr-aos-003"></a>
### 条款 T-ECO-08 / req-wr-aos-003
- 唯一上游：[REQ-WR-AOS-003](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-003)；Trace refs: O7-REQ-f247bdc7-f4ae-4893-a35e-6840fab7f465。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。。
- 字段：Agent 经济转让 / 资格、容量、资源、治理、反滥用五门槛 / 保留来源、身份、历史、未决义务；费用 / levy / 自愿服务费与有界 levy 分开 / 用途、受益、上限、到期、复核；有限机会 / 来源、scope、用途、期限与 receipt / preview / pending 无顺位；长期经济审计 / 同候选 / 窗口 source/sink + 关键变化 / 含非 levy 资源、治理因果链；缺失 / 冲突 / 过期 / blocked / hold；保留原缺口 / 补证重核不是新结算或 release verdict
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看责任与历史；转让 / 申领（禁用：合同未闭合）。
- 状态规范：制造 pending：保留订单，不重复取得 slot；维护不足：upkeep 责任独立于制造完成；转让资格改变：五门槛 fresh 重评；原绑定 unknown：不确认 owner 变更；机会已使用/失效：历史保留，不再申领；audit hold：同候选补证，不升格 release verdict。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E14-transfer-audit.svg)；`normal_frame`；区域 `row 1 / R1`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-08-req-wr-aos-004"></a>
### 条款 T-ECO-08 / req-wr-aos-004
- 唯一上游：[REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。。
- 字段：只读 quote / world/chain/genesis、buyer、owner、factory / quote id/digest、expires_at；确认 / 预留 / 材料、电力、费用、slot 与工厂容量 / 接受不是生产；开工 / 完成 / consumed / held / refundable / loss / 完成不是交付身份安装；交付 / 原地点、upfront、未来 upkeep / 身份、claim、控制事实原子安装；unknown / failure / order、commit point、blocker、recheck tick / 查原请求，不重复制造
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看原订单（只读目标）；制造确认（禁用：发布能力待核）。
- 状态规范：制造 pending：保留订单，不重复取得 slot；维护不足：upkeep 责任独立于制造完成；转让资格改变：五门槛 fresh 重评；原绑定 unknown：不确认 owner 变更；机会已使用/失效：历史保留，不再申领；audit hold：同候选补证，不升格 release verdict。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E13-agent-order.svg)；`normal_frame`；区域 `order ledger`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[agent-industrial-manufacturing.design.md](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-001)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[agent-industrial-manufacturing.design.md](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-002)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[agent-industrial-manufacturing.design.md](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-003)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[agent-industrial-manufacturing.design.md](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-004)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[agent-industrial-manufacturing.design.md](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-005)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[agent-industrial-manufacturing.design.md](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-006)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[agent-industrial-manufacturing.design.md](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-007)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[gameplay-agent-claim-economy-contract.prd.md](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#agent-manufacturing-v1)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-08-req-wr-aos-005"></a>
### 条款 T-ECO-08 / req-wr-aos-005
- 唯一上游：[REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147。
- 状态：源设计 `部分覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。。
- 字段：Agent 经济转让 / 资格、容量、资源、治理、反滥用五门槛 / 保留来源、身份、历史、未决义务；费用 / levy / 自愿服务费与有界 levy 分开 / 用途、受益、上限、到期、复核；有限机会 / 来源、scope、用途、期限与 receipt / preview / pending 无顺位；长期经济审计 / 同候选 / 窗口 source/sink + 关键变化 / 含非 levy 资源、治理因果链；缺失 / 冲突 / 过期 / blocked / hold；保留原缺口 / 补证重核不是新结算或 release verdict
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看责任与历史；转让 / 申领（禁用：合同未闭合）。
- 状态规范：制造 pending：保留订单，不重复取得 slot；维护不足：upkeep 责任独立于制造完成；转让资格改变：五门槛 fresh 重评；原绑定 unknown：不确认 owner 变更；机会已使用/失效：历史保留，不再申领；audit hold：同候选补证，不升格 release verdict。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E14-transfer-audit.svg)；`normal_frame`；区域 `row 1 / R1`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[indirect-control-agency-execution-and-continuation.design.md](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[gameplay-agent-claim-economy-contract.prd.md](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-08-req-wr-mw-003"></a>
### 条款 T-ECO-08 / req-wr-mw-003
- 唯一上游：[REQ-WR-MW-003](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-003)；Trace refs: O7-REQ-7e2cd84b-9145-4e2f-b3c2-f9c451cec6c5。
- 状态：源设计 `已覆盖`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。。
- 字段：Agent 经济转让 / 资格、容量、资源、治理、反滥用五门槛 / 保留来源、身份、历史、未决义务；费用 / levy / 自愿服务费与有界 levy 分开 / 用途、受益、上限、到期、复核；有限机会 / 来源、scope、用途、期限与 receipt / preview / pending 无顺位；长期经济审计 / 同候选 / 窗口 source/sink + 关键变化 / 含非 levy 资源、治理因果链；缺失 / 冲突 / 过期 / blocked / hold；保留原缺口 / 补证重核不是新结算或 release verdict
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看责任与历史；转让 / 申领（禁用：合同未闭合）。
- 状态规范：制造 pending：保留订单，不重复取得 slot；维护不足：upkeep 责任独立于制造完成；转让资格改变：五门槛 fresh 重评；原绑定 unknown：不确认 owner 变更；机会已使用/失效：历史保留，不再申领；audit hold：同候选补证，不升格 release verdict。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E14-transfer-audit.svg)；`normal_frame`；区域 `row 2 / R2–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业字段语义依赖：[public-levy-authorization-and-settlement.design.md](../../world-runtime/runtime/public-levy-authorization-and-settlement.design.md#levy-read-model)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[public-levy-authorization-and-settlement.design.md](../../world-runtime/runtime/public-levy-authorization-and-settlement.design.md#levy-expiry-recovery)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../p2p/prd.md)；不是当前Viewer DTO开放证明。

<a id="t-eco-08-req-wr-mw-004"></a>
### 条款 T-ECO-08 / req-wr-mw-004
- 唯一上游：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026。
- 状态：源设计 `未核实`；源实现 `未核实`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。。
- 字段：Agent 经济转让 / 资格、容量、资源、治理、反滥用五门槛 / 保留来源、身份、历史、未决义务；费用 / levy / 自愿服务费与有界 levy 分开 / 用途、受益、上限、到期、复核；有限机会 / 来源、scope、用途、期限与 receipt / preview / pending 无顺位；长期经济审计 / 同候选 / 窗口 source/sink + 关键变化 / 含非 levy 资源、治理因果链；缺失 / 冲突 / 过期 / blocked / hold；保留原缺口 / 补证重核不是新结算或 release verdict
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看责任与历史；转让 / 申领（禁用：合同未闭合）。
- 状态规范：制造 pending：保留订单，不重复取得 slot；维护不足：upkeep 责任独立于制造完成；转让资格改变：五门槛 fresh 重评；原绑定 unknown：不确认 owner 变更；机会已使用/失效：历史保留，不再申领；audit hold：同候选补证，不升格 release verdict；generic recognition authority revoked/expired: old pending rejected/released/expired; restored permission requires explicit new submission; FI-002 separate。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E14-transfer-audit.svg)；`normal_frame`；区域 `row 3 / R2–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../p2p/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../testing/prd.md)；不是当前Viewer DTO开放证明。
  - 专业字段语义依赖：[gameplay-mature-world-progression-contract.prd.md](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-recognition-opportunity)；不是当前Viewer DTO开放证明。

<a id="t-eco-08-req-wr-mw-005"></a>
### 条款 T-ECO-08 / req-wr-mw-005
- 唯一上游：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5。
- 状态：源设计 `未核实`；源实现 `未核实`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。。
- 字段：Agent 经济转让 / 资格、容量、资源、治理、反滥用五门槛 / 保留来源、身份、历史、未决义务；费用 / levy / 自愿服务费与有界 levy 分开 / 用途、受益、上限、到期、复核；有限机会 / 来源、scope、用途、期限与 receipt / preview / pending 无顺位；长期经济审计 / 同候选 / 窗口 source/sink + 关键变化 / 含非 levy 资源、治理因果链；缺失 / 冲突 / 过期 / blocked / hold；保留原缺口 / 补证重核不是新结算或 release verdict
- 单位：Exact profile/ledger unit; unknown remains unknown. No numeric assumptions.
- 动作：查看责任与历史；转让 / 申领（禁用：合同未闭合）。
- 状态规范：制造 pending：保留订单，不重复取得 slot；维护不足：upkeep 责任独立于制造完成；转让资格改变：五门槛 fresh 重评；原绑定 unknown：不确认 owner 变更；机会已使用/失效：历史保留，不再申领；audit hold：同候选补证，不升格 release verdict。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/economic-targets/frames/E14-transfer-audit.svg)；`normal_frame`；区域 `row 4 / R2–R3`。
- 写动作：`contract_blocked_pending_published_capability_and_exact_transport`；目标图不证明transport已接通。
- 未绘制/非UI：Full enumerated state permutations and interaction/runtime behavior remain text-only / unverified.；Authority validation, atomicity, conservation, fairness, persistence/replay and settlement are professional runtime/domain duties; drawing does not validate them.
  - 专业导航/待精确核对：[prd.md](../../game/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../world-runtime/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../p2p/prd.md)；不是当前Viewer DTO开放证明。
  - 专业导航/待精确核对：[prd.md](../../testing/prd.md)；不是当前Viewer DTO开放证明。


### 6.1 规划组导航

<a id="t-eco-01"></a>
- T-ECO-01：[该组首条具体义务](#t-eco-01-root-sc20-jobdispose)。
<a id="t-eco-02"></a>
- T-ECO-02：[该组首条具体义务](#t-eco-02-root-sc22-maintchoice)。
<a id="t-eco-03"></a>
- T-ECO-03：[该组首条具体义务](#t-eco-03-root-sc23-buildquote)。
<a id="t-eco-04"></a>
- T-ECO-04：[该组首条具体义务](#t-eco-04-root-sc24-recipestate)。
<a id="t-eco-05"></a>
- T-ECO-05：[该组首条具体义务](#t-eco-05-root-sc30-subchoice)。
<a id="t-eco-06"></a>
- T-ECO-06：[该组首条具体义务](#t-eco-06-req-sc31-001)。
<a id="t-eco-07"></a>
- T-ECO-07：[该组首条具体义务](#t-eco-07-req-wr-es-001)。
<a id="t-eco-08"></a>
- T-ECO-08：[该组首条具体义务](#t-eco-08-req-wr-aos-003)。

## 7. 状态、事务与持久化

loading、empty、unavailable、permission denied、pending、unknown与明确失败不得混成一个状态。模型/查询成功不等于世界生效。原结果按原主体和请求身份回读，不能因刷新/切换/重连重放在途意图。本设计不增加持久化协议；动态DTO/服务capability未提供的字段或枚举保留未知。

## 8. 安全、权限与隐私

以当前授权为准；权限丢失先停止可执行入口并清理旧主体私有上下文，不把旧图形/反馈留给新账号。凭据只以空字段/掩码示意，图稿不包含真实key、OTP、Cookie或完整私有响应。通用raw JSON不作为获得更多数据的许可。

## 9. 质量、响应式与可访问性

遵循既有brand与visual规范。窄屏/低高度、触控、键盘、焦点回返、读屏语义、IME/reduced-motion须按实际候选验证；静态SVG与文字不证明这些运行行为。错误近字段，busy和禁用原因可读，重要结果不只靠颜色。

## 10. 兼容、迁移与回滚

沿用原14张概览与兼容入口，不移除旧authority锚点。新稿先按具体source/条件接入，不将target-only按钮启用为当前。文档/资产回退只恢复对应呈现规范，不撤销任何外部世界效果；本设计不改变运行合同。

## 11. 验证设计与可追溯性

固定source与静态SVG只支持结构和视觉审读；真实交互、权限执行、跨入口连续性与世界结果仍需按下表验证。测试/手册引用是验证方法，不代表相应场景已经通过。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [root-sc20-jobdispose](../../product/world-rules-core-gameplay/prd.md#root-sc20-jobdispose)；Trace refs: O7-REQ-eacc6e2c-a75b-4c2b-b32b-f24e15832f95 | [root-sc20-jobdispose](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc20-jobdispose) | 条件目标读面：未消费 hold, WIP, in-transit, buffer-held；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc20-jobcontinue](../../product/world-rules-core-gameplay/prd.md#root-sc20-jobcontinue)；Trace refs: O7-REQ-15abc2d8-2dc7-43ed-bb20-895dc11e8452 | [root-sc20-jobcontinue](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc20-jobcontinue) | 条件目标读面：原料 A → join, 原料 B → join, join → WIP, WIP → transit → buffer；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc21-batch](../../product/world-rules-core-gameplay/prd.md#root-sc21-batch)；Trace refs: O7-REQ-93f437ea-48eb-4f07-9af2-751faac09ec7 | [root-sc21-batch](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-batch) | 条件目标读面：原料 A → join, 原料 B → join, join → WIP, WIP → transit → buffer；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc21-join](../../product/world-rules-core-gameplay/prd.md#root-sc21-join)；Trace refs: O7-REQ-6e7590d6-923d-407a-93df-eef373cccaea | [root-sc21-join](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-join) | 条件目标读面：原料 A → join, 原料 B → join, join → WIP, WIP → transit → buffer；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc21-capacity](../../product/world-rules-core-gameplay/prd.md#root-sc21-capacity)；Trace refs: O7-REQ-7f40ee7a-192f-4579-a69d-9946a6e4d5bf | [root-sc21-capacity](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-capacity) | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc21-service](../../product/world-rules-core-gameplay/prd.md#root-sc21-service)；Trace refs: O7-REQ-a3562631-35fa-4581-86e3-589e530ad253 | [root-sc21-service](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-service) | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc21-diagnostic](../../product/world-rules-core-gameplay/prd.md#root-sc21-diagnostic)；Trace refs: O7-REQ-3373ec45-3ce8-4539-8bdd-f85acca2c252 | [root-sc21-diagnostic](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-diagnostic) | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc21-quantum](../../product/world-rules-core-gameplay/prd.md#root-sc21-quantum)；Trace refs: O7-REQ-baa1e6b2-1a46-47a3-b75c-a0e9cf8b154c | [root-sc21-quantum](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-quantum) | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc21-fanout](../../product/world-rules-core-gameplay/prd.md#root-sc21-fanout)；Trace refs: O7-REQ-885f0bc7-d049-49f2-bff4-a372ee06489c | [root-sc21-fanout](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-fanout) | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc21-backpressure](../../product/world-rules-core-gameplay/prd.md#root-sc21-backpressure)；Trace refs: O7-REQ-65e2849e-3b51-4440-a5dd-78cdd7c0692a | [root-sc21-backpressure](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-backpressure) | 条件目标读面：stage slot, edge throughput, buffer, terminal；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc21-cutover](../../product/world-rules-core-gameplay/prd.md#root-sc21-cutover)；Trace refs: O7-REQ-38521519-d028-4768-abc5-723e239192c4 | [root-sc21-cutover](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-cutover) | 条件目标读面：未消费 hold, WIP, in-transit, buffer-held；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc21-terminal](../../product/world-rules-core-gameplay/prd.md#root-sc21-terminal)；Trace refs: O7-REQ-012b7904-2468-48c6-996d-14698aa7f798 | [root-sc21-terminal](viewer-economic-targets-uiux.design.md#t-eco-01-root-sc21-terminal) | 条件目标读面：未消费 hold, WIP, in-transit, buffer-held；材料 owner、当前工厂/配方执行作用域主体；Runtime 裁定接受/receipt，M4 裁定单位/容量/适用性。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc22-maintchoice](../../product/world-rules-core-gameplay/prd.md#root-sc22-maintchoice)；Trace refs: O7-REQ-cfbe031b-99ce-419b-a3b4-bc63e066e085 | [root-sc22-maintchoice](viewer-economic-targets-uiux.design.md#t-eco-02-root-sc22-maintchoice) | 条件目标读面：maintain_before_run, run_at_risk / reduce_load, upgrade / reconfigure, retire_and_rebuild；当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc22-maintunknown](../../product/world-rules-core-gameplay/prd.md#root-sc22-maintunknown)；Trace refs: O7-REQ-2684e207-9424-41df-93a5-80bd8f541251 | [root-sc22-maintunknown](viewer-economic-targets-uiux.design.md#t-eco-02-root-sc22-maintunknown) | 条件目标读面：maintain_before_run, run_at_risk / reduce_load, upgrade / reconfigure, retire_and_rebuild；当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc25-factorychoice](../../product/world-rules-core-gameplay/prd.md#root-sc25-factorychoice)；Trace refs: O7-REQ-8a215d9f-cefc-4d06-82f3-935c44454558 | [root-sc25-factorychoice](viewer-economic-targets-uiux.design.md#t-eco-02-root-sc25-factorychoice) | 条件目标读面：maintain_before_run, run_at_risk / reduce_load, upgrade / reconfigure, retire_and_rebuild；当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc25-factorydispose](../../product/world-rules-core-gameplay/prd.md#root-sc25-factorydispose)；Trace refs: O7-REQ-aa02140c-4e3d-411e-97ba-28ad19830c5b | [root-sc25-factorydispose](viewer-economic-targets-uiux.design.md#t-eco-02-root-sc25-factorydispose) | 条件目标读面：未消费 hold, WIP, in-transit, buffer-held；当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc25-factorystate](../../product/world-rules-core-gameplay/prd.md#root-sc25-factorystate)；Trace refs: O7-REQ-df369b38-4338-4207-b6e0-60cfba3f12cb | [root-sc25-factorystate](viewer-economic-targets-uiux.design.md#t-eco-02-root-sc25-factorystate) | 条件目标读面：maintain_before_run, run_at_risk / reduce_load, upgrade / reconfigure, retire_and_rebuild；当前有权工厂控制主体；维修、能力、治理 authority 各自核验，不能由观察对象选择推导权力。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc23-buildquote](../../product/world-rules-core-gameplay/prd.md#root-sc23-buildquote)；Trace refs: O7-REQ-4507fa71-95c4-404a-969a-c15c08dbeccc | [root-sc23-buildquote](viewer-economic-targets-uiux.design.md#t-eco-03-root-sc23-buildquote) | 条件目标读面：site / location / owner, access / chunk / active, construction inputs, owner-held electricity；builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc23-buildpower](../../product/world-rules-core-gameplay/prd.md#root-sc23-buildpower)；Trace refs: O7-REQ-8931e5de-af90-4c6a-8c2e-d21930e1a4ef | [root-sc23-buildpower](viewer-economic-targets-uiux.design.md#t-eco-03-root-sc23-buildpower) | 条件目标读面：start-only sink, held-through-completion/arrival, boundary-revalidated conditional, non-reserved；builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc29-sitecompare](../../product/world-rules-core-gameplay/prd.md#root-sc29-sitecompare)；Trace refs: O7-REQ-982fc046-c8dc-43d1-bc0c-8b6bd3e0244b | [root-sc29-sitecompare](viewer-economic-targets-uiux.design.md#t-eco-03-root-sc29-sitecompare) | 条件目标读面：site / location / owner, access / chunk / active, construction inputs, owner-held electricity；builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc29-siteadvisory](../../product/world-rules-core-gameplay/prd.md#root-sc29-siteadvisory)；Trace refs: O7-REQ-328d5c5c-24d4-4bf6-bfd5-d1f968c9fdd2 | [root-sc29-siteadvisory](viewer-economic-targets-uiux.design.md#t-eco-03-root-sc29-siteadvisory) | 条件目标读面：site / location / owner, access / chunk / active, construction inputs, owner-held electricity；builder + site owner 或显式 allowlist；builder 与 site 的 canonical location_id 精确相同；M4 exact factory profile。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc24-recipestate](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipestate)；Trace refs: O7-REQ-e912cdd3-0222-4a88-b920-983a4118a6d1 | [root-sc24-recipestate](viewer-economic-targets-uiux.design.md#t-eco-04-root-sc24-recipestate) | 条件目标读面：pending_validation, validated_pending_admission, active / restricted, retiring；观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc24-recipeversion](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipeversion)；Trace refs: O7-REQ-70481f2f-9b55-4b99-afde-b4d451843d1d | [root-sc24-recipeversion](viewer-economic-targets-uiux.design.md#t-eco-04-root-sc24-recipeversion) | 条件目标读面：pending_validation, validated_pending_admission, active / restricted, retiring；观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc24-recipedispose](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipedispose)；Trace refs: O7-REQ-df2f1a58-f435-4e86-b6c1-810f13f8267d | [root-sc24-recipedispose](viewer-economic-targets-uiux.design.md#t-eco-04-root-sc24-recipedispose) | 条件目标读面：pending_validation, validated_pending_admission, active / restricted, retiring；观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc27-discover](../../product/world-rules-core-gameplay/prd.md#root-sc27-discover)；Trace refs: O7-REQ-d0c8d71e-dae0-405f-8b01-c6d573eaf719 | [root-sc27-discover](viewer-economic-targets-uiux.design.md#t-eco-04-root-sc27-discover) | 条件目标读面：candidate A, candidate B, candidate C, 代表性档案；观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117 | [root-sc28-profile](viewer-economic-targets-uiux.design.md#t-eco-04-root-sc28-profile) | 条件目标读面：candidate A, candidate B, candidate C, 代表性档案；观察者只读；执行者必须有当前 recipe/factory scope；配方治理与工业处置是两个 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc30-subchoice](../../product/world-rules-core-gameplay/prd.md#root-sc30-subchoice)；Trace refs: O7-REQ-8a79f47e-f132-4467-8d09-d0ceacaac868 | [root-sc30-subchoice](viewer-economic-targets-uiux.design.md#t-eco-05-root-sc30-subchoice) | 条件目标读面：same_material_source, profile_substitute, new_recipe, decision → conversion WIP；当前输入/lot owner 与受权执行者；替代 decision 权限不自动扩展至 conversion/cutover。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc30-subsettle](../../product/world-rules-core-gameplay/prd.md#root-sc30-subsettle)；Trace refs: O7-REQ-300c6cbb-49f5-4858-a749-309991d5a5d0 | [root-sc30-subsettle](viewer-economic-targets-uiux.design.md#t-eco-05-root-sc30-subsettle) | 条件目标读面：same_material_source, profile_substitute, new_recipe, decision → conversion WIP；当前输入/lot owner 与受权执行者；替代 decision 权限不自动扩展至 conversion/cutover。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [root-sc30-subcause](../../product/world-rules-core-gameplay/prd.md#root-sc30-subcause)；Trace refs: O7-REQ-84ea0d88-f33b-4ac1-9722-ca4892df9152 | [root-sc30-subcause](viewer-economic-targets-uiux.design.md#t-eco-05-root-sc30-subcause) | 条件目标读面：same_material_source, profile_substitute, new_recipe, decision → conversion WIP；当前输入/lot owner 与受权执行者；替代 decision 权限不自动扩展至 conversion/cutover。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-SC31-001](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-001)；Trace refs: O7-REQ-bfd4b2f4-6b21-4c4a-961d-387b220e762f | [req-sc31-001](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-001) | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-SC31-002](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-002)；Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060 | [req-sc31-002](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-002) | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-SC31-003](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-003)；Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0 | [req-sc31-003](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-003) | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-SC31-004](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-004)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | [req-sc31-004](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-004) | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-SC31-005](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-005)；Trace refs: O7-REQ-62129528-2238-4733-8b8e-d226dc40943d | [req-sc31-005](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-005) | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-SC31-006](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-006)；Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf | [req-sc31-006](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-006) | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-SC31-007](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-007)；Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd | [req-sc31-007](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-007) | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-SC31-008](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-008)；Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3 | [req-sc31-008](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-008) | 条件目标读面：target / committed, produced, delivery / terminal settled, remaining / shortage；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-SC31-009](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009)；Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | [req-sc31-009](viewer-economic-targets-uiux.design.md#t-eco-06-req-sc31-009) | 条件目标读面：需求发现, 运输 / arrival, 接收验收, ownership / escrow；目标 authority 定义 purpose/recipient/数量/期限；执行者只能在当前 delegation 和资源作用域内确认。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001)；Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | [req-wr-es-001](viewer-economic-targets-uiux.design.md#t-eco-07-req-wr-es-001) | 条件目标读面：危机证据, authorization, 冻结分配, 数量追踪；beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-002)；Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7 | [req-wr-es-002](viewer-economic-targets-uiux.design.md#t-eco-07-req-wr-es-002) | 条件目标读面：危机证据, authorization, 冻结分配, 数量追踪；beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-003)；Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1 | [req-wr-es-003](viewer-economic-targets-uiux.design.md#t-eco-07-req-wr-es-003) | 条件目标读面：危机证据, authorization, 冻结分配, 数量追踪；beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004)；Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3 | [req-wr-es-004](viewer-economic-targets-uiux.design.md#t-eco-07-req-wr-es-004) | 条件目标读面：危机证据, authorization, 冻结分配, 数量追踪；beneficiary/requester、supplier、issuer、reviewer 分工；UI 不任命 issuer 或授予预算。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-AOS-003](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-003)；Trace refs: O7-REQ-f247bdc7-f4ae-4893-a35e-6840fab7f465 | [req-wr-aos-003](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-aos-003) | 条件目标读面：Agent 经济转让, 费用 / levy, 有限机会, 长期经济审计；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [req-wr-aos-004](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-aos-004) | 条件目标读面：只读 quote, 确认 / 预留, 开工 / 完成, 交付；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | [req-wr-aos-005](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-aos-005) | 条件目标读面：Agent 经济转让, 费用 / levy, 有限机会, 长期经济审计；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-MW-003](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-003)；Trace refs: O7-REQ-7e2cd84b-9145-4e2f-b3c2-f9c451cec6c5 | [req-wr-mw-003](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-mw-003) | 条件目标读面：Agent 经济转让, 费用 / levy, 有限机会, 长期经济审计；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026 | [req-wr-mw-004](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-mw-004) | 条件目标读面：Agent 经济转让, 费用 / levy, 有限机会, 长期经济审计；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5 | [req-wr-mw-005](viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-mw-005) | 条件目标读面：Agent 经济转让, 费用 / levy, 有限机会, 长期经济审计；authenticated buyer/claim owner、受权 builder；转让双方及当前五门槛；认可审核与经济审计各自 authority。 | [原请求恢复场景](../../../testing-manual.md#request-recovery-regression)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |

## 12. 决策、长期风险与未决问题

- 图稿仅代表实际映射区域/状态；没有对应frame的项保持text_spec_only，未公布提交合同保持contract_blocked。图稿完备性不得用页面数判定。
- 本主题的具体frame及字段/动作/状态关系见随稿覆盖目录；未画状态明确保留text_spec_only，不能从概览推导。
- 动态DTO、未知action集合、部署能力、原生桌面及真实世界结果链仍待各专业owner在获准环境验证。
- 需求设计关系只覆盖表中适用范围，不自动将产品状态升为已覆盖；未核实的专业协议或目标CRUD保持明确缺口。
