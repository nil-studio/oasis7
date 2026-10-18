# p2p 模块设计总览

## 1. 问题、目标与非目标

同一 world 的共识顺序、可验证材料与执行/恢复共同形成权威底座；解决 receipt、transport 或历史恢复被误当最终性/可服务的问题。

Owner role：runtime_engineer；canonical repository：eng-cc/oasis7；固定审读基线 f9d5a552d9af04c1b1398262808198a58e560230，2026-09-26。本设计消费下表有限关系；旧章节保留作兼容详细条款，新增DES细化原义务而不改算法/schema/默认值/产品承诺。任务实际source/integration/tested tree/config/environment/window/结果由GitHub Issue/Project evidence维护，本文不伪造候选/通过结果。

2026-10-12 以 e01e29064517f4bf375767a19a09802335828249 补充的
`original-candidate-recovery-v1`、`formal-controlled-authority-v1` 与
`controlled-epoch-takeover-v1` 是后继目标协议，明确新增目标类型和状态机。
它们尚未实现或验收，不改变下文已交付 prerequisite 的 schema、拒绝边界、默认入口或部署状态。

非目标：不以文档active/合并、local fixture、历史MIG或CLI可编译宣称运行、BFT、部署、SLA或release。剩余215-object required set及root SC1..10/full组合不变。

## 2. 上游约束与相关角色

runtime/P2P拥有机制；ops拥有同窗口部署事实；QA拥有组合证据判定；producer拥有产品含义；消费者和WASM拥有各自schema/manifest/行为。professional_acceptance与product_requirement是关系类型，本文不新增机器trace schema或activation。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [身份与 profile 边界](prd.md#p2p-authority-profiles) | 新建隔离 DevLocal 世界时，真实初始化材料、节点服务 signer 与消费者固定身份必须一致；不继承旧世界历史 | [新预览世界初始化](#des-p2p-fresh-preview-bootstrap) | Runtime/P2P 初始化与 CAS；ops 独立固定输入和 consumer pin；QA 持久化与竞争验证 | 不产生正式 activation/profile/finality、资产发行许可或邀请资格 |
| [professional_acceptance: p2p-authority-profiles](prd.md#p2p-authority-profiles) | 固定世界身份与合法 profile 激活，工程前置材料不自授正式权威 | [des-p2p-authority-profiles](#des-p2p-authority-profiles) | P2P/runtime、ops、消费者与 QA 同候选验证 | target；现有 prototype/单调 guard 不证明正式提交或 readiness |
| [单权威提交](prd.md#p2p-single-authority-durable-commit) | 固定身份/epoch 的本机原子追加、去重与恢复查询 | [本机日志前置](#des-p2p-local-authority-journal-prerequisite) | DistFS 本机 journal 与 Runtime 接收 | 仅工程前置；不证明跨故障域持久确认 |
| [单权威提交](prd.md#p2p-single-authority-durable-commit) | 双端持久确认、未知结果保留原请求及单调 head | [双端协议前置](#des-p2p-replicated-durability-prerequisite) | P2P/DistFS 双端与 Runtime 接收 | 本地协议验证不证明真实独立故障域、正式提交或 readiness |
| [交接](prd.md#p2p-authority-handoff) | H 提交后旧 profile 永久停写；H/H+1 不可撤回且不能双权威 | [des-p2p-authority-profiles](#des-p2p-authority-profiles) | P2P/runtime、治理与 QA 同候选验证 | target；不以 prototype 替代正式 BFT 交接 |
| [单权威提交](prd.md#p2p-single-authority-durable-commit) | 完整 candidate 尚未落盘时保留原决定，核对持久事实后按原身份确定性重建 | [原 candidate 恢复 v1](#des-p2p-original-candidate-recovery-v1) | Runtime 执行闭包；DistFS 持久状态；QA 故障 oracle | 新协议待实现；guarded 路径仍拒绝该窗口自动 replay |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[AC-DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | 缺失/冲突恢复材料不能生成替代 candidate 或更短历史 | [原 candidate 恢复 v1](#des-p2p-original-candidate-recovery-v1) | Runtime/P2P 原闭包与 head 验证 | 目标；不承诺缺失 native 工件可恢复 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[AC-DWE-002](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-002) | 未决定 pending 当前重审；固定原 continuation 不重开 admission | [原 candidate 恢复 v1](#des-p2p-original-candidate-recovery-v1) | Runtime 两阶段接收与 QA 对照 | 不替代完整 lineage 合同 |
| [合法 profiles](prd.md#p2p-authority-profiles) | 正式初次激活、同历史 proof 与原 CommitRef 绑定 | [正式受控权威 v1](#des-p2p-formal-controlled-authority-v1) | P2P 证明与 Runtime typed activation | 目标；工程证据或配置不能自授正式权威 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[AC-DCS-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) | 普通五路 API 共同验证正式历史与 service grade | [正式受控权威 v1](#des-p2p-formal-controlled-authority-v1) | WorldService/client；tier 仅引用证明 | 新 wire/consumer 未实施，不证明完整玩家旅程 |
| [单权威提交](prd.md#p2p-single-authority-durable-commit) | 硬隔离旧签写权限，恢复 signer 仅限原未决请求 | [人工接管 v1](#des-p2p-controlled-epoch-takeover-v1) | Ops 真实隔离；P2P 存储和 signer | 不提供自动接管或部署授权 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[AC-DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | 单域丢失及半完成 epoch 保留 tail、原请求与单调 epoch | [人工接管 v1](#des-p2p-controlled-epoch-takeover-v1) | P2P/ops 恢复材料与 QA 故障矩阵 | 目标；尚无实际故障域证据 |
| [交接](prd.md#p2p-authority-handoff) | H 已提交后恢复 issuer 也不得恢复 controlled 写入 | [人工接管 v1](#des-p2p-controlled-epoch-takeover-v1) | P2P 历史 handoff verifier | 不替代 BFT 算法与新 profile 恢复 |
| [professional_acceptance: p2p-ordered-execution](prd.md#p2p-ordered-execution) | 下一committed height、有序序列/root/decode及journal绑定；错误不推进 | [des-p2p-ordered-commit](#des-p2p-ordered-commit) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-receipt-finality-boundary](prd.md#p2p-receipt-finality-boundary) | receipt≠QC；同parent/manifest/actions独立重执行，缺artifact/fault/root拒绝vote/commit | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-target-bft](prd.md#p2p-target-bft) | Propose/Prevote/Precommit仅verified >2/3 active stake cert生效；world/height/round/phase/roots/set和dedup签名绑定 | [des-p2p-target-bft](#des-p2p-target-bft) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-target-recovery-trust-chain](prd.md#p2p-target-recovery-trust-chain) | manifest/genesis→cert/header+set transition→hash snapshot→canonical replay→root→serve/vote；每环缺失/冲突/回退/异world停止 | [des-p2p-target-recovery](#des-p2p-target-recovery) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-target-role-isolation](prd.md#p2p-target-role-isolation) | 治理validator与permissionless服务、light/archive角色隔离；验证材料而非operator身份；prune重建+archive条件 | [des-p2p-layer-authority](#des-p2p-layer-authority) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-service-gate](prd.md#p2p-service-gate) | verified-serving→stale/catching-up→readonly→serving；proof冲突→isolated；非serving新intent receipt0、历史不改 | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-replication-recovery](prd.md#p2p-replication-recovery) | writer/epoch从seq1及guard不污染；权威恢复错误与qualified storage-challenge fallback区分 | [des-p2p-replication-scope](#des-p2p-replication-scope) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[product_requirement: req-dcs-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001) | 验证certificate及hash-bound材料，拒绝服务/cache/peer自授authority；runtime state apply仍外部 | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[product_requirement: ac-dcs-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) | 验证certificate及hash-bound材料，拒绝服务/cache/peer自授authority；runtime state apply仍外部 | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: req-dcs-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) | sameworld恢复链每环验证；历史只读与重新服务闸门分开 | [des-p2p-target-recovery](#des-p2p-target-recovery) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: ac-dcs-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | sameworld恢复链每环验证；历史只读与重新服务闸门分开 | [des-p2p-target-recovery](#des-p2p-target-recovery) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[product_requirement: req-dcs-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) | governed validator与permissionless服务隔离；公网IP非通用前提 | [des-p2p-layer-authority](#des-p2p-layer-authority) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[product_requirement: ac-dcs-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-003) | governed validator与permissionless服务隔离；公网IP非通用前提 | [des-p2p-layer-authority](#des-p2p-layer-authority) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: req-dcs-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004) | verified >2/3 active stake证书才推进；wrongset/equivocation/round缺证拒绝 | [des-p2p-target-bft](#des-p2p-target-bft) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: ac-dcs-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-004) | verified >2/3 active stake证书才推进；wrongset/equivocation/round缺证拒绝 | [des-p2p-target-bft](#des-p2p-target-bft) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[product_requirement: req-dcs-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) | 服务转换及manifest/head/finality conjunction；非serving intent receipt0 | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[product_requirement: ac-dcs-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) | 服务转换及manifest/head/finality conjunction；非serving intent receipt0 | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[product_requirement: req-dwe-001](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | 同parent/version/order重执行及root绑定；runtime原子apply外部 | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | runtime/WASM/Agent/Viewer专业owner与QA；不定义其schema | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[product_requirement: ac-dwe-001](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-001) | 同parent/version/order重执行及root绑定；runtime原子apply外部 | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | runtime/WASM/Agent/Viewer专业owner与QA；不定义其schema | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[product_requirement: req-dwe-002](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002) | finality不可用保持无效果pending；恢复按当前条件重新裁决，consumer状态外部 | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | runtime/WASM/Agent/Viewer专业owner与QA；不定义其schema | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[product_requirement: ac-dwe-002](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-002) | finality不可用保持无效果pending；恢复按当前条件重新裁决，consumer状态外部 | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | runtime/WASM/Agent/Viewer专业owner与QA；不定义其schema | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[product_requirement: req-dwe-003](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-003) | receipt无第二效果为交接条件；互斥lineage首有效receipt唯一胜者/终止其余由runtime拥有，拒绝过期只终止自身，独立intent并发 | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | runtime/WASM/Agent/Viewer专业owner与QA；不定义其schema | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[product_requirement: ac-dwe-003](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003) | receipt无第二效果为交接条件；互斥lineage首有效receipt唯一胜者/终止其余由runtime拥有，拒绝过期只终止自身，独立intent并发 | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | runtime/WASM/Agent/Viewer专业owner与QA；不定义其schema | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[product_requirement: req-dwe-004](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) | 首次committed/finality-verified canonical block与activation选version；compat declaration不选规则，历史receipt原manifest replay，缺证failclosed；runtime/WASM详细设计外部 | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | runtime/WASM/Agent/Viewer专业owner与QA；不定义其schema | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[product_requirement: ac-dwe-004](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-004) | 首次committed/finality-verified canonical block与activation选version；compat declaration不选规则，历史receipt原manifest replay，缺证failclosed；runtime/WASM详细设计外部 | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | runtime/WASM/Agent/Viewer专业owner与QA；不定义其schema | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[根 SC-1](../product/world-infrastructure/prd.md#sc-infra-001) | 合法 profile/epoch 与持久提交、BFT/交接组合，非单叶证明 | [接收条款](#des-p2p-authority-profiles) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[根 SC-2](../product/world-infrastructure/prd.md#sc-infra-002) | 角色权限、复制/证明提供及 pruning 可重建/archive 条件分别成立 | [接收条款](#des-p2p-layer-authority) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[根 SC-2](../product/world-infrastructure/prd.md#sc-infra-002) | 角色复制子范围：writer/epoch 从 seq1 的顺序恢复、guard 不污染，权威恢复错误不被 qualified storage-challenge fallback 掩盖；不代签全部角色存储 | [接收条款](#des-p2p-replication-scope) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[根 SC-3](../product/world-infrastructure/prd.md#sc-infra-003) | bootstrap/snapshot/replay/state-sync/pruning/灾备信任链各环及失败隔离 | [接收条款](#des-p2p-target-recovery) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607, O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[根 SC-4](../product/world-infrastructure/prd.md#sc-infra-004) | 两 profile、全部活动验证者、升级/混合版本与原历史重放 | [接收条款](#des-p2p-receipt-finality) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405, O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[根 SC-5](../product/world-infrastructure/prd.md#sc-infra-005) | pending/未知结果区分、有效胜者原子处置、拒绝/过期自身与独立并发 | [接收条款](#des-p2p-target-service-gate) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[根 SC-6](../product/world-infrastructure/prd.md#sc-infra-006) | 原世界恢复、缺失/冲突/异世界隔离、历史与未 final 请求连续性 | [接收条款](#des-p2p-target-recovery) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4, O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[根 SC-7](../product/world-infrastructure/prd.md#sc-infra-007) | 同候选同窗口服务三态、回退、receipt 0/1 与独立正向续跑 | [接收条款](#des-p2p-target-service-gate) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-a15a1b8e-8b7f-4c6a-9cb5-2f949730496c；[入口 SC-9 独立义务](../product/player-entry-distribution/prd.md#sc-entry-009) | 会话中断不重开原 admission；未知先核对，固定原决定按原身份续接；不把缺响应当无效果 | [原决定恢复](#des-p2p-original-candidate-recovery-v1) | 入口/session/读取权限/Agent owner 与 QA；本专业只接收原请求恢复子范围 | 不定义登录、授权字段或 UI；全入口状态/授权组合尚未闭合 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[根 SC-2](../product/world-infrastructure/prd.md#sc-infra-002) | pruning 前的重建、hash/root 与冗余 archive 条件；只接收角色存储组合中的恢复前置 | [已有接收](#des-p2p-target-recovery) | 原 SC 的 P2P/runtime、运维、消费者及 QA 分工 | 本条只承接所列子义务；不替代根全部组合或未批准字段 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前专业合同（冻结source） | 目标 / 差距 | 实现 / 执行证据 |
| --- | --- | --- | --- |
| 本专题现行规则 | 配对PRD与保留旧章节的有界接口/失败合同 | 目标锁定/解锁、anti-double-sign、timeout/new-round、set transition与partition/heal/restart需独立对抗性证明；本文给拒绝条件而不发明新lock算法/schema。pending/lineage/manifest/consumer字段及DC5 structured attachment仍外部未闭合。 | 本次source阅读，未运行behavior tests；具体测试定义存在不等于覆盖或通过 |
| BFT/QC/治理恢复 | root记录stake-threshold prototype；无持久QC/round/view-change/replication QC复验 | 目标证书/round/transition/recovery有明确拒绝条件，实现仍缺 | 运行/部署/发行未验证；不能由文档推导 |

## 4. 边界与结构

代码归属按消费者需要分层：`oasis7_cas` 拥有本地内容寻址存储、编码、pin、文件索引及其存储探针；`oasis7_formal_protocol` 拥有可离线验证的 formal 协议类型、签名、证明和信任锚算法，不依赖本地 CAS、压缩或端点服务。`oasis7_distfs` 保留持久化协调、复制、端点、传输与恢复接线，并重导出同一份类型保持消费者兼容。客户端证明消费者直接依赖纯协议库，纯 CAS 消费者直接依赖 CAS 库。拆分不改变编码、hash domain、落盘路径、回退顺序或权威接纳规则；native 恢复证据仍由实际恢复校验产生，不能用公开的未校验构造器替代。

net 拥有 transport/peer/DHT；consensus 拥有 proposer/attestation/action-root；存储域由 cas/distfs 分担本地 CAS/challenge 与分布式 replication；node/chain-runtime拥有commit后执行/恢复接线；proto仅共享wire；wasm_abi是manifest/runtime ABI单一来源。公开服务只能供应hash/proof材料，不能投票或写canonical。

### des-p2p-ordered-commit

下一committed height、有序序列/root/decode及journal绑定；错误不推进。

net 拥有 transport/peer/DHT；consensus 拥有 proposer/attestation/action-root；存储域由 cas/distfs 分担本地 CAS/challenge 与分布式 replication；node/chain-runtime拥有commit后执行/恢复接线；proto仅共享wire；wasm_abi是manifest/runtime ABI单一来源。公开服务只能供应hash/proof材料，不能投票或写canonical。

### des-p2p-receipt-finality

receipt≠QC；同parent/manifest/actions独立重执行，缺artifact/fault/root拒绝vote/commit。

hash-bound material仍须与verified历史/roots相符；服务operator身份、RPC可达、缓存/local receipt不能代签。active validators在投票前从同committed parent、runtime/manifest、有序actions独立重执行，artifact/fault/root错误拒vote/commit。版本由首次committed/finalityverified canonical execution block与activation边界决定；client compatibility只是声明；历史按原manifest重放，跨version replacement服从外部lineage单次效果。

### des-p2p-target-bft

Propose/Prevote/Precommit仅verified >2/3 active stake cert生效；world/height/round/phase/roots/set和dedup签名绑定。

目标证书验证先确定同world/height/round/phase的活动治理set及stake，再逐签名验真并按validator去重，核对block/action/execution roots和严格大于三分之二stake；缺失、冲突、double-sign或错误set/round/phase/root拒绝且无权威进展。slot仅pacing/proposer，timeout进入独立round；lock/unlock、anti-double-sign及transition/restart必须保留同一安全约束，不能用本地threshold或重启跳过。具体lock/timeout证据及算法接口未由现prototype实现，本文不发明新算法。

### des-p2p-target-recovery

固定 world/chain/genesis 身份→合法历史 profile/epoch/manifest 激活链→已确认 checkpoint/header+authority transition→hash snapshot→canonical committed replay→root→serve/vote；BFT 高度另验证 certificate/set transition，每环缺失/冲突/回退/异world停止。

按信任链逐环确认，snapshot blob缺失/hash错或replay/root不一致停止，不能用任意peer/latest backup/otherworld/覆盖替代。pruning必须先证明checkpoint+replay重建、hash/root验证和冗余archive可用。既有历史/offline local recovery事实不抹除；当前governed live部署禁止raw-copy路径，signed-V2只在原runbook限定observer/drill scope，observer高head不等于execution-required full restore。

运维接受入口是 [node-triad inventory/采样合同](node/node-triad-operations-observability.prd.md#inventory-与采样合同) 与 [权威边界](node/node-triad-operations-observability.prd.md#权威边界)；governed live 约束见 [bootstrap truth model](blockchain/public-testnet-governed-bootstrap.runbook.md#3-truth-model)、[hard rules](blockchain/public-testnet-governed-bootstrap.runbook.md#4-hard-rules) 和 [failure/rollback](blockchain/public-testnet-governed-bootstrap.runbook.md#14-phase-h---failure-handling-and-rollback)。这些具体环境的恢复 admission 不覆盖历史/offline local 恢复。

### des-p2p-layer-authority

治理validator与permissionless服务、light/archive角色隔离；验证材料而非operator身份；prune重建+archive条件。

net 拥有 transport/peer/DHT；consensus 拥有 proposer/attestation/action-root；存储域由 cas/distfs 分担本地 CAS/challenge 与分布式 replication；node/chain-runtime拥有commit后执行/恢复接线；proto仅共享wire；wasm_abi是manifest/runtime ABI单一来源。公开服务只能供应hash/proof材料，不能投票或写canonical。

### des-p2p-target-service-gate

verified-serving→stale/catching-up→readonly→serving；proof冲突→isolated；非serving新intent receipt0、历史不改。

只读恢复提供 verified 历史与 truthful pending/unknown；服务闸门必须同时满足 identity/checkpoint/head/hash 材料、当前 append/finality、governing manifest compatibility 和单调 head。
不满足时新 intent 不产生 committed receipt；满足后，尚无固定原决定且已知无效果的 pending 按当前 permission/resources/version/conditions 重审，无 outage 期限/priority 继承。
已经固定的原决定按 [原 candidate 恢复](#des-p2p-original-candidate-recovery-v1) 先核对、续办，unknown 不推断为无效果；accepted 新 intent 至多一个原结果且无第二效果。
退回 blocked/isolated 不撤销或改写已 confirmed receipt；consumer 可读 semantic/blocker/next-step 与 linked attachment 由外部 owner 供给。

组合执行 lane 见 [Game World State Sync and Commit Closure](../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md)；evidence envelope 见 [topology/node truth](../testing/templates/state-sync-closure-evidence-packet-template.md#topology-and-node-truth)、[peer-head/gap sync](../testing/templates/state-sync-closure-evidence-packet-template.md#peer-heads-and-gap-sync) 与 [bundle/blob closure](../testing/templates/state-sync-closure-evidence-packet-template.md#state-sync-bundle-and-blob-closure)。DC-5 还需要真实 linked structured attachment/schema，逐项含状态转换、manifest/head 负例、每 intent receipt0/1、consumer blocker/next-step；现有模板本身不闭合它。Ops同窗口事实、runtime根/receipt和消费者反馈由QA组合，缺任何适用字段不通过。

### des-p2p-replication-scope

writer/epoch从seq1及guard不污染；权威恢复错误与qualified storage-challenge fallback区分。

net 拥有 transport/peer/DHT；consensus 拥有 proposer/attestation/action-root；存储域由 cas/distfs 分担本地 CAS/challenge 与分布式 replication；node/chain-runtime拥有commit后执行/恢复接线；proto仅共享wire；wasm_abi是manifest/runtime ABI单一来源。公开服务只能供应hash/proof材料，不能投票或写canonical。

### des-p2p-authority-profiles

承接 [合法 profiles](prd.md#p2p-authority-profiles)、[单权威持久提交](prd.md#p2p-single-authority-durable-commit) 与 [H/H+1 交接](prd.md#p2p-authority-handoff)。同一世界/执行/receipt 协议按各高度合法 profile 验证，固定 world/chain/genesis 身份与可演进 manifest/profile/epoch 分开；受控单权威不宣称 distributed consensus，threshold prototype 不取得正式权威。

单权威追加先在最终存储原子检查 epoch、预期 parent/head、顺序；signer 在 fenced epoch 下核对完整 prepare 的本机与独立副本持久确认后生成提交决定/证明；最终证明和决定再按同一协议复制持久化。prepare 不发布世界效果；本机与独立故障域完整记录和正式提交决定/证明确认后才回 committed。单调 guard 与本机锁不能替代旧 writer fencing。未知结果保留原 identity 查询，独立副本不可用时暂停确认；人工恢复先隔离旧 signer/追加权，无法证明则停写。

#### des-p2p-local-authority-journal-prerequisite

本地持久日志仅承接 [单权威持久提交](prd.md#p2p-single-authority-durable-commit) 中的本机原子记录、请求去重及恢复查询前置。固定 trusted world/chain/genesis、signer 与 epoch；在本地独占进程锁内检查预期 parent/head 与原请求 payload digest，将签名 local decision、完整日志与派生的 head/request index 作为同一持久事务保存。同 identity 同 payload 查询原本地结果，异 payload 冲突；任何不确定写入沿原 identity 返回 unknown。

`oasis7_distfs::controlled_authority` 的 local decision proof 使用独立 schema/domain 与 `local_durable_prerequisite` 范围，offline verifier 只验证该范围的身份、签名和链接。它不构成已激活 profile 的正式 CommitRef，既有 runtime、manifest startup、WorldService 与旧 finality verifier 不消费它作为 committed。独占锁及固定 epoch 只防同一受信存储路径的本地竞争，不具备跨主机 fencing；没有独立故障域完整持久确认时不得发布正式效果。

恢复核对 trusted identity 与外部保留的最低 head anchor，重建日志/head/request index 并拒绝截断、签名或链接冲突；本地 snapshot 单独无法证明未被回滚，丢失外部 anchor 不自动降格为可信恢复。固定 epoch 不支持自行提升、signer 轮换或 H/H+1 激活。首次实现仅在支持文件和目录持久同步的 Unix 本地文件系统开放；Windows 或不支持目录同步的平台明确拒绝该新入口，既有 DistFS 路径不改变。回归入口为 `cargo test -p oasis7_distfs controlled_authority`，覆盖竞争写者、stale epoch/parent、相同请求与 payload 冲突、持久阶段故障与 proof 篡改；其运行结果由 [Issue #4363](https://github.com/eng-cc/oasis7/issues/4363) 对应候选维护。本切片不关闭跨故障域灾备、正式提交及消费者接线义务。

#### des-p2p-replicated-durability-prerequisite

`controlled_authority::replicated_protocol` 承接同一单权威持久提交义务中的两端记录协议，使用独立 `replicated_durability_prerequisite` 签名范围。固定 trust 绑定 world/chain/genesis、writer、epoch 与不同身份/密钥的 primary、replica；它不转换 local proof，也不接入正式 CommitRef、runtime、profile 激活或自动接管。

封闭记录保存输入、结果、snapshot、连续 journal、nonce/去重状态、外部效果 outbox、执行 manifest、WASM 与规则的完整字节对象，以及明确声明的引用闭包。接收端核对角色、长度、hash、重复和缺失对象；不访问外部 URL，不执行工件，也不证明调用方执行正确、玩家授权或任意 opaque 对象内部未声明引用。调用方仍负责可信执行记录与实际重放材料的语义完整性。

协议顺序为两端 prepare → primary 原子决定 → replica 保存同一决定 → 两端保存完整证据。primary 的同一持久快照原子维护 fixed epoch、预期 parent/head、决定与原请求索引；一旦决定持久化不可取消。prepare 不产生世界效果。同 identity 同 payload 查询/续办原决定，异 payload 冲突；任何可能已改变持久状态的错误保留 identity 为 Unknown，停止该实例追加并要求明确恢复。

prepare、primary decision 与 replica decision 分别使用签名域。接收端只有在完整记录和决定同步落盘后才签 durable receipt；离线 verifier 要求不同配置端点的两份同决定回执，单端签名无法构造合格证据。协调器将完整证据同步保存到两端后才返回 DurabilityQualified。该结果证明受信接收端声明满足所配置同步合同；签名和两个本地目录不能独立证明实际硬件持久性或故障域隔离，也不是产品 committed 状态。

恢复先验证 fixed trust、完整记录、签名/链接与外部最低 head anchor；anchor 区分不可撤回的决定与已有双端证据，不从 prepare 推导效果。primary decision/replica prepare 只能续办原决定；同位置分叉或材料冲突隔离停写。任一端丢失后，幸存端的完整合格证据可验证原记录；恢复写服务仍须重建缺失副本并重新满足双端条件。固定 epoch 不授权更换密钥/端点或人工接管，旧 signer 与实际最终存储访问控制仍需部署隔离合同。

本切片仅支持可同步文件与目录的 Unix 受控文件路径，防同路径进程竞争；不防同权限恶意进程替换目录/锁 inode。回归入口为 `cargo test -p oasis7_distfs --lib replicated_protocol`，覆盖持久阶段崩溃/丢回复、同请求重试、parent 竞争、prepare-only、单端丢失、伪造回执与错误 anchor。候选及结果由 [Issue #4363](https://github.com/eng-cc/oasis7/issues/4363) 维护；实际独立故障域、存储防绕过、正式消费者接线与 epoch 恢复尚未完成。

交接停写排空后，新集合同步 H−1；旧 authority 在 H 正式记录新集合/profile/epoch/H+1 边界及结果根。H 提交前可取消，提交后接续权不可撤回，入口/存储/signer/proof verifier 均拒绝 H+1 旧 profile；新集合启动失败保持停写。历史仍按各阶段合法 profile 验证。演练覆盖 H 已提交而 H+1 未提交时宕机/取消、重启拒绝旧 writer，以及无资产重发/断链/失效 pending 自动生效。该设计尚需协议、消费者与故障域实证，不是当前 readiness。

#### des-p2p-controlled-history-reexecution-prerequisite

execution bridge 的历史 capture 为显式 opt-in（`--capture-schedule-recipe-history`，默认关闭），只支持每高度恰好一个带既有已验证 ingress origin 的 `ScheduleRecipe`。它在真实执行前保存原始 Snapshot、Journal、上下文及实际 security policy，将该引用绑定进首次 product-validation intent 原子发布；执行后封闭包保存九角色真实字节、typed module registry 所声明的全部 WASM 和前缀关联。启用时材料持久或校验失败阻止记录发布并回滚内存执行；旧记录的 None 不反填，普通/BFT 默认路径不改变。

这里的 outcome index 仅是所支持连续记录前缀派生的 execution request outcome 累积索引，绑定既有 player/key/nonce/session tuple、实际提交 CBOR hash、parent/input/result。它不是 GameplayNonceLedger，不证明 ingress reservation 已成为正式提交，也不覆盖其他操作或所有 nonce domains。origin 是现行 ingress 签名、nonce 与玩家身份校验所得的受信 producer 请求身份；包中 tuple 不替代原玩家签名证明、Agent 控制 grant 或 Hosted issuer 授权，离线验证不授予玩家控制权。

只读入口 `oasis7_chain_runtime verify-controlled-history --trusted-config <json> --evidence <json>` 要求操作员独立固定 world/chain/genesis、writer/不同端点 keys、epoch、最低 qualified head、初始执行 height/root/block 与 security policy。完整连续历史必须从声明的初始锚起，不能跳过缺 capture 的旧记录、prepare-only 或单回执。初始锚可选显式外部 published anchor：操作员在输入前保存原 V3 record、Snapshot CBOR、Journal CBOR 的路径和独立 BLAKE3 hash；trust JSON 仍限 64KiB，record 限 64KiB、Snapshot/Journal 各限 64MiB并保留原 typed budget/no-follow。其 world/height/block/root/CAS refs、journal 长度和原 driver diagnostic context 必须相符；初始 pre-state 可按已验证的 published/default 完整资源注解 pair 演进；若 World 保留原 bootstrap 缓存 pair，须在输入前独立保存原 typed Snapshot CBOR 并固定 BLAKE3 hash，通过可选 `published_initial_anchor.cached_resource_snapshot` 的 `snapshot_path`/`snapshot_hash` 提供；所有外部锚文件路径必须为绝对路径。该文件沿用 Snapshot 的 64MiB、typed budget 和 no-follow 限制，只认证其完整 manifest/delta pair；world 必须对应 published record，缓存时间/manifest height 不得晚于锚，delta 仍按原完整 manifest/context 校验。全部其余当前 state 和 Journal 原字节连续，真实 root 不替换。原缓存只属 diagnostic annotation，不授予 genesis/profile/Agent authority；从待验证 capture 后取出的 pair 不能作为预先固定的外部信任。原 exact raw pre-root 路径保持兼容。入口验证双端证据、请求和实际 payload、所有 typed 引用闭包与前缀，再以原始 Snapshot/Journal、原模块字节和相同 committed context 调用既有 Runtime 的真实 WASM step，比较完整结果 Snapshot/Journal、state/block root 与 effect 材料。内存恢复不配置持久目录或 dispatcher；WASM 使用既有空 Linker，无新 LLM 调用、网络 host function 或替代模块。

连续性保留完整 Snapshot/Journal 字节约束，仅两个已验证资源注解允许既有缓存、重开或按实际 manifest hash/journal 长度生成的默认完整 pair 演进；delta 必须绑定相同 manifest，不能混合 pair 或任意去字段归一化 root。真实 pre/post roots 始终绑定原始材料。当前 driver 的 diagnostic resource context 使用 chain_id=world_id、genesis_ref=None，这些注解不证明正式 genesis；外部 fixed trust 不补写历史身份。

文件输入有界、拒绝观察到的祖先/末端 symlink，支持范围为 Unix 操作员受控目录；不承诺防同权限恶意进程替换祖先。合格输出仅为本地持久协议与重执行前置验证，不转换正式 CommitRef、不激活 planned profile、不改变 BFT、开放全路径 nonce 共持久或创建新世界。Rules 角色保存实际前后 World Manifest 配置，原生配置绑定在 Snapshot，security policy 与操作员配置核对；原 Runtime 没有可历史加载的 native Rust 规则二进制工件，本入口仅证明同候选 Runtime 语义下重执行，不宣称任意旧 native 版本的可移植 replay。真实候选须显式打开 capture 生成新材料；既有未 capture 世界不能通过补造 sidecar 获得此证明。回归入口为 `cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime --no-default-features --features node-libp2p,wasmtime controlled_history`；当次命令、候选与结果另由工程 evidence 维护。
#### des-p2p-initial-activation-verifier-prerequisite

`controlled_authority::activation` provides an offline initial-activation prerequisite. It does not change manifest v2 planned status, startup readiness, legacy finality, or CommitRef. The caller independently authenticates an issuer key and fixes world/chain/genesis, initial epoch, writer/P/R, initial state root, execution manifest root, activation height, and the external genesis head. Certificate declarations, self-signatures, and configuration hashes cannot establish this authority. Writer/P/R keys must be pairwise distinct.

A dedicated-domain strict schema v1 signature binds `controlled_single_authority` profile/version 1, position 1, genesis parent, height, before/after state roots, and all eight non-Input artifact roots. Input is the original canonical CBOR signed envelope, whose BLAKE3 must equal the record payload digest and Input root. The signed body excludes Input to avoid circular signature/hash binding. The envelope is bounded to 16 KiB; existing closed-record budgets remain unchanged. Verification requires the complete nine-role record, writer proposal, and both P/R DecisionDurable receipts. Prepare-only evidence, a missing receipt, a wrong external anchor, and later positions fail closed.

Only successful verification constructs opaque `VerifiedInitialActivation`. It is not yet connected to execution-semantic verification, formal CommitRef, durable epoch publication, handoff, or runtime activation. Real FileEndpoint/coordinator fixtures exercise the local synchronous filesystem contract; they do not prove retained-world issuer authorization, independent deployment fault domains, or a formal opening. Upgrades require an explicit old-profile transition action and evidence chain; this initial-only verifier rejects them. Regression: `cargo test -p oasis7_distfs --lib activation`.

#### des-p2p-certificate-bound-bootstrap-anchor-prerequisite

The read-only execution bridge `controlled_bootstrap_anchor` consumes an opaque `VerifiedInitialActivation` and its exact original ClosedRecord. It checks Input digest, all eight signed roots and state roots before decoding. Version 1 accepts a no-state-change bootstrap marker only: before/after roots equal the complete original runtime Snapshot digest. It does not prove or apply an execution transition. No caller can deserialize or construct the opaque validated bootstrap output directly.

A strict `ActivationArtifactManifestV1` at ExecutionManifest binds world/chain/genesis, the existing schema-v3 ExecutionBridgeRecord checkpoint height, the separate simulation tick, the predecessor execution-block hash, and seven artifact roots. It excludes its own digest and Input to avoid circular dependencies. Height comes from the certificate-bound ExecutionBridgeRecord, never from `WorldState.time`. The existing ExecutionHashPayload algorithm recomputes block/state/journal consistency using the manifest-bound predecessor; that predecessor is an issuer assertion, not an independently verified history chain.

Snapshot and Journal use existing Observer decoding budgets and World::from_snapshot. This narrower contract requires a complete journal with a nonempty canonical prefix commitment, exact restored bytes, and the concrete declared module bytes with no extra or missing artifacts. WASM uses the existing capture registry/byte-reference closure; Rules binds the actual Manifest, PolicySet and externally fixed ReleaseSecurityPolicy. EffectOutbox is the existing pending/inflight effect tuple. NonceIndex v1 is specifically the existing capability authorization nonce-record map; it does not claim universal gameplay request-index recovery. Result is the actual typed execution checkpoint record. Exact reconstructed objects/references reject aliases, opaque placeholder roles, missing/extra objects and undeclared dependencies.

Runtime Snapshot has no separate canonical world/chain/genesis authority field. Authority therefore comes from the independently authorized activation certificate and its pre-pinned manifest; ChainResourceManifest identity annotations are consistency checks, including regenerated manifest/delta equality with the restored state. Absent genesis annotations and existing diagnostic history using genesis_ref=None fail closed and are never rewritten. This is a certificate-bound restored checkpoint prerequisite, not independent canonical-genesis lineage, runtime activation, readiness, CommitRef, a new journal or automatic takeover. Original historical Snapshot/Journal remain unchanged. Regression: `cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime --no-default-features --features node-libp2p,wasmtime controlled_bootstrap_anchor`.

#### des-p2p-offline-bootstrap-cli

`oasis7_chain_runtime verify-controlled-bootstrap --trusted-config <external-json> --evidence <original-proof-json>` is an offline consumer of the initial-certificate and typed bootstrap verifiers. Dispatch occurs before startup-option parsing, Node construction and services. It has no signer, key/world creation, provider, directory persistence, readiness transition or manifest-v2 planned-profile relaxation. It reads the original Input envelope from the supplied single DurabilityEvidence record, then validates the independently fixed policy, external minimum head and exact original typed closure. Files are never repaired or rewritten.

Trusted JSON schema version 1 contains `issuer_policy` (the existing fixed trust, issuer_public_key, initial_state_root, execution_manifest_root, activation_height), `minimum_head`, and `release_security_policy` with all four existing policy booleans explicitly supplied. Unknown fields at every configuration layer and unsupported versions fail closed. This wire declaration explicitly constructs InitialActivationPolicy; it does not make that opaque trust boundary deserializable or authenticate its own issuer/world authority. The operator must authenticate the policy independently before invoking the command. No real issuer/world identity is generated by this entry point.

Config is limited to 64 KiB and original proof JSON to 64 MiB; the existing closed-record, signed-envelope, Snapshot and Observer typed budgets still apply. Reused offline file reading rejects observed ancestor/final symlinks and nonregular/oversized files on Unix operator-controlled paths; it does not guarantee protection against hostile same-permission ancestor replacement. Trailing JSON data, changed signed material, wrong roots/profile/minimum, absent original genesis annotations and unsupported artifact semantics are rejected. Output explicitly states the certificate-bound no-state-change checkpoint prerequisite and caller-authenticated trust basis; it is not CommitRef, live activation/readiness, independent fault domains or independently established genesis lineage.

Ordinary process regression seals an isolated engineering fixture before invocation, exercises early dispatch in a child process for success and failures, and verifies unchanged input bytes and directory contents. The same regression also invokes the actual built chain binary when `OASIS7_BOOTSTRAP_CLI_TEST_BINARY` is explicitly provided during final verification. The internal ignored child helper is invoked by the always-running parent; it is not an omitted acceptance test. Run `cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime --no-default-features --features node-libp2p,wasmtime,test_tier_required controlled_bootstrap_cli`.

#### des-p2p-guarded-local-initial-execution-prerequisite

Deployment is deferred by the user. The explicit `dev guarded-world --config <external-json>` engineering command installs the existing NodeRuntime execution hook and existing signed WorldService HTTP protocol. It does not activate the planned manifest-v2 formal profile, claim independent hardware fault domains, create an issuer/world/genesis identity, or enable Viewer/production-launcher consumption. Ordinary startup and ordinary client assurance retain their existing fail-closed boundaries.

Strict configuration version 1 has scope `guarded_local_execution_prerequisite`, `node_id`, existing private `node_keypair_directory`, private `records_directory`, loopback `status_bind`, `node_tick_ms` (1–60000), `trusted_bootstrap_config`, original `activation_evidence`, P/R directory paths, existing writer/P/R key-file paths, and an independently supplied `minimum_runtime_head`. Relative artifact paths resolve against the external configuration directory; the provisioned node-key directory must resolve to an absolute protected path. Existing activation/bootstrap verification runs before ordinary startup mutation. On Unix, P/R and node-key directories require current-user ownership and mode 0700; existing signing key files require current-user ownership, regular non-symlink files and mode 0600. Unsupported platforms reject this branch; it never generates keys or changes permissions of operator inputs. This config is independently authenticated operator trust input, not issuer authorization proof. The branch rejects external transport/provider/consensus overrides, constructs no libp2p/gossip/discovery/replication network, uses the existing single-local-validator decision machinery, and admits only existing authenticated WorldService routes on loopback. Public CAS/checkpoint/observer and legacy submit handlers are absent from this branch.

The Node captures the complete original local decided context, timestamp, predecessor hashes, action reservation, slot/height counters and actual stake totals before invoking the hook. Durable private continuation precedes execution. `Deferred` retains that exact decision and blocks successors; it never substitutes a network decision or reruns proposal/attestation evaluation. Candidates restore detached World::from_snapshot state, preserving no published-world persistence binding. Original signed WorldService input, complete Snapshot/Journal, result ledger and effect material remain exclusively in private operator storage. No candidate read cursor, external dispatcher or checkpoint export is available before qualification. A crash after execution starts but before a complete candidate becomes durable leaves recovery blocked: it fails closed without automatic executor replay, replacement nonce or repair. Only a retained complete original candidate supports the tested Unknown-to-restart continuation; formal PRD §9.2 service recovery remains unfinished.

The versioned generic nine-role closure preserves the existing ScheduleRecipe capture contract. Input holds exact original Node actions including signed request bytes; Manifest binds the original continuation, predecessor checkpoint and separate before/after simulation ticks. Snapshot/Journal include exact predecessor objects and complete journal-prefix continuity. Rules/WASM use concrete existing policies, module registry and module bytes; nonce closure explicitly covers capability nonce records, authenticated gameplay nonces, WorldService results and agent intent ledger. EffectOutbox includes existing pending/inflight state and original execution-effect material. Result and ExecutionHashPayload bind actual chain checkpoint height independently of simulation tick. Existing closed-record, Observer and capture budgets remain unchanged.

P/R `Unknown`/`Pending` retain the same request, ClosedRecord and candidate without successor execution. Qualification independently verifies both receipts, exact package/result/root/module closure and predecessor ancestry before immutable publication. The durable published checkpoint is saved before `Applied`; original continuation remains after Node completion for crash recovery. Restart reconciles the exact retained request against real locked endpoint status/head and the complete immutable published prefix. Missing acknowledged tails, changed config, alien heads and ordinary replication rollback/checkpoint/synthetic-decision recovery fail closed. An in-process monotonic verified-head fence prevents current HTTP reads from regressing after tail deletion; explicit historical pins still verify the entire prefix and exact requested CommitRef.

All signed Describe, IntentResponse, ReadView and Changes responses bind optional `execution_evidence_scope=replicated_durability_prerequisite`. Transport CommitRef identifies this engineering cursor only; it does not establish formal finality/readiness. Absent scope preserves ordinary serialized bytes. Ordinary RemoteWorldServiceClient rejects any present scope; explicit `ReadEvidencePolicy::AllowControlledPrerequisite` accepts exactly this scope and rejects unknown or omitted scope. Read authority identity binds that policy. Existing Viewer remains ordinary and rejects this backend; the bounded consumer is the explicitly configured pure API client for Describe, Submit, Lookup, ReadView and Changes only, with existing projection/agent authorization unchanged. The owner-read helper refuses engineering policy before transport; ordinary signed owner reads remain supported, and no engineering owner/Viewer capability is enabled.

Focused fixtures use real local FileEndpoints, actual NodeRuntime decisions and signed WorldService requests, with endpoint-file unavailability and restart recovery; two local directories do not prove independent hardware. Verification command: `cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime --no-default-features --features node-libp2p,wasmtime,test_tier_required controlled_live`.

#### des-p2p-formal-rpc-frame-preparation

现有 formal TCP 客户端须在建立连接前完成原签名请求的 CBOR 帧编码及既有 64 MiB 大小检查，避免本地编码空窗消耗端点已经开始等待首字节的 socket read 期限。此顺序不改变请求、签名域、挑战、minimum、帧字节或响应验证。每次调用仍在原位置确定一次 effective absolute deadline；编码后先核对剩余时间，再连接、写入已编码帧和读取响应，所有步骤消费同一 deadline，不在编码或连接后重新给满预算。编码失败、超限或编码期间到期须在零连接、零发送下失败；已有调用方提供的跨 RPC absolute deadline 继续覆盖其准备工作，不能通过局部帧准备续期。同步编码本身不声称可抢占，返回后到期仍必须拒绝连接。

端点 socket read timeout、客户端配置期限与测试等待期限保持原值；不能把端点首读等待误称为完整帧的 absolute deadline。未知结果仍查询原请求，不自动重试或推进信任锚。验证须覆盖受控慢编码时连接数为零、编码后过期/失败/超限零连接、原帧往返及现有慢分片 absolute deadline；小 loopback 模型只证明该排序契约，不追认历史 CI 超时或 RPC EOF 的根因。

### des-p2p-original-candidate-recovery-v1

本节选择“同原请求、同原决定输入的确定性 candidate 重建”，作为正式受控权威的目标协议。
它不是当前 guarded-local 的自动重放开关：现有完整 candidate 落盘前崩溃仍按
[工程边界](#des-p2p-guarded-local-initial-execution-prerequisite) 停写。
只有本节闭包、正式激活与故障验证全部实现后，新入口才可声明支持此恢复能力。

#### 执行前持久闭包

每个世界最多有一个已固定但尚未完成提交的 execution slot；未收敛前不选择或执行后继 slot。
选定输入只是不可替换的执行决定，不等于世界已经 committed。
执行器第一次进入前，`PreExecutionContinuationV1` 及其可达对象必须在 primary 和独立
replica 完整校验并同步落盘；两端返回同一 continuation digest 才允许执行。
只完成单端保存时只能续存同一记录，不能开始执行或另选输入。

该版本的必需材料如下；引用均为内容 hash，所指完整字节也属于持久闭包。

| 材料 | 必须固定的内容与校验 |
| --- | --- |
| 身份与位置 | `world_id/chain_id/genesis_hash`、合法 activation ref、profile/version、authority epoch、全局 position/height、预期 parent CommitRef/state root、原 Snapshot/Journal 与已验证历史锚。 |
| 原请求与 admission | 原始已签名 envelope 字节、请求 identity 与 payload digest、所有适用 nonce domain、原 nonce reservation、授权检查依据及原有序 action 序列；不从重建结果反推身份。 |
| 原决定 context | Node 已固定的时间、slot/height、stake、前驱 hash、simulation tick 和计数器；原执行使用的随机输入或确定性随机流及其算法版本。恢复后主机时间或新随机数不能替代。 |
| 精确执行版本 | 原 native executable 字节及 digest、受支持 target/ABI/feature 与确定性配置，原 runtime manifest、全部 WASM、规则与 typed 引用闭包；版本字符串或当前二进制不足以恢复旧语义。 |
| 无外部效果边界 | 执行所需 provider 结果已作为原输入固定；candidate 只能访问上述输入和内存暂存。LLM、HTTP、当前权限查询、主机时钟/熵及外部 dispatcher 均不进入重建过程。 |
| 持久控制 | continuation digest、原请求索引、预留执行/存储预算及状态。原 nonce reservation 随记录保留，不释放给 replacement，也不第二次扣除。 |

continuation 使用独立的非最终性签名域；签名只承诺选定输入，不冒充正式提交证明。
执行前按既有域容量规则预留闭包与 candidate 空间，超限拒绝进入执行；未决闭包不参与 GC。
native 历史工件不可用、平台不能复现原语义或原执行需要未冻结的外部输入时，进入
`recovery_required`，不能用当前 Runtime、mock provider 或补造 sidecar 继续。

#### 恢复顺序与写入一次

重启先核对两端持久状态、原请求索引、已发布 checkpoint、正式证明和外部最低 head 锚。
单端 `NotRecorded`、超时或本地 marker 不能证明请求未提交。
先采用已决定/qualified/published 事实；只有排除冲突历史且需要原 candidate 时才调用执行器。

| 查明的原请求阶段 | 唯一允许的后继动作 |
| --- | --- |
| 尚无固定 continuation 的 pending | 保持原 signed intent；服务恢复后按当前权限、资源、期限和 governing manifest 重新 admission，尚不适用原决定 replay。 |
| continuation 已完整持久、execution 未开始 | 校验原闭包和预期 parent；以原决定执行一次。当前 snapshot 不能替代原 parent。 |
| execution 已开始、完整 candidate 未持久 | 在 detached World 上用精确原输入重建 candidate；无磁盘发布绑定或 dispatcher。与任何已保存 proposal/result hash 对账后续存同一 candidate。 |
| 完整 candidate 已持久、尚无持久提交决定 | 校验完整九角色记录与原 continuation，续办同一 prepare/decision；不再次执行。 |
| primary 已持久决定、尚未 qualified | 决定不可撤回；补齐该决定的副本、回执和完整证据，不能重选结果或判失败。 |
| 已 qualified、尚未发布 | 安装同一不可变 checkpoint/result/nonce/outbox generation，持久发布指针后才回应；不再次执行。 |
| 已发布、原回应丢失 | Lookup/同身份重试读取原 disposition、结果和 CommitRef；不产生第二 receipt、里程碑或外部派发。 |

重建阶段的 `candidate_put(original_identity, payload_digest, continuation_digest)` 是持久
put-if-absent：同 key 同完整结果返回既有对象，任一 digest/root 不同即隔离。
任何残存 prepared proposal 必须与重建的全部九角色 bytes/roots 相同；不能仅比较业务结果。
原请求结果、业务状态、nonce/去重、里程碑和 outbox 沿一个正式记录及完整 generation 发布，
只能写入一次。候选内计算可以重新运行；对外效果仍仅由 committed outbox 按既有幂等合同派发。
这与 [Runtime 的 canonical history replay](../world-runtime/design.md#runtime-controlled-candidate-recovery-receiver)
是不同阶段，不放宽历史 replay 的不执行外部效果约束。

材料缺失、不同端点决定冲突、未知更高 head、原版本不可执行或重建不一致均保留原身份并
进入 `recovery_required`；只允许补回可验证的原字节或续办原持久决定。
重试次数、经过时间、用户换 nonce 或恢复后权限变化都不能把已固定决定变成失败或新动作。
恢复任务按世界串行，有界占用既有预算；没有新材料或状态变化时不无限自动重放。

### des-p2p-formal-controlled-authority-v1

本节规定目标 `controlled_single_authority` profile version 1 的正式发行与消费协议；
实现仍缺。它沿用两端完整持久判定，新增正式激活链、proof 编码、不可绕过的 signer/storage
admission 和普通客户端验证，不能通过删掉 `execution_evidence_scope` 升级工程记录。
此 profile 仍信任运营权威的执行正确性，不声称独立验证者 BFT。

#### 初次激活与证明编码

初次 issuer 来自部署前独立认证并固定的创世授权；不能由待验文件自行提供可信 issuer。
`FormalAuthorityActivationV1` 固定世界/chain/genesis、初始状态与执行工件、profile/version、
initial epoch、writer/primary/replica 身份和互不相同的 key、允许的恢复 issuer/key 与恢复权限，
以及部署 inventory 中的独立持久域引用。已有保留世界必须沿其合法身份与历史接续，
没有合法历史证明时阻断，不能以该证书重创世或把工程历史追认为正式提交。

issuer 的证书只授权首次 activation record。其签名 body 绑定初始 height、genesis parent、
初始 state root、执行工件和除 Input 外的八角色 roots；Input 是完整证书的 canonical 字节，
随后由正式 commit core 绑定 Input root，避免证书递归包含自己的 hash。
primary/replica 分别验证证书、完整 typed bootstrap 闭包及 genesis 身份，保存同一无业务
状态变化的 activation record。两端正式证明完整持久、checkpoint 安装且只读复验通过后，
才可构造不可由配置直接生成的 `ActiveAuthorityV1`；离线 `VerifiedInitialActivation`
和 bootstrap anchor 仍只是 prerequisite，不能转换为该对象。

正式对象使用新域 `oasis7.controlled_authority.formal.<kind>.v1.<purpose>`。
`kind` 仅为 `core`、`proof`、`activation`、`continuation`、`commit`、`primary_prepare`、`replica_prepare`、
`primary_decision`、`replica_decision`、`recovery_plan`、`epoch_seal`、`epoch_activation`。
`purpose=object` 对完整 typed 对象（含已有签名封装）求内容 digest；`purpose=signature`
只对该类型的未签名 body 签名，body 不含自己的签名。`core/proof` 只允许 object purpose，
`request_commitment`、`recovery_requests` 与 `pending_handoff` 是另三个仅供 object digest
使用的 kind，分别承诺原请求、恢复 allowlist 和待交接 pending 集合，不能签成正式提交。
编码固定为无浮点、无重复/未知字段、整数最短表示的 deterministic CBOR；map key 以编码字节
长度及字节序排序，数组保持协议顺序，缺失必需值和显式 null 均拒绝。
domain UTF-8 长度以 u64 big-endian 前缀编码，再串接 domain 和 body，形成 framing bytes。
BLAKE3 对 framing bytes 计算内容 digest；Ed25519 直接签完整 framing bytes，验证使用
strict verification，不把 digest 再当作待签 message。key/hash 为 32 bytes、signature 为 64 bytes。
这要求专门 codec 和 golden fixtures，不能把现有 serde 编码函数直接标记成新正式编码。

| 对象 | 必需绑定与准入规则 |
| --- | --- |
| `FormalControlledCommitCoreV1` | record kind、固定世界/chain/genesis、全局 position/height、epoch 内 sequence、parent commit hash/state root、result state root、activation certificate digest、profile/version/epoch、runtime/native/WASM/rules 工件闭包的 commitments、九角色 roots；execution 另绑定 `request_commitment` 与 continuation digest。公开 core 不包含原主体/nonce tuple 或原 payload。 |
| `FormalControlledCommitProofV1` | core digest、writer decision、同一 core 的 primary/replica prepare 与 durable decision 回执、适用 activation certificate 及其历史证明引用；角色 key 匹配当时合法 activation，公开 proof 引用逐一验 hash。九角色私有字节闭包只由受控持久/恢复端核验。 |
| 存储 head / 请求索引 | 原子 compare-and-append 校验世界、epoch、sequence、预期 parent 和原请求 identity/digest；同位置只能有一个 core，持久决定不可替换。允许确认的最高 head 与完整 proof/record 一起保存。 |
| `CommitRef` | 映射到该 core 的世界、height、commit hash、parent 和 state root；仅凭 WorldService 签名或格式正确不能构造可认可的正式 CommitRef。 |

签名不循环：activation certificate digest 不包含承载它的 commit proof；core 不包含自身
proof。prepare receipt 只绑定 core digest 与接收端角色/身份，不等待 writer commit 签名；
writer decision 的签名 body 绑定 core digest 和两端 prepare receipt digest；decision receipt
再绑定该完整 writer decision digest，完整 proof 是这些对象的外层封装。
first activation 使用 position 1 和 genesis parent；以后 position 连续，epoch 切换不重置
全局 position，epoch 内 sequence 从 1 起。普通 execution 的高度/上下文保持原决定，
控制记录不伪造 gameplay tick 或资源效果。

`request_commitment` 是专用 `request_commitment` 域中对原请求 identity、payload digest
和 continuation digest 的承诺；该域仅计算 BLAKE3 内容 digest，不签发提交证明。
原 tuple、原签名和 payload 留在授权 Lookup 及私有 Input，普通读者只核对承诺与正式签名。
公开 proof 验证证明受信 authority 对该 root 的正式承诺，不证明读者已经重执行私有状态；
ReadView 的私有投影继续依赖受权服务对精确 CommitRef 的签名绑定。

执行采用“两端完整 prepare → 合法 writer 签 commit → primary 持久不可撤回决定 →
replica 持久同一决定 → 两端保存全部 proof/record 与可确认 head → 发布”的顺序。
signer 仅能通过受控服务使用 key，按 record kind 验证授权：execution 校验当前 active
epoch、parent、两端 prepare 和原 continuation；initial activation 校验独立固定的 issuer
证书、genesis 空 head 与两端 prepare；epoch activation 校验既有 recovery policy、全部
seal、唯一新绑定和两端 prepare。后两者不要求先存在待构造的新 ActiveAuthority，且只允许
对应的那一条控制记录；不能把控制记录授权用于 execution。
writer 不能直接读出 signer key、替换最终存储文件或绕过接收端追加检查。
任一端丢失后，幸存端保存的完整 qualified 前缀仍可验证已确认历史；缺副本时只读，
恢复写入必须先恢复完整双端条件。独立域和实际 fsync 能力另须部署事实证明，不能由签名
或两个目录推导。首次正式开放及单故障域演练仍由现有运行验收闸门控制。

<a id="formal-replica-staging-content-reuse"></a>
#### 健康 Replica 归档安装中的内容复用

健康集合安装仍先验证 archive transfer 的签名 checkpoint、seal 和新绑定，再遍历完整认证闭包。copy walk 内的共享索引路径及已验证 Result envelope 的 business delta node writes 可以重复引用相同内容；每次引用不必再次进行远端签名 chunk 传输。

一次 `copy_authenticated_closure(copy=true)` 内可使用最多 16 MiB、4096 个对象的有界内容缓存，所有接纳字节先核对原 CAS digest；仅回填当前已验证材料内的 inline writes 或本次源读取的正确字节。16 MiB 约束保留对象的 bytes 权重，key/map、解码及返回值等临时分配不在此限内，不代表总 RSS 上限。缓存只提供相同 digest 的 bytes，每条引用边的 schema、深度、head/root/上下文、proof 和整体闭包预算仍独立验证。容量不足时按原源读取路径继续，不放宽预算、不缓存 absence 或任何授权/资格判定。缓存每次 walk 新建，失败、重启及跨 manifest 不复用；预复制的 raw seed 不成为缓存接纳依据或权威捷径。

provider 的 `copy=false` 独立本地 descriptor 读回不使用该内容缓存；目的端原文件校验、fsync、绑定与持久安装顺序保持。实际读取或接纳的源 bytes 与 digest 不符、缺边、错误 manifest/authority 及目的端已有对象损坏继续拒绝；已认证 inline Result 中完整的相同 digest bytes 可以用于重建目的逻辑对象，但不能据此声称原源副本完整或可用。分别记录 transfer 验证、closure copy、binding/persist 的单调耗时及源对象读取次数/字节、缓存命中/未命中；源对象读取数不冒充 chunk RPC 帧数，不记录 payload 或凭据。回归使用真实闭包与计数源证明重复读取减少，并验证容量退化、坏 hash、每条边校验、跨 operation 隔离和独立 provider 篡改拒绝。

#### 普通 WorldService / client 消费

目标正式服务固定使用 `WorldService contract_version=2`；v1 原 wire/signature 和工程策略保持
现状。v2 签名响应新增 `formal_authority_ref`；凡声明 committed view/result 的响应另必需
`commit_proof_ref`，二者均为内容 digest 引用，响应请求关联与原有字段也在 v2 签名内。
其中 `formal_authority_ref` 指向已持久 activation record 的完整 proof digest，而非单独的
issuer 证书；`commit_proof_ref` 指向本次结果记录的完整 proof digest。普通 execution core
中的 certificate digest 必须与前者证明的合法 activation 对应。
v2 不接受 `execution_evidence_scope`。签名域必须包含 contract version；不允许 v2 转 v1
或丢字段后重新签名来兼容正式消费者。JSON 中 u64 沿现有十进制无损约定，正式 proof 字节
由上述 codec 独立验证。未来 active tier 只引用已验证 activation，见
[manifest 接收边界](blockchain/formal-network-tiers-testnet-mechanism.design.md#formal-controlled-activation-receiver)。

| 路由 | 正式消费与失败后行为 |
| --- | --- |
| Describe | 返回经过验证的 activation/profile/epoch、proof scheme、当前 service grade 和 head；消费者核对独立固定的 genesis/issuer 与完整历史链。配置声明或服务签名不足以把 planned 变为 active。 |
| SubmitIntent | admission 检查固定世界、当前 active epoch 和当前授权；accepted/pending 不产生 committed 声称。进入原决定后固定原身份与 continuation，最终结果必须带可验证 proof。 |
| LookupIntent | 先验证查询主体与原请求关联，再查原持久索引；原结果不能因旧 Submit 的 freshness 或现行 execution 版本被重算。未决/未知/无效果拒绝沿既有 API 合同呈现，只有合格证明支持 committed。 |
| ReadView | 独立验证精确 CommitRef 的 profile/proof 与 root，再验证服务签名对该 root 和授权投影的绑定；公开证明不扩大 owner/private 投影权限。 |
| Changes | 每个 cursor 推进绑定连续的 CommitRef/proof；跨 epoch/profile 验证 activation 链。缺页、缺 proof、错误 genesis 或回退保留旧 cursor 并阻断推进，不混拼缓存视图。 |

#### Proof 字节交付与公开边界

上述引用必须可解析为完整公开证明。目标另设固定只读入口 `POST /v2/world/proofs`，
复用配置中已认证的 WorldService origin 与 v2 读请求/响应签名，不接受响应内任意 URL、
重定向 origin、文件路径或由 proof 声明的新可信服务 key。它只供应证明材料，不新增世界写入。
`ProofReadRequestV1` 绑定 world、request correlation，并且恰选一种查询：
`objects` 请求至多 16 个去重的内容 digest；`history` 请求从已验证 `after CommitRef`
到固定 `through_commit_hash` 的连续公开记录，单页至多 16 条。
响应回显原 world/query，完整返回对应对象或显式 `missing`；history 必须从 after 的直接
后继开始，返回最后 record 的 cursor 和 `complete`，不能跳页或由服务改写目标 head。

公开对象种类只含 core、正式签名与两端回执、activation certificate、epoch seal、公开
recovery authorization 和历史 header/proof。单对象 canonical bytes 上限 64 KiB，单次响应
编码后上限 2 MiB；未知 kind、超限、重复/额外对象、错 hash 或非 canonical 编码拒绝。
历史引用单调向既有 parent/epoch，循环或不连续拒绝；客户端按既有请求预算逐页验证，
只保存已经验过的连续锚，目标 proof 未闭合前不认可目标结果/推进其业务 cursor。
缺页、未取得证明、取消或超时只使本次核验保持不可用，并保留原请求与最后已验锚；
不能退回“只验服务签名”，也不能把同请求改成新动作。

接口使用显式公开类型白名单，不从 ClosedRecord 通用序列化输出字段。九角色的 Input、
Snapshot、Journal、nonce 全账本、outbox、目标正文、账户私有状态及完整执行闭包不在
公开 proof DTO，也不能通过其 root digest 从此入口读取；返回这些字段属于协议错误。
public recovery authorization 只含冻结请求集合的 commitment，不含原主体或请求列表；
运维隔离证据也只公开引用 digest，实际凭据/实例控制信息仍受控。
授权 Lookup 才把调用者原 tuple/payload 与 `request_commitment` 对账；proof 可读不授予
owner 读取权、原始账本读取权或重建 private projection 的能力。

普通客户端、Agent 与 Viewer 共用正式 verifier；服务 grade 与提交 proof 分别校验。
当前 authority 不可用但历史完整时可以提供明确的 verified read-only 历史；不能恢复写入。
客户端持久保存已接受的最低 head/activation 锚，重连拒绝低于该锚的历史；无锚的新客户端
仍须使用独立认证的 genesis/issuer，不能信任 endpoint 自报的新世界。
原请求 Unknown、rejected 及重试规则由 [外部 API 恢复协议](../world-simulator/llm/external-agent-game-api.design.md#63-操作状态、回查和恢复协议)
继续主责，本节只增加证明消费，不引入一套替代结果状态。

| wire / evidence | 既有 v1 客户端 | 目标正式 v2 客户端 |
| --- | --- | --- |
| v1 且无工程 scope | 保持现有兼容行为 | 只能识别为兼容服务，不能打开正式世界或确认正式提交。 |
| v1 且为已知 prerequisite scope | Ordinary 拒绝；显式工程 policy 按原有界入口消费 | 拒绝正式使用；不通过删 scope、换签名或重新包裹提升旧记录。 |
| v2 且完整已知正式 proof/activation | 版本不支持，拒绝 | 全部身份、历史、root、签名和路由授权验证通过才接受。 |
| v2 缺该响应必需的 proof/activation、混工程 scope、未知 profile/版本/签名域，或未知 wire version | 拒绝 | 拒绝且保留原请求/cursor，不回退旧 verifier；真实 pending 无 committed 声称时不要求结果 proof。 |

### des-p2p-controlled-epoch-takeover-v1

目标 v1 采用人工停写与硬隔离，不使用 timeout/lease 推选新 authority。
创世授权只允许恢复 issuer 为同世界 controlled epoch 指定受限恢复器及唯一 E+1 绑定，
不授予它任意业务写入、改资产/授权、另建 genesis 或撤销 BFT handoff 的权限。
旧机器从硬隔离完成起即不能新增任何签名或存储写入；旧 key 在排空原请求期间仅由受限
恢复器使用，封 epoch 后才彻底退役，避免原决定已冻结却无人能完成其正式签名的死锁。

#### 权限与持久状态

`RecoveryPlanV1` 绑定固定世界身份、唯一 `recovery_id`、E/E+1、先前 activation、两端
可确认 head/外部最低锚、冻结原请求集合的 commitment、受限恢复器公钥、
原 writer/primary/replica 绑定、预定新绑定，以及实际隔离证明引用；由既有 recovery issuer
签名。每个 E 的计划用 compare-and-set 持久保存；同 ID 内容不符或第二份竞争计划拒绝。
完整冻结集合保存原 identity、continuation digest/position 和必要 payload，属于两端与
signer 的私有恢复闭包；按原 identity 的 canonical 编码排序，在 `recovery_requests` 域
计算的内容 digest 必须等于计划 commitment，不能从公开 proof 枚举。
issuer 签的就是上述含 commitment 的公开 plan body；公开 verifier 收到完整原签名 body，
不是从已签私有 allowlist 对象中删字段。受控端另持有完整私有集合并核验 commitment。
单世界单 execution slot 使允许继续执行的原决定集合至多一个；尚未选定的 pending
保留原身份，但不进入旧 epoch 恢复签名 allowlist。

实际隔离必须同时撤销旧进程对 signer 和接收端的凭据、终止并阻止旧实例重启/重挂盘、
撤销直写卷/对象存储路径及备用凭据。存储和 signer 服务由不同受控权限保护，旧 writer
不能拥有其管理员权限或原始密钥。基础设施控制面确认与旧凭据实际签写拒绝结果共同作为
隔离证据；“节点失联”“磁盘暂时不可达”或操作员声明本身都不是证据。
恢复所需 endpoint/writer key 必须可从受保护、独立于失效域的备份恢复到服务，不能因
key 丢失生成替代 key 来假冒旧身份；无法安全恢复则继续停写。

| 阶段 | 执行主体、持久动作与允许的后继 |
| --- | --- |
| `fenced` | Ops 完成真实隔离；恢复控制器读取全部可用持久前缀、外部锚与未决闭包。任何已确认 tail 无法证明完整时保持 `recovery_required`，不能选择较短 head。 |
| `plan_persisted` | 两端接收器和 signer 保存同一 RecoveryPlan 后进入仅恢复模式。只允许计划内原 identity/continuation/position，拒绝新 Submit、替换 payload、新 nonce 或其他旧 epoch core。 |
| `draining` | 隔离恢复器获得 signer 服务的计划限定访问，原 writer key 仍只签原决定；接收器同时校验 allowlist、parent、原 continuation 和两端 prepare。先补已持久决定/qualified/publication，再按恢复 v1 重建原 candidate。 |
| `drained` | 所有不可撤回原决定、result/nonce/outbox 已持久收敛并发布；固定唯一 cutover head C 与根。原未决定 pending 及其 nonce/去重关联完整冻结，形成私有交接集合和 `pending_handoff_root`。两端及 signer 校验无其余未决 continuation，持久绑定计划、C、该 root 与新绑定 digest。 |
| `sealing` | 两端及 signer 分别持久 `EpochSealV1(recovery_id,E,E+1,C,pending_handoff_root,new_bindings_digest)`；每个组件从本地 seal 成功起永久拒绝 E 签写，同时禁止普通 E+1 execution。首个 seal 后不能退回 draining 或改 C、pending 集合、新绑定。 |
| `epoch_activation` | 预授权 recovery issuer 签新 epoch activation，绑定先前 activation、C、`pending_handoff_root`、全部 seal 回执与新角色 keys。新绑定指定且满足独立持久域要求的 primary/replica 先复制并核对 C 的完整历史、九角色恢复闭包、外部最低锚、同一计划/全部 seal 及私有冻结集合；新两端完成本地持久接收后才可签该 activation 的 prepare。新 writer signer 先持久同一计划与 seals，只允许承载该证书的唯一控制记录：全局下一 position、E+1 sequence 1、相同业务 state root。其正式 proof/record 在新两端持久后才激活 E+1；旧实例不得因轮换 key 恢复签写权限。 |
| `active_next` | 新 ActiveAuthority 及当前 head/工件/双端条件全部通过才开放写入；原未决定 pending 按当前权限和 manifest 重审。旧 E 永久只可查历史。 |

受限恢复器不拥有原 key 的任意签名接口。signer 与两个存储接收器均从已持久计划生成
allowlist，只接受原 continuation 推导的 core 或已经保存的决定；新结果仍须按原闭包
确定性核验。恢复 issuer 的签名不能替代上述检查，也不能自己为旧 epoch 创建新业务请求。

待交接 pending 集合保留原签名请求、原 identity/digest、状态及 nonce/去重关联，按原
identity 的 canonical 编码排序，在 `pending_handoff` 域计算 `pending_handoff_root`；
空集合也有唯一显式 root，缺集合不能解释为空。该 root 同时进入 seal 与新 activation 的
签名 body，覆盖尚未进入 C 的 canonical execution 的已接受请求。新两端必须按 root
逐项验证并持久保存完整私有集合；缺失、冲突或较短 tail 均阻断 activation prepare。
公开 proof 只带该 digest，不能通过 proof 入口取得私有 pending；接管不重签请求或重占 nonce。

控制面只限制 key 的调用主体，未提前废除旧 epoch 的合法签名，所以 `draining` 能完成
原决定；seal 后的新 epoch activation 由创世预授权证书闭合，不再要求已经退役的旧 key
签新记录。新 activation 沿 C 接续同一历史，不执行游戏动作或制造新资产。

#### 单域丢失、半完成 epoch 与旧盘

单域失效时先硬隔离失效角色的旧实例，再从幸存端完整 qualified 前缀、可确认 head、
未决 continuation/decision 与外部最低锚恢复；不能只取 snapshot。
若排空需要缺失 endpoint 的回执，先以原 endpoint ID/key 恢复其受控服务并复制完整闭包，
两端核对后才进入原 E 的受限 draining。角色换 key/端点的正式生效只能发生在 E+1；
同身份恢复旧服务也必须证明旧实例无法再签写，不能并行运行两个持有原 key 的实例。

计划或 seal 只写到一端后崩溃，不把它视为取消：重启合并可验证的单调状态，只能完成
同一 recovery_id/新绑定；C 一旦在 drained 阶段固定也不可改变。尚未 sealed 的端点不得
独自放行 E 或 E+1；任何一份已持久
seal 都要求重建另一端后续封，不能回滚本地 max epoch。所有端点在正常服务前先核对
现行计划/seal/activation 与独立最低锚，旧盘的较小 epoch/head 只可作历史材料，不能成为
writer、signer 或最高 head 的来源。
若现存材料无法判断是否曾封 epoch、是否存在更高可确认 tail 或旧写权限是否已隔离，
保持 verified read-only / `recovery_required`；不以超时推断失败或重开旧 epoch。

恢复发现已经正式提交的 BFT handoff H 时，拒绝创建或继续 controlled E+1 activation。
即使 H+1 尚无 BFT 提交，也只能由合法新 profile 恢复；本节不撤销 H 或恢复旧单权威。
未开始隔离且未持久计划前可放弃一次接管准备；进入持久恢复流程后按表单调推进，
不把恢复失败当作旧机器重新上线的授权。

<a id="des-p2p-fresh-preview-bootstrap"></a>
### 新预览世界初始化

`initialize-preview-world` 是普通新建 DevLocal 技术预览的显式初始化入口。
它消费操作员固定的 world/node ID、已有受保护 node key directory、全新绝对 root、
loopback endpoint 与 genesis 初始化 nonce；nonce 不选择模拟世界 seed，也不是授权凭据。
已有世界的身份、历史或部分目录不能被该命令覆盖、修复或重新创世。

创世摘要使用版本化域分离编码，绑定真实保留的初始状态/日志字节、初始化参数和节点派生
服务公钥。初始化使用现有 canonical Runtime preset、实际 step 与原 NodeRuntime/execution
driver，生成可验证 CAS、提交记录和 world proof；不写入测试能力、虚构资金、占位提交或
人为 finality。cognition 的 pending binding 和本地提交只证明该预览的初始化材料。

输出在同文件系统的 0700 sibling staging 构建，文件和目录同步后通过 OS 原子 no-replace
发布。receipt 固定初始 publication manifest 的摘要；manifest 绑定原始 execution record、
节点签名 envelope 及 snapshot、journal、world-head proof 的确切 CAS 字节，使初始证明在
历史 CAS 回收后仍可验证。目标已存在时核对原 receipt、创世原字节、identity、实际 signer、
归档初始证明与当前 live CAS/record/proof；缺失、冲突、篡改或非受保护路径立即拒绝。旧根若
没有该认证归档，只能从已验证备份恢复，初始化器不重建、不重签、不改写旧证明；幂等查询也
不因后续真实提交推进而把原初始锚改写成当前 head。

私钥只保留在受保护配置中。公开初始化 receipt 给出 identity、材料承诺和 derived
feedback-submit service public key，不提供 node seed 或消费者签名私钥。操作员通过独立
认证的部署通道核对该 pin 后，将 endpoint/world/genesis/service key 和独立 public-read
配置交给 Viewer；读取签名不授予 Agent、operator、validator 或治理权限。

这是新建隔离预览的 provenance 和初始化合同，不替代上文正式创世授权、profile 激活、
历史恢复链或独立故障域耐久性。跨平台输出需核对相关 schema/hash/persistence 等价并在
目标机器实际打开和验证 pin。使用与负例验证见 [初始化手册](../testing/manual/fresh-world-preview-bootstrap.manual.md)。

## 5. 关键运行流程

先校验提交身份/大小/队列→固定有序actions/root→同parent/manifest执行并匹配receipt commitments→fenced epoch/parent/head 下原子预备追加及跨故障域持久确认→合法 profile 生成并验证提交决定/证明→最终决定/证明跨故障域持久确认→消费者committed读取。BFT 的独立重执行与投票步骤继续适用其专属合同；单权威不通过等待尚未生成的证明制造循环依赖。当前prototype推进与目标QC闸门不是同一实现证明。恢复先证明原world历史链再判只读；当前追加/finality/manifest/head全满足才重新serve/vote，失效回只读/隔离。

拒绝保留可定位原因；timeout/retry受原策略有界限制。并发冲突遵循原guard/precondition/共识authority，不能因接线或重连获得新权限。取消/替代只有外部专业合同明确支持时成立，本设计不新建取消或补偿schema。

## 6. 接口与数据合同

consensus→node交付有序payload/root与commit上下文；runtime→validator提供receipt/parent/manifest执行语义；DistFS/服务→恢复者供应hash-bound材料，不能自授authority。身份/版本/顺序不匹配拒绝，不接受latest任意backup。wire/schema演进仍属原专业authority；本次文档未宣称已有字段或实现支持。

| producer → consumer | 输入身份/版本 | 顺序与幂等 | success / error | 兼容与资源边界 |
| --- | --- | --- | --- | --- |
| consensus/runtime/DistFS → node/consumer | 原PRD列明identity/hash/version，不复制schema | 上述flow及DES验证顺序；local幂等不推导global exactly-once | 完成相应validation/apply/persist/publish后才声明该结果；失败不伪进展 | 原policy/defaults及旧data兼容；不加未知deadline/阈值 |

## 7. 状态、事务与持久化

pending/candidate无世界效果；verified commit才推进权威高度。node block/action/execution roots、journal、snapshot作为同一绑定。提交证明与持久恢复按各高度合法 profile 成立；`bft` 另要求目标 lock/round/validator-set proof 和 certificate，当前缺这些，不以旧TickCertificate补缺。恢复只读≠恢复可服务；闸门回退不改历史receipt。

accepted、applied、persisted、published必须分别具备各自证据；workflow done/文档active不属于运行状态。外部receipt/journal/state root生命周期和recovery由runtime authority；本地观察不能提供更强保证。

## 8. 部署、安全与运行约束

target protected validators + public services；governed registry有效epoch/stake/signer唯一投票真值。公网IP非通用前提。独立local/dev world永不并入global。prune前先证明重建/hash/root与冗余archives；service compromise只能隔离/缺证。

设计只定义受控输入和失败条件；操作留在现行runbook。当前governed live网络raw-copy禁令和signed-V2边界只适用于该runbook环境，不否定历史/offline local事实。任何真实fleet/health/restore claim需固定inventory/manifest/package/config与同窗口证据，不从triad标签或本地成功推导。

## 9. 质量与容量

| 环境/刺激 | 预期响应 | 判定指标/阈值来源 | 验证入口 | 当前证明范围 |
| --- | --- | --- | --- | --- |
| 同冻结候选的local unit/fixture，逐DES负例与旧数据兼容 | 精确拒绝/no false mutation；正确路径满足原顺序 | 配对PRD常量/default/policy；未定义容量/latency不新增数字 | §11.1逐场景的source/手册 | source定义/计划，SN1未执行 |
| 多节点/恢复/服务角色组合，固定world/version/config/window | 同history/root或明确隔离/只读，不产生无证世界效果 | root与testing-manual S9A；root历史lag≤50、DistFS ratio≤0.1、mismatch0及insufficient_data阻断口径保留 | [testing-manual](../../testing-manual.md) S9A/S9/S10；按原场景选择tier | 无真实规模、窗口或容量/SLA执行证明 |

## 10. 兼容、迁移与回滚

保留旧tick记录仅snapshot/replay/diagnostic兼容；不能推进finality。dormant path-index/empty runtime_bridge不复活；production WASM只external canonical binary+receipt，source compile仅显式dev/test。协议/manifest升级和历史replay由runtime authority管辖，不能用当前manifest改历史。

本文保留原path/旧章节anchors与历史ID，不替换task truth；添加的接受/DES只细化旧合同。新输入、消费者或语义变更需重新绑定/专业审读。package/config回退不能复原已删chain state或撤回committed效果；破坏性操作依原runbook与runtime恢复合同。

## 11. 验证设计与可追溯性

下表是可复用计划：existing test source、planned场景与executed evidence分开。每条只消费准确upstream/DES；执行时须同可比较candidate/config/入口/环境，local fixture不升级为真实网络。未定位完整测试写planned，owner为runtime/QA；缺证阻断该组合结论，不能从required局部降低root full。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [身份与 profile 边界](prd.md#p2p-authority-profiles) | [新预览世界初始化](#des-p2p-fresh-preview-bootstrap) | 显式新世界材料与 signer/consumer 一致；不可覆盖、部分成功或工程材料升格 | [初始化手册](../testing/manual/fresh-world-preview-bootstrap.manual.md)；同候选实际初始化、原 receipt 重验、状态推进、身份/nonce/key 冲突、CAS/证明篡改、权限/symlink、双初始化及带外目标竞争，部署机器 signed read 与重启 | 当次交付 Issue 的原 source/binary、真实持久材料与拒绝/接收结果；不在设计保存执行过程 | 未证明正式 activation/finality、分布式共识、跨故障域、生产 mint 或完整玩家旅程 |
| [professional_acceptance: p2p-authority-profiles](prd.md#p2p-authority-profiles) | [des-p2p-authority-profiles](#des-p2p-authority-profiles) | 固定身份与合法 profile 激活，工程证据不授予正式权威 | [testing-manual.md](../../testing-manual.md) S9A；planned 同候选 profile/identity 错配、无激活证明、工程 scope 升格负例与合法转换正例 | 同 world/profile 候选、激活材料与拒绝/接收结果 | profile 原则验证不证明正式入口实现、真实域或 BFT |
| [professional_acceptance: p2p-single-authority-durable-commit](prd.md#p2p-single-authority-durable-commit) | [本机日志前置](#des-p2p-local-authority-journal-prerequisite) | 仅固定身份/epoch的本机原子记录、去重与恢复查询；不授予正式 committed | [testing-manual.md](../../testing-manual.md) S9A；`cargo test -p oasis7_distfs controlled_authority`；同候选测试竞争写者、stale epoch/parent、请求碰撞、各持久阶段故障、跨世界/proof篡改、带外最低anchor截断拒绝；Unix持久行为与不支持平台拒绝分别判定 | 当次 GitHub Issue #4363 候选/命令/exit/log；只有实际执行结果可记录通过 | 未提供独立副本确认、跨主机 fencing、完整 runtime prepare/commit 接线或 BFT/激活证明 |
| [professional_acceptance: p2p-single-authority-durable-commit](prd.md#p2p-single-authority-durable-commit) | [双端协议前置](#des-p2p-replicated-durability-prerequisite) | 双端确认与未知结果恢复维持原请求和单调 head | [testing-manual.md](../../testing-manual.md) S9A；`cargo test -p oasis7_distfs replicated_protocol` 为现有本机协议基线；逐持久阶段中断、单端不可用、重复原请求与错误 receipt/head；真实独立域验证仍 planned | 双端持久阶段、原请求/回执与恢复 head；同候选实际命令结果 | 本地目录不证明跨主机 fencing、正式激活或真实故障域 |
| [professional_acceptance: p2p-authority-handoff](prd.md#p2p-authority-handoff) | [des-p2p-authority-profiles](#des-p2p-authority-profiles) | H/H+1 唯一 profile 与提交后不可撤回 | [testing-manual.md](../../testing-manual.md) S9A；planned H 未提交、H 已提交及 H+1 尚未提交/已提交四个窗口，尝试旧 profile 追加、回滚和双权威激活 | H/H+1 证据链、选定 profile、拒绝结果与零旧追加 | 不证明完整 BFT 或实际治理交接已执行 |
| [professional_acceptance: p2p-ordered-execution](prd.md#p2p-ordered-execution) | [des-p2p-ordered-commit](#des-p2p-ordered-commit) | 下一committed height、有序序列/root/decode及journal绑定；错误不推进 | [testing-manual.md](../../testing-manual.md)；S9A phase2/3；同candidate重排、tamper、decode/root/journal负例；执行receipt/状态变化oracle。现有手册入口，完整组合未运行。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-receipt-finality-boundary](prd.md#p2p-receipt-finality-boundary) | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | receipt≠QC；同parent/manifest/actions独立重执行，缺artifact/fault/root拒绝vote/commit | [testing-manual.md](../../testing-manual.md)；S9A；全部活动validator+runtime相同输入/父状态/版本，逐一根不匹配及旧threshold1证据负例。target完整矩阵planned。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-target-bft](prd.md#p2p-target-bft) | [des-p2p-target-bft](#des-p2p-target-bft) | Propose/Prevote/Precommit仅verified >2/3 active stake cert生效；world/height/round/phase/roots/set和dedup签名绑定 | [testing-manual.md](../../testing-manual.md)；S9A full target场景：错误签名/阈值/集合/round、缺证/equivocation拒绝；锁定/解锁、timeout/new-round、transition、partition/heal/restart；prototype不能证明。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-target-recovery-trust-chain](prd.md#p2p-target-recovery-trust-chain) | [des-p2p-target-recovery](#des-p2p-target-recovery) | manifest/genesis→cert/header+set transition→hash snapshot→canonical replay→root→serve/vote；每环缺失/冲突/回退/异world停止 | [testing-manual.md](../../testing-manual.md)；S9A state-sync/full restore：逐环负例、snapshot/replay/state-sync/pruning/drill；同world+samecandidate+同窗口材料，readonly历史与writable闸门分判。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-target-role-isolation](prd.md#p2p-target-role-isolation) | [des-p2p-layer-authority](#des-p2p-layer-authority) | 治理validator与permissionless服务、light/archive角色隔离；验证材料而非operator身份；prune重建+archive条件 | [testing-manual.md](../../testing-manual.md)；S9A full：compromised服务/错误角色签名不能推进，archive失联禁止未证明prune；Ops同窗口inventory/role与runtime验证oracle。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-service-gate](prd.md#p2p-service-gate) | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | verified-serving→stale/catching-up→readonly→serving；proof冲突→isolated；非serving新intent receipt0、历史不改 | [testing-manual.md](../../testing-manual.md)；S9A + GWSC full同candidate：当前manifest/head/finality逐项负例、三恢复状态+gate rollback；receipt0/1与consumer blocker/next-step必须实际linked附件，现模板不足。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-replication-recovery](prd.md#p2p-replication-recovery) | [des-p2p-replication-scope](#des-p2p-replication-scope) | writer/epoch从seq1及guard不污染；权威恢复错误与qualified storage-challenge fallback区分 | [testing-manual S4/S9A](../../testing-manual.md)；exact local test source `crates/oasis7_node/src/tests_fetch_blob_chunking.rs`；existing fetch_blob_storage_challenge_empty_provider_routes_probe_bounded_generic_route、fetch_blob_storage_challenge_without_provider_lookup_does_not_probe_generic_route；node局部fixture，不是DHT fetch fallback或QC证明。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[product_requirement: req-dcs-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001) | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | 验证certificate及hash-bound材料，拒绝服务/cache/peer自授authority；runtime state apply仍外部 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[product_requirement: ac-dcs-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | 验证certificate及hash-bound材料，拒绝服务/cache/peer自授authority；runtime state apply仍外部 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: req-dcs-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) | [des-p2p-target-recovery](#des-p2p-target-recovery) | sameworld恢复链每环验证；历史只读与重新服务闸门分开 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: ac-dcs-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | [des-p2p-target-recovery](#des-p2p-target-recovery) | sameworld恢复链每环验证；历史只读与重新服务闸门分开 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[product_requirement: req-dcs-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) | [des-p2p-layer-authority](#des-p2p-layer-authority) | governed validator与permissionless服务隔离；公网IP非通用前提 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[product_requirement: ac-dcs-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-003) | [des-p2p-layer-authority](#des-p2p-layer-authority) | governed validator与permissionless服务隔离；公网IP非通用前提 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: req-dcs-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004) | [des-p2p-target-bft](#des-p2p-target-bft) | verified >2/3 active stake证书才推进；wrongset/equivocation/round缺证拒绝 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: ac-dcs-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-004) | [des-p2p-target-bft](#des-p2p-target-bft) | verified >2/3 active stake证书才推进；wrongset/equivocation/round缺证拒绝 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[product_requirement: req-dcs-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | 服务转换及manifest/head/finality conjunction；非serving intent receipt0 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[product_requirement: ac-dcs-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | 服务转换及manifest/head/finality conjunction；非serving intent receipt0 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[product_requirement: req-dwe-001](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | 同parent/version/order重执行及root绑定；runtime原子apply外部 | [testing-manual S9A](../../testing-manual.md)；对应DWE同candidate跨runtime/P2P/消费者replay/outage/lineage/activation负例计划；DWE004 full，其余局部required不代根组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[product_requirement: ac-dwe-001](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-001) | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | 同parent/version/order重执行及root绑定；runtime原子apply外部 | [testing-manual S9A](../../testing-manual.md)；对应DWE同candidate跨runtime/P2P/消费者replay/outage/lineage/activation负例计划；DWE004 full，其余局部required不代根组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[product_requirement: req-dwe-002](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002) | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | finality不可用保持无效果pending；恢复按当前条件重新裁决，consumer状态外部 | [testing-manual S9A](../../testing-manual.md)；对应DWE同candidate跨runtime/P2P/消费者replay/outage/lineage/activation负例计划；DWE004 full，其余局部required不代根组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[product_requirement: ac-dwe-002](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-002) | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | finality不可用保持无效果pending；恢复按当前条件重新裁决，consumer状态外部 | [testing-manual S9A](../../testing-manual.md)；对应DWE同candidate跨runtime/P2P/消费者replay/outage/lineage/activation负例计划；DWE004 full，其余局部required不代根组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[product_requirement: req-dwe-003](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-003) | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | receipt无第二效果为交接条件；互斥lineage首有效receipt唯一胜者/终止其余由runtime拥有，拒绝过期只终止自身，独立intent并发 | [testing-manual S9A](../../testing-manual.md)；对应DWE同candidate跨runtime/P2P/消费者replay/outage/lineage/activation负例计划；DWE004 full，其余局部required不代根组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[product_requirement: ac-dwe-003](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003) | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | receipt无第二效果为交接条件；互斥lineage首有效receipt唯一胜者/终止其余由runtime拥有，拒绝过期只终止自身，独立intent并发 | [testing-manual S9A](../../testing-manual.md)；对应DWE同candidate跨runtime/P2P/消费者replay/outage/lineage/activation负例计划；DWE004 full，其余局部required不代根组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[product_requirement: req-dwe-004](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | 首次committed/finality-verified canonical block与activation选version；compat declaration不选规则，历史receipt原manifest replay，缺证failclosed；runtime/WASM详细设计外部 | [testing-manual S9A](../../testing-manual.md)；对应DWE同candidate跨runtime/P2P/消费者replay/outage/lineage/activation负例计划；DWE004 full，其余局部required不代根组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | external durability/lineage/activation/消费者设计及证据未闭合 |
| Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[product_requirement: ac-dwe-004](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-004) | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | 首次committed/finality-verified canonical block与activation选version；compat declaration不选规则，历史receipt原manifest replay，缺证failclosed；runtime/WASM详细设计外部 | [testing-manual S9A](../../testing-manual.md)；对应DWE同candidate跨runtime/P2P/消费者replay/outage/lineage/activation负例计划；DWE004 full，其余局部required不代根组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | external durability/lineage/activation/消费者设计及证据未闭合 |
| [单权威提交](prd.md#p2p-single-authority-durable-commit) | [原 candidate 恢复](#des-p2p-original-candidate-recovery-v1) | 未开始、执行中、完整 candidate、已决定、qualified 与回应丢失均收敛原身份一次 | CA-R1，planned crash integration：在表中各阶段切断进程，和无故障同输入 oracle 比较全部九角色 roots、原结果与 nonce；LLM 调用及 outbox 新派发不得因重建增加；实施入口承接 [S9A manual](../../testing-manual.md) | 原 identity、故障 seam、恢复阶段、全 roots 与效果计数 | 现有 `controlled_live` 不覆盖 candidate 未完整窗口；不是执行通过 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[AC-DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | [原 candidate 恢复](#des-p2p-original-candidate-recovery-v1) | 缺失或冲突材料不能产生替代决定 | CA-R2，planned negative integration：逐项删改原 parent、native/WASM、time/randomness、nonce、prepared root 或更高 head；应保留原请求并 recovery_required；补回精确原材料后才能续办；实施入口承接 [S9A manual](../../testing-manual.md) | 故障输入、阻断原因、零新增发布及续办结果 | 不证明任意旧 native 版本可移植；缺工件保持阻断 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[AC-DWE-002](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-002) | [原 candidate 恢复](#des-p2p-original-candidate-recovery-v1) | 未决定 pending 当前重审；固定决定保持原语义 | CA-R3，planned Runtime contract：分别在两阶段改变当前权限、期限和 manifest；pending 走新条件，固定 continuation 对原 parent/版本产生相同结果且不新占 nonce；实施入口承接 [S9A manual](../../testing-manual.md) | 两阶段输入、选版、reservation 与结果对照 | 不关闭跨域 lineage 或全部消费者验证 |
| [合法 profiles](prd.md#p2p-authority-profiles) | [正式激活/证明](#des-p2p-formal-controlled-authority-v1) | 初次 activation 与完整双端证明才授予正式权威 | CA-A1，planned codec/unit/integration：golden 字节；自授 issuer、错误身份/roots/keys、递归字段、非 canonical 编码、单回执及 activation 半持久负例；完整链正例；实施入口承接 [S9A manual](../../testing-manual.md) | 证书/codec vectors、双端 proof 与 ActiveAuthority 构造结果 | 签名不证明实际独立故障域或执行正确性 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[AC-DCS-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) | [普通消费](#des-p2p-formal-controlled-authority-v1) | 五路 API 不将工程或未知版本当正式；保持投影权限 | CA-C1，planned WorldService/client conformance：未预装 proof 的新客户端从独立信任锚取得 genesis 至当前 head 的连续公开证明，再逐路执行 wire/evidence 矩阵，检查 Describe/read-only、原 Lookup、私有 projection、Changes 缺 proof/回退；proof 不可用、错 hash/缺页/超限/私有字段泄露均拒绝，公开 plan 签名及私有集合 commitment 对账；旧 v1 必须拒 v2；实施入口承接 [S9A manual](../../testing-manual.md) | 请求关联、签名/proof verdict、原结果/cursor 与零越权读取 | 现有 service-signature verifier 不覆盖正式 proof；不证明玩家完整旅程 |
| [单权威提交](prd.md#p2p-single-authority-durable-commit) | [人工接管](#des-p2p-controlled-epoch-takeover-v1) | 旧机器/旧盘/备用凭据不能绕过存储与 signer | CA-F1，planned adversarial integration 和后续独立域演练：旧实例存活、重挂旧盘、旧 signer/存储凭据实际写入；隔离未证实不得进入 draining，隔离后全部拒绝；实施入口承接 [S9A manual](../../testing-manual.md) | 同窗口控制面证据与实际负例拒绝结果 | 文档和本地目录不证明基础设施隔离；部署尚未执行 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[AC-DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | [人工接管](#des-p2p-controlled-epoch-takeover-v1) | 单域丢失与部分 plan/seal/activation 不回退或丢尾部 | CA-F2，planned crash matrix：每次持久动作后中断，并丢失任一域；同 recovery_id/C/新绑定收敛，原未决决定一次；新绑定端缺 C 历史、闭包、seal 或原 pending 时不得签 activation prepare；缺完整 tail 或隔离证据只能停写；实施入口承接 [S9A manual](../../testing-manual.md) | 前后 head/epoch、冻结请求、seal 回执、新两端接收证明、旧写拒绝和恢复 root | 真实 durability/key backup 仍须受控环境演练 |
| [H/H+1 交接](prd.md#p2p-authority-handoff) | [人工接管](#des-p2p-controlled-epoch-takeover-v1) | H committed 后不得用恢复 issuer 复活 controlled 权威 | CA-B1，planned handoff integration：H 已持久、H+1 未提交时宕机，尝试 controlled drain/epoch activation；全部拒绝新旧 profile 越界；实施入口承接 [S9A manual](../../testing-manual.md) | H proof、恢复请求、拒绝 verdict 与零旧追加 | 不实现或代签完整 BFT 共识验收 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[根 SC-1](../product/world-infrastructure/prd.md#sc-infra-001) | [接收条款](#des-p2p-authority-profiles) | 合法 profile/epoch 与持久提交、BFT/交接组合，非单叶证明 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-FINALITY`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[根 SC-2](../product/world-infrastructure/prd.md#sc-infra-002) | [接收条款](#des-p2p-layer-authority) | 角色权限、复制/证明提供及 pruning 可重建/archive 条件分别成立 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-FINALITY`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[根 SC-2](../product/world-infrastructure/prd.md#sc-infra-002) | [接收条款](#des-p2p-replication-scope) | 角色复制子范围：writer/epoch 从 seq1 的顺序恢复、guard 不污染，权威恢复错误不被 qualified storage-challenge fallback 掩盖；不代签全部角色存储 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-FINALITY`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[根 SC-3](../product/world-infrastructure/prd.md#sc-infra-003) | [接收条款](#des-p2p-target-recovery) | bootstrap/snapshot/replay/state-sync/pruning/灾备信任链各环及失败隔离 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-RECOVERY`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607, O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[根 SC-4](../product/world-infrastructure/prd.md#sc-infra-004) | [接收条款](#des-p2p-receipt-finality) | 两 profile、全部活动验证者、升级/混合版本与原历史重放 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-EXECUTION`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405, O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[根 SC-5](../product/world-infrastructure/prd.md#sc-infra-005) | [接收条款](#des-p2p-target-service-gate) | pending/未知结果区分、有效胜者原子处置、拒绝/过期自身与独立并发 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-EXECUTION / V-CONSUMER`；根 `test_tier_required` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[根 SC-6](../product/world-infrastructure/prd.md#sc-infra-006) | [接收条款](#des-p2p-target-recovery) | 原世界恢复、缺失/冲突/异世界隔离、历史与未 final 请求连续性 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-RECOVERY`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4, O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[根 SC-7](../product/world-infrastructure/prd.md#sc-infra-007) | [接收条款](#des-p2p-target-service-gate) | 同候选同窗口服务三态、回退、receipt 0/1 与独立正向续跑 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-RECOVERY / V-CONSUMER`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-a15a1b8e-8b7f-4c6a-9cb5-2f949730496c；[入口 SC-9 独立义务](../product/player-entry-distribution/prd.md#sc-entry-009) | [原决定恢复](#des-p2p-original-candidate-recovery-v1) | 原结果与会话正交，查询/原身份重试不产生第二效果 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：入口 U1～U5（../product/player-entry-distribution/prd.md#player-session-unknown-outcome-acceptance） 中的原请求/回复丢失/错误归属子范围；原 test_tier_full，每个 primary mode 同候选/世界/主体分别成案 | PR 中原请求、持久结果、故障窗口、查询返回及副作用次数 | 本次未运行；不代签 Agent 委托、读取授权或真实 hosted/player surface |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[根 SC-2](../product/world-infrastructure/prd.md#sc-infra-002) | [已有接收](#des-p2p-target-recovery) | pruning 前的重建、hash/root 与冗余 archive 条件；只接收角色存储组合中的恢复前置 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-FINALITY`；根 test_tier_full 同候选条件，局部层级不代签组合 | PR 独立记录前置、故障/续跑条件、原始结果和未执行项 | 本次未运行；产品与专业原剩余边界继续有效 |

#### val-p2p-controlled-authority-v1

以下 CA 场景全部为新目标验证计划，尚无实现通过或真实故障域证据。
现有 `controlled_live`、`replicated_protocol`、`controlled_history` 回归仅提供故障接缝与原
工程拒绝行为的基线，不覆盖新 codec、formal activation、普通 v2 消费或跨主机 fencing。
实施时由 Runtime/P2P/QA 绑定同 source/integration/tested tree、world/config、执行工件和
环境窗口；单机故障注入与实际独立域演练分别出证。实际部署 hold 不因文档验收解除。

上述 §11.1 主表的 CA-R1/R2/R3、CA-A1、CA-C1、CA-F1/F2 与 CA-B1
分别承接本节八项目标关系；本锚点仅说明验证范围，不另建第二套映射。

## 12. 决策、长期风险与未决问题

选择保留现有专业合同并精确分配，拒绝以导航/历史标签免除真实设计，也拒绝将local证明升级为full。目标锁定/解锁、anti-double-sign、timeout/new-round、set transition与partition/heal/restart需独立对抗性证明；本文给拒绝条件而不发明新lock算法/schema。pending/lineage/manifest/consumer字段及DC5 structured attachment仍外部未闭合。

后继 controlled v1 明确选择执行前双端闭包和原决定重建，代价是额外持久写入、原 native
工件保留与单世界未决执行串行；不选择重开原请求、采用当前版本或依赖外部 provider 重试。
人工接管选择真实隔离加受限恢复 signer，代价是失联或隔离不可证明时牺牲写入可用性。
正式 proof 和 WorldService v2 是兼容边界，不保留通过删除工程标签升级的捷径。
独立评审必须核查确定性闭包、签名域/编码、恢复 signer 权限与半完成 epoch；其后仍需实现
和 CA 矩阵证据，本文不提供安全、运行或发布通过结论。

runtime/P2P负责缺失技术/实现；ops负责环境事实；QA负责计划覆盖和verdict；producer审读产品含义。新算法/schema、权限/默认值变化、额外消费者或claim扩展触发重新评审；未解除项保留在GitHub正式evidence及剩余required set，不能靠未来Issue占位关闭。下列原章节的里程碑/审计轮次是保留provenance，不维护可变进度。



审计轮次: 7

- 对应需求文档: `doc/p2p/prd.md`
- 对应任务管理: GitHub Issue / GitHub Project
- 对应文件级索引: `doc/p2p/prd.index.md`

## 1. 设计定位
`p2p` 模块的 `design.md` 负责描述链上大世界状态底座的总体设计入口。该底座由网络传输、共识、分布式存储、状态同步、执行记录、observer/ops 与 projection/API 等子层共同组成；P2P transport 是其中的网络层证据，不单独代表大世界状态闭环。

## 2. 阅读顺序
1. `doc/p2p/prd.md`
2. `doc/p2p/design.md`
3. GitHub Issue / GitHub Project（任务状态与历史）
4. `doc/p2p/prd.index.md`
5. 需要验证模块级闭环时进入 `testing-manual.md#s9a链上大世界状态底座自闭环`
6. 下钻 `blockchain/`、`consensus/`、`distfs/`、`network/`、`node/` 等专题目录

## 3. 设计结构
- 网络层：节点发现、连接、同步、传输与非全公网 reachability 边界。
- 共识层：身份、投票、finality、状态传播与一致性策略。
- 存储层：DistFS、blob closure、路径索引、复制与恢复。
- 状态同步层：replication、gap sync、state sync、checkpoint 与 peer-head freshness。
- 节点执行层：execution record/receipt、奖励、执行校验与治理对接。
- 观测与投影层：observer/ops 证据、API/viewer projection 与 claim boundary。

历史 `distributed-*` 专题的稳定设计已归入上述分层：`oasis7_net` 拥有 transport/peer/DHT，`oasis7_consensus` 拥有 proposer/attestation/action-root，`oasis7_distfs` 拥有 CAS/replication/challenge，node/chain-runtime 拥有 commit 后执行与恢复接线，`oasis7_proto` 只拥有共享 wire 类型。`oasis7_wasm_abi` 是 `ModuleManifest` 与 runtime ABI 类型的单一来源，不得在 net/proto/viewer 再定义副本。

当前设计不承诺历史 `/aw/rr/*`、固定 bitswap/graphsync、每次 zone commit 的 lease 接管、viewer 默认执行 bridge 或跨节点 DistFS challenge topic/envelope。production WASM 只消费外部 canonical binary + receipt；本地 source compile fallback 仅可作为显式 dev/test 兼容，不是节点生产路径。

早期 net path-index/Observer 的 `execution_storage.rs`、`head_follow.rs`、`head_validation.rs`、`observer.rs`、`observer_metrics.rs`、`observer_replay.rs` 与 `bootstrap.rs` 均已从当前 tracked source tree 删除，仅可从 Git history 追溯；本页及专题文档保留其历史约束。`runtime_bridge` 只是空兼容 feature；历史实现、文档合同或 feature 可编译都不代表当前恢复、同步或 bridge 能力。

## 4. 集成点
- `doc/world-runtime/prd.md`
- `doc/headless-runtime/prd.md`
- `doc/testing/prd.md`

## 5. 专题导航
- 基础链路进入 `network/`、`consensus/`
- mixed-topology / 非全公网主链级覆盖网络进入 `network/mainnet-private-reachability-architecture.*`
- 数据与复制进入 `distfs/`
- 节点执行与奖励进入 `node/`
- 区块链和生产化扩展进入 `blockchain/`；分布式运行时总体边界由本页与根 PRD 承载，`distributed/README.md` 仅作 successor 导航
- 模块级自闭环、claim level 与测试层级进入 `testing-manual.md#s9a链上大世界状态底座自闭环`

## 设计目标
- 提供链上大世界状态底座的总体设计入口，并把历史 `P2P 基础设施` 口径收束为底座内网络/存储/同步等组件证据。

## 设计范围
- 覆盖模块级结构、主链路、分层与专题导航。
- 不替代专题 `*.design.md` 的细化设计。
- 不把单一 transport、storage、observer 或 node 专题的 green 结果提升为 S9A `module_full`、`integration_required` 或 `release_full`。

## 关键接口 / 入口
- 需求入口：`doc/p2p/prd.md`
- 执行入口：GitHub Issue / GitHub Project
- 索引入口：`doc/p2p/prd.index.md`

## 设计演进计划
- 历史建档（2026-03-09，ROUND-006）：模块级 `design.md` 入口来源；原 Task 未唯一定位，记录保留，不作为当前排期。
- 持续结构要求：高复杂度专题在对应 `*.design.md` 维护细化合同；不恢复旧 M2 任务。

## 设计风险
- 若专题级设计未及时补齐，模块级 `design.md` 可能承载过多导航职责。
- 若 legacy redirect 未明确标注为兼容跳转，读者可能误判历史入口为当前执行入口。

## Ordinary WorldService restore reuse

Ordinary immutable-record reads may reuse a successful detached `World::from_snapshot` result for exactly the same verified Snapshot and complete Journal bytes. The process-local key contains domain-separated BLAKE3 digests and both byte lengths. The retained original has no persistence directory or receipt signer; consumers receive independent World clones. Failed restores are never retained. The FIFO keeps at most two entries, with an 8 MiB source-byte limit per entry and 16 MiB total retained-input-byte weight. These serialized input lengths are a retention proxy, not an exact heap cap; World clones also allocate transient memory. Oversized inputs and a poisoned cache use the original restore path.

The cache does not select or authenticate a head. Every request still loads its actual record, reads and hashes all three CAS artifacts, decodes Snapshot/Journal/proof, checks their complete root/boundary/record/proof bindings, and derives the current runtime binding and exact requested CommitRef. Deleted or corrupt records/CAS objects, GC, conflicting fixed references, and changed heads cannot be satisfied by an earlier cache entry. Authorization and projection run after pinning under their existing checks. Guarded prerequisite reads retain their separate complete history verification and captured publication fence; they do not use this cache. No wire, assurance scope, startup gate or deployment behavior changes.

Concurrent cold reads of an admitted identical key share one in-flight restore. Each caller completes the same record/CAS/proof checks before joining and receives an independent clone of the successful detached original. At most two active keys and 16 MiB of active source-byte weight are admitted separately from the retained FIFO budget; an existing key remains joinable when admission is full. Distinct keys at saturation and oversized inputs bypass coalescing and restore normally, so this is not a bound on all concurrent restores. Decoded inputs, waiting callers, transient result Arcs and World clones are outside those source-weight budgets. The global cache mutex protects only selection and bookkeeping; expensive restore and cloning occur outside it. Attached waiters may share their leader's exact error, but no negative result remains cached. Leader cleanup removes only its own marker and wakes waiters on error or unwind; abandoned work is retried iteratively. Poisoned cache, flight or retained-original locks fall back to the original restore path.

### 工程节点配置入口

工程启动使用 `oasis7_chain_runtime dev node --config PATH`。配置合同为
`oasis7.development-node/v1`，包含绝对 `working_directory`、强类型 `runtime`
及可选 `local_provider`；所有层级拒绝未知字段。`dev local-provider --config PATH`
必须提供完整 provider 材料，且仅允许 `DevLocal`。托管 mock funding lane 仍要求
明确的 hosted session、test-tier 构建与 standalone 模式。配置不接受 manifest、
原始 argv 或正式 authority。synthetic auto-attest 仅属于工程计数，不能作为正式共识验收。

已有工程 wrapper 可通过 `dev prepare-config --output PATH -- source-options`
先验证并生成同一合同，再启动明确的 dev 命令；失败时不启动节点。相对材料和存储路径
在准备时按原 cwd 固定。launcher、S9/S10 和 longrun 的实际启动采用该流程。
网络 manifest 启动拒绝 synthetic auto-attest，并从已核验 World 的治理 registry 获取
validator signer；缺远端 signer 时拒绝启动，不为它派生本机 root 公钥。

离线核验入口为 `inspect bootstrap`、`inspect history`、`inspect rebuild-proof`，
参数仍由原有 verifier 核验；旧 `verify-*` 拼写仅为兼容别名。help 和非法参数解析
不启动节点，也不创建世界或身份材料。
