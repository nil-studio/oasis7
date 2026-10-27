# p2p 模块设计总览

## 1. 问题、目标与非目标

同一 world 的共识顺序、可验证材料与执行/恢复共同形成权威底座；解决 receipt、transport 或历史恢复被误当最终性/可服务的问题。

Owner role：runtime_engineer；canonical repository：eng-cc/oasis7；历史审读基线 f9d5a552d9af04c1b1398262808198a58e560230，2026-09-26。本设计消费下表有限关系；旧章节保留兼容合同，后继规范明确其适用 profile、算法和 schema，不改变已部署默认值。任务实际source/integration/tested tree/config/environment/window/结果由GitHub Issue/Project evidence维护，本文不伪造候选/通过结果。

2026-10-12 以 e01e29064517f4bf375767a19a09802335828249 补充的
`original-candidate-recovery-v1`、`formal-controlled-authority-v1` 与
`controlled-epoch-takeover-v1` 是后继目标协议，明确新增目标类型和状态机。
它们是正式规范；当前已有 native formal、受保护 signer 接口、原输入恢复、有界只读 full-sync 子范围，以及显式固定集合 V1 与动态集合 V2 的 engineering BFT NodeRuntime 实现，详见[持久节点手册](blockchain/persistent-world-node-lifecycle.runbook.md#正式合同层与准入边界)。文中的早期 planned 描述只保留其当时证据范围；不能整体推断当前无实现，也不能由本机接线推断真实故障域或全部消费者通过。V2 已接入治理排程、h/h+1/h+2 集合边界与受保护 provider 安装确认；正式 BFT profile、长期 checkpoint 接续及真实 custody 仍待实现和验收，既有 prerequisite 的拒绝边界及部署 hold 不变。

非目标：不以文档active/合并、local fixture、历史MIG或CLI可编译宣称正式 BFT、部署、SLA或release。剩余215-object required set及root SC1..10/full组合不变。

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
| [BFT 证书闸门](prd.md#p2p-target-bft) | 固定高度的 Tendermint 锁、validRound、nil、超时和严格权重阈值 | [轮次与锁定](#des-p2p-bft-round-lock-v1) | P2P 状态机；Runtime valid(v)；QA 模型/对抗验证 | 固定集合 engineering 路径已实现；不证明正式 BFT readiness |
| [BFT 证书闸门](prd.md#p2p-target-bft) | 对外签名前持久决定；恢复与复制实例不重签冲突字节 | [签名与恢复](#des-p2p-bft-signing-recovery-v1) | P2P WAL；受保护 signer；ops custody/反回滚；QA 崩溃验证 | 本机 WAL 覆盖进程崩溃，不证明跨机 signer 安全或回滚抵抗 |
| [BFT 证书闸门](prd.md#p2p-target-bft) | h 最终提交更新、h+2 固定激活，治理有效与本机安装分离 | [集合转换](#des-p2p-bft-validator-transition-v1) | 治理认证；P2P 高度/集合；Runtime install ACK；QA 转换 | 当前不支持安全 runtime 更新，不因 registry 查询成功授予投票 |
| [professional_acceptance: p2p-target-recovery-trust-chain](prd.md#p2p-target-recovery-trust-chain) | manifest/genesis→cert/header+set transition→hash snapshot→canonical replay→root→serve/vote；每环缺失/冲突/回退/异world停止 | [des-p2p-target-recovery](#des-p2p-target-recovery) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [完整同步只读资格](prd.md#p2p-full-sync-read-only) | 外部 C0、minimum、同 cut 和完整闭包验证后才发布只读 head | [完整状态同步](#des-p2p-full-sync-read-only-v1) | P2P proof/CAS；安全治理读权；Runtime snapshot；QA 角色拒绝 | 已有有界实现；不含 writer/validator 或完整部署资格 |
| [长期同步容量](prd.md#p2p-full-sync-capacity-checkpoint) | 容量限制可见；checkpoint 接续保留 C0 与受保护 minimum，GC 前证明重建 | [checkpoint 与容量](#des-p2p-full-sync-checkpoint-capacity-v1) | P2P 接续；ops 持久水位/archive/SLO；QA 冷恢复 | 长期接续与跨当前容量边界仍待实现验证 |
| [单权威提交与恢复](prd.md#p2p-single-authority-durable-commit) | P/R 单域失效、原请求续办、旧写者隔离及防回滚的真实证据 | [实际故障域验收](#des-p2p-controlled-fault-domain-acceptance-v1) | Ops 实际域/provider/witness；Runtime/P2P；QA 同窗口判定 | 不承诺单端失效后不停写；本机测试不证明真实故障域 |
| [professional_acceptance: p2p-target-role-isolation](prd.md#p2p-target-role-isolation) | 治理validator与permissionless服务、light/archive角色隔离；验证材料而非operator身份；prune重建+archive条件 | [des-p2p-layer-authority](#des-p2p-layer-authority) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-service-gate](prd.md#p2p-service-gate) | verified-serving→stale/catching-up→readonly→serving；proof冲突→isolated；非serving新intent receipt0、历史不改 | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-replication-recovery](prd.md#p2p-replication-recovery) | writer/epoch从seq1及guard不污染；权威恢复错误与qualified storage-challenge fallback区分 | [des-p2p-replication-scope](#des-p2p-replication-scope) | runtime/WASM/消费者/ops/QA各自authority；同candidate证据 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[product_requirement: req-dcs-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001) | 验证certificate及hash-bound材料，拒绝服务/cache/peer自授authority；runtime state apply仍外部 | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[product_requirement: ac-dcs-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) | 验证certificate及hash-bound材料，拒绝服务/cache/peer自授authority；runtime state apply仍外部 | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: req-dcs-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) | sameworld恢复链每环验证；历史只读与重新服务闸门分开 | [des-p2p-target-recovery](#des-p2p-target-recovery) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: ac-dcs-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | sameworld恢复链每环验证；历史只读与重新服务闸门分开 | [des-p2p-target-recovery](#des-p2p-target-recovery) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[product_requirement: req-dcs-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) | governed validator与permissionless服务隔离；公网IP非通用前提 | [des-p2p-layer-authority](#des-p2p-layer-authority) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[product_requirement: ac-dcs-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-003) | governed validator与permissionless服务隔离；公网IP非通用前提 | [des-p2p-layer-authority](#des-p2p-layer-authority) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: req-dcs-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004) | verified >2/3 active stake证书才推进；错集或缺少合法 round 证明拒绝；矛盾票不重复计权、不使合法证书失效 | [des-p2p-target-bft](#des-p2p-target-bft) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: ac-dcs-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-004) | verified >2/3 active stake证书才推进；错集或缺少合法 round 证明拒绝；矛盾票不重复计权、不使合法证书失效 | [des-p2p-target-bft](#des-p2p-target-bft) | P2P/runtime、ops、consumer与QA；product owner审读含义 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
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
| 本专题现行规则 | 配对 PRD 与保留旧章节的有界接口/失败合同 | engineering NodeRuntime 已有 Alg1 状态机、签名前 WAL、QC 历史恢复和 UDP 接线；V2 接入真实 World 治理执行、h+2 动态集合转换及安装恢复；正式 authority profile、checkpoint 消费者全链路及部署接受仍缺。pending/lineage/manifest/consumer 字段及 DC5 structured attachment 仍外部未闭合。 | 本候选有本机 core 与 NodeRuntime 测试；具体候选/命令/退出码由 PR evidence 绑定，不代表完整组合接受 |
| BFT/QC/治理恢复 | 旧 stake-threshold prototype 与 V1/V2 工程分支分离；V2 保存受保护 journal 并逐高度重验 QC、执行与集合转换 | 证书/round/recovery 与治理 transition 已有工程接线；正式 profile、长期 checkpoint 和真实外部可信 signer 验收尚未闭合 | 本机实现与测试不证明运行部署、反回滚或故障域独立 |

## 4. 边界与结构

代码归属按消费者需要分层：`oasis7_cas` 拥有本地内容寻址存储、编码、pin、文件索引及其存储探针；`oasis7_formal_protocol` 拥有可离线验证的 formal 协议类型、签名、证明和信任锚算法，不依赖本地 CAS、压缩或端点服务。`oasis7_distfs` 保留持久化协调、复制、端点、传输与恢复接线，并重导出同一份类型保持消费者兼容。客户端证明消费者直接依赖纯协议库，纯 CAS 消费者直接依赖 CAS 库。共享、版本化的 BFT V2 validator-set、proposer schedule、QC 验证以及正式 target/handoff/execution commitment 属于 `oasis7_formal_protocol::bft` 的纯证明合同，供 engineering 与正式调用方复用；`oasis7_consensus` 保留共识状态机、签名 journal 和持久化，并在原 `tendermint_v2` 路径重导出同一份协议类型与验证器。客户端普通依赖闭包不得因此带入 CAS、World、Node 或执行后端。拆分不改变编码、hash domain、落盘路径、回退顺序或权威接纳规则；native 恢复证据仍由实际恢复校验产生，不能用公开的未校验构造器替代。

net 拥有 transport/peer/DHT；consensus 拥有 proposer/attestation/action-root；存储域由 cas/distfs 分担本地 CAS/challenge 与分布式 replication；node/chain-runtime拥有commit后执行/恢复接线；proto仅共享wire；wasm_abi是manifest/runtime ABI单一来源。公开服务只能供应hash/proof材料，不能投票或写canonical。

### des-p2p-ordered-commit

下一committed height、有序序列/root/decode及journal绑定；错误不推进。

net 拥有 transport/peer/DHT；consensus 拥有 proposer/attestation/action-root；存储域由 cas/distfs 分担本地 CAS/challenge 与分布式 replication；node/chain-runtime拥有commit后执行/恢复接线；proto仅共享wire；wasm_abi是manifest/runtime ABI单一来源。公开服务只能供应hash/proof材料，不能投票或写canonical。

### des-p2p-receipt-finality

receipt≠QC；同parent/manifest/actions独立重执行，缺artifact/fault/root拒绝vote/commit。

hash-bound material仍须与verified历史/roots相符；服务operator身份、RPC可达、缓存/local receipt不能代签。active validators在投票前从同committed parent、runtime/manifest、有序actions独立重执行，artifact/fault/root错误拒vote/commit。版本由首次committed/finalityverified canonical execution block与activation边界决定；client compatibility只是声明；历史按原manifest重放，跨version replacement服从外部lineage单次效果。

### des-p2p-target-bft

proposer 优先级缩放中的整数舍入固定采用上述 CometBFT 同一提交的 [`RescalePriorities`](https://github.com/cometbft/cometbft/blob/709fd12b4b18cf1442d43c5d34009392c7d674ed/types/validator_set.go)：`scale=ceil((max-min)/(2W))`，随后有符号整数除法向零截断。该公式明确规范伪代码除法未标注的舍入方式，保持缩放后 range 不超过 `2W`；实现须避免计算 `diff+threshold-1` 引入溢出。

Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46。首版采用下述完整 Tendermint 状态机；oasis7 保有应用、wire、持久化与证明消费语义，依赖可替换，不要求新增外部进程或 Go 依赖。现有 proposer/attestation threshold prototype 不作此状态机的实现证明。

`BftProposalV1`、`BftVoteV1`、`BftCommitCertificateV1` 是新协议目标。所有签名绑定固定 world/chain/genesis、协议版本、height、round、消息 phase、活动 validator-set hash 及 block ID 或显式 nil；proposal 另绑定 proposer 与 validRound。block ID 承诺 parent CommitRef、authority epoch、governing manifest、有序 actions 及 execution/event/state roots，候选身份不含当前 round 或 proposal 签名，保证同一值能跨 round 重提。编码沿用[正式协议](#des-p2p-formal-controlled-authority-v1)的 canonical CBOR、domain framing 与 strict Ed25519，但使用独立 `oasis7.bft.<kind>.v1.<purpose>` 域；nil 与任意非空 hash 不可混用，旧 wire 不自动升级。

验证先从合法历史确定本高度固定的集合与整数权重，再逐票验签、按 validator 去重，要求同一 height、round、precommit phase 和非 nil block ID 的总权重 `q=floor(2W/3)+1`。权重求和与阈值算术均 checked，零总权重、溢出、重复身份/key 或错集拒绝。证书中拼接不同 round、phase 或 block 的票不能凑数；当前已进入更高 round 不妨碍接纳同高度较早 round 的完整有效 commit certificate。发现 equivocation 保留可验证证据，不能重复计权或未经治理自行移除当前集合成员；一名 Byzantine 的矛盾投票也不能使一个仍满足规范的有效证书自动失效。

`valid(v)` 由同一 committed parent、manifest、确定有序输入和可验证执行闭包定义；每名活动验证者在隔离候选状态重执行并核对全部 roots，投票前不发布世界效果、不重新调用 LLM。仅在证书验证与适用持久条件成立后安装 canonical 状态；复制、checkpoint、普通消费者和冷恢复使用同一 verifier，不接受 receipt、传输成功或本地 stake 计数替代。

#### des-p2p-bft-round-lock-v1

规范算法为 Buchman、Kwon、Milosevic [The latest gossip on BFT consensus, v3，Algorithm 1](https://arxiv.org/abs/1807.04938v3)；其 `n` 按 voting power 计。固定高度内 Byzantine power 须小于 `W/3`；安全性不依赖及时到达，活性依赖最终同步和可用的足额正确节点。proposer 选择采用 [CometBFT weighted round-robin 规范](https://github.com/cometbft/cometbft/blob/709fd12b4b18cf1442d43c5d34009392c7d674ed/spec/consensus/proposer-selection.md)的优先级更新、归一化与成员变更规则；oasis7 将相同优先级的 tie-break 固定为 consensus 公钥原始字节升序。优先级属于确定性持久状态，每个 `height/round` 只有一个合法 proposer。slot 仅 pacing，不替代 round。初始化、一次触发规则、nil、未来 round 跳转和三类 timeout 完整按算法实现，不混入旧 prototype 的同高替换或其他共识的解锁规则；采用的参考规范内容随协议版本冻结，上游文档变化不自动改变已激活规则。

高度边界的持久 proposer priority 每提交一个 block 固定推进一次，与用于提交的 QC round 无关；当前高度的 round 只推进临时 proposer schedule。相同 parent/block ID 的不同 round 合法 QC 必须导出完全相同的下一高度 schedule，正常提交、历史重放和保护签名 journal 共用该规则。依据固定版本 CometBFT [`updateState`](https://github.com/cometbft/cometbft/blob/709fd12b4b18cf1442d43c5d34009392c7d674ed/state/execution.go#L553-L607) 的逐 block 更新，不能把 round-local priority 当作下一高度锚。

签名 wire layout V1 与共识规则代次分别标识：validator-set hash 必须绑定显式 `height-seed-v2` schedule ruleset，并使用独立 hash domain；proposal、vote、QC、genesis pin、WAL 和 provider binding 均沿该 set hash 隔离旧规则。旧 round-dependent 规则的 set hash、WAL 和历史证书拒绝按新规则重解释，不自动迁移或混合集群。既有工程链需显式配置新的 scope/genesis；历史迁移或在线治理切换须另有完整协议和验收。


下表是实现审查必须保持的安全不变量，算法原文仍定义全部触发顺序：

| 事件 | 允许的动作与持久状态 |
| --- | --- |
| 新 proposal，无 validRound 证据 | 只有 valid(v) 且未锁定或锁定同一值才 prevote(v)；否则 nil。仅接受本 round 合法 proposer。 |
| proposal 引用旧 round 的 validRound | 先取得该高度、该值、该旧 round 的 q 个有效 prevote；仅在 lockedRound≤validRound 或锁定同值时允许改投。相等边界采用 Algorithm 1 第 29 行伪代码，proposal 转移与本地投票校验必须一致，不采用同篇说明段的严格不等号。裸 round 数字、超时或新 proposer 自述均不解锁。 |
| 本 round 的 q 个同值 prevote | 按算法保存 validValue/validRound；尚处 prevote 时先保存 lockedValue/lockedRound 再 precommit(v)。q 个 nil prevote 或 prevote timeout 只能走相应 nil precommit，不能清锁。 |
| 任一 phase 的投票权重 | 同一 phase 中任意值的不同合法 validator 总权重达到 q 时才启动该 phase timeout；同一 validator 的 equivocation 证据最多贡献一次权重。lock/valid/commit 仍要求相应的同值 q，混合 nil/block 票不能合并成决定。进入 phase 本身不启动 quorum timeout。 |
| 超时、未来 round 与提交 | propose timeout→nil prevote；规范条件启动的 prevote/precommit timeout 分别→nil precommit/下一 round；未来 round 跳转须同一高度、同一未来 round 中超过 W/3 的不同合法成员消息。round 推进保留锁和有效值，只有本高度完整 commit 后才能初始化下一高度。 |

timeout 随 round 增长，旧高度/round 的 timer 不改变当前状态；prevote 与 precommit timeout 仅在对应 phase 已观察到任意值的 q 权重后启动，phase 转换时会重新检查已缓存 tally。equivocator 在该门槛也只按一名 validator 计权；锁定、validValue 和 commit certificate 继续按单值 quorum 判断。上溢或无法恢复计时状态停签，不回绕。实现须用独立状态机模型和一致性向量逐规则核对，覆盖消息乱序、重复、丢失、nil、锁定后 proposer 作恶及分区重合；只改阈值并不形成 BFT。三等权时 q=3；四等权时 q=3，才具备容忍一名 Byzantine 的权重条件。共享 key custody 或单机多实例不能作为独立故障域。

#### des-p2p-bft-signing-recovery-v1

签名槽固定为 `(world/chain/genesis, protocol, signer_key, height, round, phase)`；nil 同样占用槽。同槽仅允许同一完整 canonical bytes，不能把时间戳、manifest、set hash 或其他字段变化当作可重签例外。已保存的原签名可重发；异字节一律拒签。不同 round 的安全改投仍须上述锁定证据，不能因签名槽不同跳过共识状态机。

签名路径是“验证事件及锁条件 → WAL 持久保存完整决定与新状态 → signer 保留该槽的原字节决定 → 生成并持久保存签名 → 对外释放”。WAL 同步保存 height/round/step、locked/valid value 与 round、对应 prevote 证明、活动/待生效集合、原 proposal/签名字节及最后 committed anchor；引用的候选和必要执行工件先持久 pin。文件同步、原子替换和目录同步或等价事务必须在响应/广播前完成，任一步失败或结果不确定立即停签，不只是记录 last_error 后继续。

只靠本机 WAL 防崩溃，不防旧盘回滚或复制 signer。正式 custody 须把签名决定与不可回滚水位置于同一受保护签名边界，并保证该 key 同时只服务一个合法实例；远程 signer 也必须执行相同规则，不能成为任意 bytes 签名口。跨机接管须证明旧实例停签并转移原决定/WAL；无法证明时保留只读。不得以删除状态文件、清 pending、重置到 committed height、换目录或自动新建 signer-state 恢复投票。

重启先校验外部最低 anchor、受保护签名水位与 WAL 完整性，重放到最后可证明状态再接网络。reserve 已落盘但签名回应丢失时，只续办原字节；已签而尚未广播时只重发原签名；材料不齐、倒退或冲突保持 blocked。进入下一高度和回收旧热 WAL 必须已有完整持久 commit 及可重建历史证据；不能丢失尚未提交高度的锁或处罚证据。

#### des-p2p-bft-protected-signer-v1

Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46。当前工程 Remote adapter 使用 Unix OS CSPRNG 和文件生命周期能力；非 Unix 平台明确拒绝该模式，不声称新增跨平台 custody 支持。受保护 BFT signer 与现有 P/R endpoint custody 复用固定 provider key、资源授权、请求认证和独占 lease 的信任边界，但使用独立的 BFT 协议域、实例绑定和结构化命令；不能把 BFT 原文塞进 endpoint 通用签名入口。provider 的硬件与部署属性仍须独立验收，签名的 custody 声明本身不是该属性的证明。

工程 API：先由独立 provisioning 将完整绑定及 genesis journal 写入满足本节持久事务合同的 backend；[`BftSignerServiceV1::open`](../../crates/oasis7_distfs/src/protected_bft/service.rs)只打开既有绑定，`serve_connection`为有生命周期的协议入口。Node 显式选择 `with_tendermint_v1` 后，通过 [`with_protected_bft_signer`](../../crates/oasis7_node/src/node_bft_protected.rs)安装带固定 trust 的 `RemoteBftSignerProviderV1`，replication 目录只保留持久镜像，不配置本机 consensus 私钥。该 API 不自动 provision key、不激活正式 BFT profile，也不替代实际 backend 的硬件与防回滚验收。

绑定固定 world/chain/genesis、authority epoch、完整 validator set、local validator/key、timeout policy 和 operator 配置的 signer instance。provider 须独立预置该绑定、持有 validator 私钥并授权请求者；Node 的 acquire 不能创建、重置、替换 key 或自授权注册。一个绑定至多有一个活跃 lease；断联后旧 lease 失效，旧实例不能凭旧 lease 继续修改或签名。认证 key 与 validator key 分离，Node 不持该 validator 私钥。监听、worker 和任何签名释放前先取得 lease、核对 provider 状态并完成历史及 WAL 恢复。

acquire 先用认证且只读的 PrepareAcquire 获取 provider 当前持久 generation 和本连接独有的 provider CSPRNG prepare nonce，再把该 expected generation 与 nonce 绑定到签名 Acquire 请求；服务端核对并一次性消费本连接 nonce，旧 Prepare/Acquire 帧组合不能搬到新连接使用；backend 原子比较并递增，并返回该原子点的最新完整 journal 状态；非原子预读不是 journal 不变的比较锚。CAS 可能已成功但返回无效状态或结果无法确定时，服务停签并要求权威恢复，不能当作普通可重试冲突。若 Prepare 与 Acquire 之间原连接释放使代次推进，客户端仅在收到经过认证且明确无活跃 token 的更高代次冲突时，有界重新 Prepare；新活跃 owner 或结果不确定时停止，不盲目抢租。原 Acquire 帧重放须因旧 generation 拒绝，不能再次 fence 当前实例；该拒绝跨 provider 重启仍成立。Node 在 acquire 前先持本机 WAL 生命周期锁，重复启动失败不能先撤销现有 worker。

服务端 lease 同时绑定实际连接会话；原 signed frame 不能搬到其他连接使用。连接结束、响应写入失败或显式关闭时，以精确 generation/token 原子释放并推进持久 generation，连同该旧代次下预先签署的 Acquire 一并失效；PrepareAcquire 与 Acquire 由服务端绑定同一实际连接，不能信任请求自报 session ID；迟到的旧会话释放不能撤销新 generation。可调用 handler 与 TCP listener 共用有生命周期的会话入口，不保留无会话的授权旁路；释放结果不确定时停签并要求权威恢复。

lease 具有 provider 持久化的单调 generation；journal CAS、签名决定提交与原签名响应的释放均在同一受保护事务中核对当前有效 generation。新 lease 生效后，旧 generation 的迟到请求不能产生新决定或覆盖新状态；客户端核对响应的 lease/generation。已提交并对外释放过的原签名仍是可重传证据，不承诺撤回网络中已有字节。分区或接管时不能按 Node 本地 PID、socket 超时或猜测旧实例死亡抢租；无法证明旧 generation 已 fenced 时拒绝新的写 lease。

保护对象包含完整的当前高度签名 journal：高度、父提交、起始 proposer priority、原始计时基点、已验证事件、锁/valid 状态的可重放材料、pending proposal/vote intent 和精确 pending QC。只保存签名槽高水位而允许 Node 恢复更旧的锁状态，不满足本合同。journal 的共享纯状态转换规则由 Node 本机模式与 provider 一致使用；provider 从固定 genesis 和逐高度合法 QC 推导 schedule，并独立重放协议事件、校验转换，不能接受任意新 hash 或任意 preimage 作为状态更新。

每次 journal 转换用预期 revision 与原 journal digest 作 CAS；只允许经过状态机校验的一步转换或完全相同的重试。首次高度计时通过专用 start-height 操作，由 provider 在事务内采样可信当前时间并写入原点；普通 advance 禁止调用者自行提交 None→Some 的计时原点，避免回填旧时间提前触发 timeout。首个高度时钟不可重置；事件只能合法追加；pending intent 只可按确定性决定预约，并由对应原签名事件完成；下一高度只可由精确 pending QC 导出的合法 schedule 初始化。远端事件与 timer 也须受保护，不能在改变锁状态后只更新本机文件。

protected 模式使用 provider 的受保护单调时间轴，Node 不以自身 wall clock 快进 timeout。provider 用已持久化的原基点、timeout policy 和可信当前样本核对事件时间及实际到期条件，在事务中维持不回退的时间 floor；签名完成事件由 provider 记时。新 challenge 的时间读取不修改 journal revision，也不重新开始高度计时。provider 重启后若无法证明 elapsed 连续性或当前时间不早于持久 floor，则停签，不能以重置 clock 恢复服务。受控测试时钟只能证明此接口与拒绝行为，不能代替真实可信时间验收。

已完整验证的历史前缀和 schedule 可作为进程内缓存，逐次验证新的合法扩展；缓存建立与恢复仍从固定 genesis 完整验真，不能把后端自报 digest 当作已验证凭据。同一身份但内容改变、历史倒退或分叉必须拒绝。普通时钟轮询只在 backend 对当前连接 lease 和持久时间 floor 完成精确检查后，使用本会话已验证的 revision/digest 返回轻量证明，不加载或重验整条 QC 历史；新 journal 转换、签名与重开仍保留相应完整验证。历史长度不能令每 5–100 ms 的空闲时钟读取退化为全链签名验证并触发健康节点停签。

活进程重新获取租约时，也须保持此前已验证状态的连续性：同 revision 必须内容完全相等；较新状态只能沿原预约与正式 journal 转换前进，跨高度必须消费原 pending QC。无法证明中间转换的 revision 跳跃或多高度变化停签，不能以一次独立的全历史验真覆盖已观察到的预约、签名或锁。

当前高度 journal 与保留 QC 历史分别沿用 256 MiB 编码上限；保留高度仍至多 4096，round 仍至多 4096。RPC 总 frame 上限必须容纳两者及有界决定/身份 envelope，同时在读取长度后、分配 body 前拒绝超限；不能只增大总体 frame 而不验证子材料界限。旧已提交高度不再接受新的签名请求；其原 QC 按历史合同保留。容量耗尽停签，不删除锁/处罚证据以腾空间，也不宣称实现了可信 checkpoint GC。

签名命令只能引用当前 journal 已预约的完整 proposal/vote intent。provider 重新检查 scope/set/key、height/round/phase、nil 或 block 值、原 canonical bytes 与重放后的协议决定，在同一受保护事务中保存精确决定及签名，再响应 Node。同槽同字节返回原签名；异字节、旧 lease、错误 revision、非法 round/lock 转换拒绝。Node 入口只接受固定 provider pin 与请求新鲜性验证的 Remote adapter，拒绝任意外部 provider/lease trait 注入；该 adapter 校验 provider 证明，Node 再独立校验返回签名与完整 journal，再持久保存本机镜像后才广播。proposal 和 vote 均须经过此路径，不留可用的本机私钥备用入口。

provider 是受保护 journal 与签名决定的权威存储，本机 WAL 是有生命周期锁的持久镜像。provider 已完成而本机镜像未落盘、签名响应丢失或进程崩溃时，重连通过新 challenge 取得同一绑定的最新认证 journal 和原决定；重放验证后恢复原锁、timer 和 pending intent，不重签异字节。本机镜像水位领先 provider、相同水位不同 digest、未知绑定、缺少完整合法 QC 历史或无法重建当前 schedule 时停签。旧整盘备份若缺少 provider 已知高度的提交历史，必须先完成合法历史恢复或明确阻断，不能把 provider 水位退回旧盘位置。

本机工程 WAL 与受保护镜像使用可辨识的持久格式；受保护目录缺少 provider 或误配回本机 signer 时拒绝启动。既有导出过私钥的本机工程链不能靠删除私钥文件、换目录或把同一 key 导入服务而宣称转换成不可导出 custody；本阶段不提供这种迁移。新 protected 绑定由独立 provisioning 产生；既有链的正式接入仍须适用的治理 key 轮换或权威交接合同。已有 P/R wire 与本机 BFT WAL 保持自身格式，不能静默接受新模式或降级。

实际验收须覆盖真实 Node proposal/vote 调用、同槽精确重试、冲突拒绝、含锁状态的旧 WAL 恢复、provider 领先历史时停签、同 key 第二实例、签名前后和镜像发布前后的故障、provider 重启及无降级。受控测试 provider 只能证明接线、事务与恢复行为；真实 HSM/TEE 或等价服务的 key 不可导出、存储不可回滚、实例 fencing 和独立故障域仍是单独部署义务，未满足时不能发布正式 BFT readiness。

#### des-p2p-bft-validator-transition-v1

Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46、O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3。治理准入、授权与运行应用闭环由[安全治理合同](blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-membership-application)拥有；本节唯一规定投票集合何时切换。采用与 [CometBFT 官方 validator update 语义](https://docs.cosmos.network/cometbft/latest/spec/abci/Requirements-for-the-Application#updating-the-validator-set)一致的两高度延迟，作为 oasis7 自身协议，不声称已接入 ABCI。

首版一个 world 至多有一个 scheduled 尚未激活的转换；其他 approved 意图保持未排程，待前次激活后按当时 committed policy/set 重新验证。epoch 意图须由协议确定性映射到提交高度 h 和固定 activation_height=h+2；无法映射、已错过边界或旧 set hash 不符则不排程，不能由单机解释或延后。

V2 转换使用固定映射：`activation_height = activation_epoch.checked_mul(epoch_length_ticks)`，`h = activation_height.checked_sub(2)`；溢出、h 不是合法正提交高度或在 h 之后才提交的 activation 均拒绝，首版不在 genesis 排程转换。epoch 长度及映射规则版本绑定到 V2 genesis 与转换链，首版拒绝在线改变该映射的策略更新。已有转换在 A₁ 激活时，下一转换的排程高度 A₂−2 必须不早于 A₁；这按实际边界拒绝重叠，不额外禁止长度为 1 的 epoch。V1 固定集合的历史、wire 和签名域保持自身版本，不按 V2 重解释。 治理可以在 h 之前提交目标 epoch 的签名 activation 意图；它只进入未来治理状态，不提前取得投票权，集合转换仍须在 h 从当时真实 World 重算并最终提交。已错过 h 的新 activation 不得留待下一边界自动生效；入队预检与候选执行均须拒绝迟到意图。

治理集合的只读预演可以基于同一个已认证 committed World，显式解析指定未来 epoch 的 admissions 与 registry；结果必须同时保留来源 commit、真实 state root、已验证 profile、来源 epoch 和目标 epoch，不能把目标 epoch 伪装成当前 World 时间。预演复用治理选择、资格与 registry 校验，错误照常传播，不改变 World、当前集合或运行应用状态，也不构成本机安装 ACK。转换区块的最终集合仍必须从该候选真实执行后的 World 独立重算，不能直接信任调用者提交的预演结果。

V2 的候选执行和 committed 读取分属不同证明阶段。候选在固定已验证 parent 上独立执行完整 canonical actions，再从真实执行后的 World 解析目标 epoch 集合；该阶段绑定 parent、治理 action 身份、原策略及 execution/state roots，不伪造尚不存在的本块 CommitRef。h 区块承诺完整转换材料及其摘要，不能把自身最终 QC CommitRef 放入自身 block ID；在 h 的 QC 验证成功后才派生对应的 committed 来源。h+1 的证明继续绑定同一转换，并从该高度真实执行后的 World 重算 activation epoch 的集合，必须仍与已提交的新集合 hash 相同；不能通过其他治理入口改写 pending 转换。安装只能消费这条从固定 genesis 延伸的合法证明链。

BFT 治理动作复用既有完整动作签名域、controller 阈值、nonce 和父策略规则。其运行时 parent 上下文由真实 driver 已恢复的 opaque BFT cursor 与逐项匹配的 World root、time、manifest 和固定 epoch 映射建立；固定 genesis 信任仍由启动配置及 driver 独立检查，cursor 本身不认证任意调用者给定的 genesis。该上下文与受控模式的 qualified parent 分开，不能伪造 formal reader。执行前固定父 controller registry，显式开始对应 successor tick，在同一 detached World 上验证并执行批次，完成后结束阶段；上下文不进入 Snapshot。没有匹配 parent/tick 的普通 step 不得应用 admission 治理，新 controller policy 不能在同批次为自身或后续动作授权。

受保护签名器的 V2 genesis 绑定固定初始集合、epoch 映射策略和本机不可导出 key 身份；运行中活动集合由逐高度证书与转换链导出，不允许普通 acquire 改写集合。集合安装原子推进持久 signer generation，隔离旧 owner，并返回绑定转换来源、激活高度、集合摘要、runtime 实例和新 generation 的真实安装确认。本机 consensus、WAL 与 provider 均完成安装并持久记录后才能报告本机 adopted；中途崩溃只能续完同一安装或停签。 V2 以 provider 的受保护 journal 作为签名前 WAL 的唯一持久权威，不再维护另一份可独立推进的 Node 日志。 启动必须先固定本机 driver 的模式／genesis 并取得其稳定载体上的 OS 生命周期锁，再执行远端 provider acquire；本机重复实例不能在本地锁失败之前推进远端租约代际、隔离原实例。节点必须从该 journal 与已验证 schedule 实际恢复目标高度的共识状态，并核对 driver 的持久执行记录，之后才签安装 ACK；WAL 安装摘要绑定同一 journal 和 schedule。重启须重做独立协议／执行重放并核对适用的持久安装回执，不能仅凭 DTO 摘要或旧 ACK 恢复投票；原请求恢复不得产生不同的安装证据。未被当前集合纳入、已被移除或尚未安装的 key 不得签名，但合法历史验证与只读追赶仍可继续。软件测试的安装确认不证明硬件不可回滚或真实故障域。

| 权威边界 | 集合与证明规则 |
| --- | --- |
| h 最终提交 | 旧集合 S_old 签 h；治理动作经同一确定性执行产生完整 S_new、权重/key 映射、旧/新 set hash、原治理决定及 h+2。更新承诺进入 h 的 block/execution roots。 |
| h+1 | 仍由 S_old 签，header/证明绑定下一高度的 S_new hash 和 h 的转换承诺。新节点取得 h+1 的完整合法状态/证明，恢复签名水位并安装 S_new。 |
| h+2 及以后 | 只以 S_new 验票；退出/旧 key 权重为零。未安装或未同步完成的节点停止签名，不能让全网边界等待本机 ACK，也不能临时沿用旧集合。 |

排程和安装记录至少含治理决定 CommitRef、world/manifest、旧/新集合 hash、activation height、runtime 实例与 signer binding，安装 ACK 持久化后只报告对应的本机采用事实。迟到节点须从完整合法历史定位当前活动集合的 h+1 确认证书，重放至当前 head 后补做安装；不能要求 h+1 仍是当前 tip。安装意图分别固定转换确认 h+1 与实际已安装 head T，T 可以是 h+1 的合法后继；pending 意图保持同一请求和证据，不随重试改写。ACK 引用的 head 须属于当前 verified head 的合法祖先链，且其 set/activation 与当前签名高度及受保护 signer 状态一致；旧实例、已被后继集合替代、部分安装和重启丢失 ACK 均不授予投票权，不要求每个新块重做同一集合安装。重复 validator/key、负权重、空集合和总权重溢出在 h 的有效性检查时拒绝。轮换、移除与 stake 改动同样适用；发现作恶不允许本地先改分母。

安全性沿旧集合最终提交的唯一历史传递到新集合，每个活动集合分别满足 Byzantine 权重限制；不要求把新旧票混合凑一个阈值，也不把独立治理批准当作共识证书。该规则与[单权威在 H 提交、BFT 从 H+1 开始](prd.md#p2p-authority-handoff)的 profile 交接不同。

同步/恢复须验证完整集合转换链并固定可信 checkpoint/minimum。长期离线的节点不能只依赖已退出 key 对“最新历史”的自证：BFT 激活配置须规定可信 checkpoint 的有效期及旧 key/stake 安全约束，有效期不得超过该约束可保证的期限；超期或约束不明时停止自动追随，重新取得独立认证的同世界 checkpoint。此信任条件及当前各集合的实际故障域须进入部署验收，不能由算法论文代签。

#### 本次工程垂直切片：固定集合 NodeRuntime BFT

显式 BFT voter 仍须遵守既有本地角色策略：选择器和启动入口均要求 `network_policy.node_role_claim=ValidatorCore`。Sequencer 以及显式配置该 claim 的 Storage 可以参与；Observer 和仅声明 FullStorage 的节点拒绝参与签名，选择器不隐式覆盖角色或网络策略。

该选择器只用于启动前配置，不实现运行中的 PoS→BFT 交接。有正在运行或尚未 join 的 worker 时拒绝选择，不能在旧 PoS worker 仍运行时仅改配置并显示 BFT running。它是按值消费 runtime 的 builder；返回错误会按既有 Drop 合同停止并回收被消费的 runtime。

本地动作入队、选择 BFT 前已有队列及启动前队列均由 execution hook 按同一闭合 RuntimeAction 解码规则检查；不支持的 SimulatorAction、provider/service 输入及无法解码的 runtime action 必须在接受入队或启动 worker 前拒绝，保持队列和字节预算不变。远端候选仍须独立验证，不能以本地 admission 取代执行预演。

本地提案先按 action ID 规范排序并合并完全相同的重试；同 ID 的冲突输入在 BFT 入队或预存队列检查时拒绝。提案前与 PoS 复用当前 parent 的治理重授权与 controller nonce 高水位规则，移除已失效的治理项并释放字节预算，记录本地拒绝原因后继续有效输入；到达顺序、重复重试和过期治理 proof 不得终止 BFT worker。远端 proposal 的严格有序、去重和授权校验不因此放宽。

批次上限只截取本次候选，不误删队列尾部；它不延长治理 proof 的 parent 绑定。前一子批提交后，尾批的旧 parent proof 可能失效，须被重授权检查拒绝并由调用方按当前父状态重新授权，不能自动改写或沿用旧签名。

Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46。此次实现为显式 opt-in 的 `engineering_tendermint_v1` NodeRuntime 路径，必须调用 `NodeRuntime::with_tendermint_v1` 提供固定集合配置；`NodeConfig` 没有 consensus-mode 输入字段，`NodeConsensusMode::TendermintV1` 只是运行快照的状态投影，不是选择或授权入口。默认 PoS prototype 行为不变；旧 Proposal/Attestation/Commit 与新 BFT 域类型互不升级或回退。集合权重与成员数使用通用 checked 整数算法，四名等权验证者、q=3 是集成验收基准，不是生产算法常量。每个高度内集合不变，任何缺少新格式证书的输入均不能走旧 `PosConsensusStatus::Committed`、replication 或恢复路径。

选择 `with_tendermint_v1` 后，启动会校验固定 scope、validator set、local validator 与 NodeRuntime identity、耐久签名配置及 execution hook；多验证者配置必须提供显式 gossip transport，静态 peer 数量受 validator bound 限制。该模式所需依赖缺失时，在 listener 与 worker 启动前失败；未调用选择器仍是既有 PoS 默认路径，不把默认值描述为自动回退。每个固定集合和每个候选都会在广播或释放签名之前按完整最大 QC 的 JSON gossip envelope 预检 65,507-byte UDP payload 上限；预检失败就拒绝启动或候选。协议中的 16 MiB 单 proposal、2 MiB signed material 与 256 MiB 历史上限不是 UDP 可传输承诺，不能据此分片或放宽 datagram 限制。

NodeRuntime 的 BFT 分支使用 Algorithm 1 的 propose/prevote/precommit、valid/locked 值与 round、q 证据、nil 投票、超时和 `f+1` 同高未来 round 推进。提案者按 CometBFT weighted round-robin 维护确定性 priority，priority 相同时以 consensus 公钥原始字节升序决胜；成员权重、总权重、q 与 round/height 算术均检查溢出。候选先在独立执行副本上验证并核对 block/action/event/execution/state roots；该验证不能改动 canonical World、journal、receipt、commit head 或 replication。只有同高度、同 round、同 set、同非 nil block ID 的完整 q-precommit 证书，连同匹配提案与 `valid(v)` 结果，才进入 BFT 专属唯一安装、replication 与重放入口。

本机 WAL 在签名前原子持久化完整的 height/round/step、lock/valid 证据、priority、候选/提案与签名槽决定；签名落盘后才允许广播。每个高度的首次单调计时样本也先持久化，重启沿用该高度原始计时基点，不能因进程重启延后 phase timeout。重启只重发相同签名槽的原 canonical 决定，异字节冲突或不可证明的 WAL 拒绝投票。冷启动从固定配置的 genesis schedule 打开 WAL；节点必须从完整、逐高度重验的 QC 历史推导当前 schedule，再校验 WAL 并处理唯一允许的精确 pending-QC 崩溃窗口。WAL 中保存的 schedule/priority 本身是不可信输入，不能作为新的恢复锚。这个本机工程 WAL 仅覆盖进程崩溃恢复，不证明 VM/磁盘回滚抵抗、复制 signer 隔离、硬件保护或真实故障域独立。

BFT gossip 使用与旧 PoS gossip 不同的消息变体。线上 envelope 外层使用 `kind: "bft"`，BFT payload 再以自己的 `kind` 与 `message` 字段编码，例如 `{"kind":"bft","message":{"kind":"history_request","message":{...}}}`；两个 tag 位于不同对象层，避免与旧 gossip 的 `kind` 冲突。在线追赶请求只是未签名的有界 cursor hint：响应端必须把 scope、validator-set hash 与请求的精确 `(height, block_id)` 和已验证本地历史核对，再至多返回一个完整 QC；请求本身不授权历史、投票、执行或状态安装。NodeRuntime 的 `raw_bft_commit_history_page` 只返回连续、范围受限的原始 QC 对象，不返回 verified token 或权威判断；`bft_status` 提供当前 phase/round、lock/valid 投影、拒绝计数和 transport 诊断。两者都是进程级观察接口：状态计数、last error 与 durable-event-count 不是受保护或可独立验证的持久证据；消费者仍须从固定 genesis 与配置重验完整 QC 链。worker 对请求与响应限速，收到的 QC 仍须完整重验、隔离预执行并通过唯一 commit hook。实际 gossip 是单 UDP datagram，不做 IP 分片或可靠传输；签名前按完整 JSON envelope 计算并拒绝超过 65,507 字节的消息。协议/CBOR 的 16 MiB 或历史 256 MiB 上限不代表它们可经此 UDP 路径传输，无法放入单 datagram 的证书或结果必须明确阻断，不能先签后发现无法广播。

此切片不激活正式 `bft` authority profile，不实现 h/h+1/h+2 validator-set transition、治理成员生效、controlled-to-BFT authority handoff、跨故障域 signer custody 或部署 readiness；formal controlled profile 必须继续拒绝该工程模式。静态集合上的 NodeRuntime 正向闭环和对抗回归只能证明当前实现路径，不得写成正式链可用、主网安全或 BFT readiness。

#### 动态集合 NodeRuntime V2 工程入口

Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46。动态集合使用独立的 `NodeRuntime::with_tendermint_v2` API、V2 wire/signing 域、QC 历史与 execution ledger；调用者提供固定 genesis 的 `BftSignerBindingV2`、带固定 trust 的 `RemoteBftSignerProviderV2` 和实际 execution hook。该入口直接运行 Node worker、gossip 与确定性提交；不是 `NodeConsensusMode` 状态字段或正式 profile 的选择器。V1/V2 不共用 ledger，也不从旧签名、旧 WAL 或旧证书推断新集合权限；混用、配置漂移或缺少材料均阻断，迁移不静默降级。 V2 journal 与 schedule 的 proposer priority 保留完整 checked i128 算术，线上使用规范十进制字符串；拒绝整数、正号、前导零、负零、空白、溢出及重复 key。该编码须通过实际 kind/body tagged command 的 canonical CBOR 往返验证，不能只验证裸 journal；V1 编码保持独立。

`NodeRuntimeExecutionDriver` 从原始 V2 QC 链独立重放真实 World，复核治理 parent、动作认证和实际候选 roots，按本节 h/h+1/h+2 合同应用集合。Node 在释放新集合签名前，恢复 provider 的受保护 journal 并对照实际 driver 持久记录完成安装确认；迟到安装与原请求重试沿用前述祖先及 pending 合同。`raw_bft_v2_commit_history_page` 返回原始证书，`bft_v2_status` 只投影进程状态；消费者仍须独立验证完整链及自己的信任起点。历史响应只查询已经验证的连续历史，不在每个轮询请求中重复验证整个证书前缀。

V2 工程 journal 持久保留当前未提交高度的已验签冲突 proposal header 对，追加证据不得改写待完成的签名决定，重启须恢复原证据。只有该高度的合法 QC 持久提交并转入已验证的后继高度后，才退役当前高度 journal；当前工程状态接口与 QC 历史不提供跨高度处罚证据归档或 slashing，不能据此宣称实现了历史处罚链路。

对应验证入口见[动态集合 V2 工程路径](../../testing-manual.md#动态集合-noderuntime-bft-v2-工程路径)。Node 网络用例的确定性 test hook 与内存 signer backend 不证明实际 World、磁盘耐久性或硬件防回滚；真实 World/ledger 由 chain-runtime 用例另行验证。V2 工程入口仍不激活正式 `bft` authority profile，不完成 controlled-to-BFT H/H+1 权威交接、checkpoint/消费者全链路、跨故障域 custody 或部署 readiness；这些是 #4975 保留的后续实现与验收义务。

### des-p2p-target-recovery

固定 world/chain/genesis 身份→合法历史 profile/epoch/manifest 激活链→已确认 checkpoint/header+authority transition→hash snapshot→canonical committed replay→root→serve/vote；BFT 高度另验证 certificate/set transition，每环缺失/冲突/回退/异world停止。

按信任链逐环确认，snapshot blob缺失/hash错或replay/root不一致停止，不能用任意peer/latest backup/otherworld/覆盖替代。pruning必须先证明checkpoint+replay重建、hash/root验证和冗余archive可用。既有历史/offline local recovery事实不抹除；当前governed live部署禁止raw-copy路径，signed-V2只在原runbook限定observer/drill scope，observer高head不等于execution-required full restore。

运维接受入口是 [node-triad inventory/采样合同](node/node-triad-operations-observability.prd.md#inventory-与采样合同) 与 [权威边界](node/node-triad-operations-observability.prd.md#权威边界)；governed live 约束见 [bootstrap truth model](blockchain/public-testnet-governed-bootstrap.runbook.md#3-truth-model)、[hard rules](blockchain/public-testnet-governed-bootstrap.runbook.md#4-hard-rules) 和 [failure/rollback](blockchain/public-testnet-governed-bootstrap.runbook.md#14-phase-h---failure-handling-and-rollback)。这些具体环境的恢复 admission 不覆盖历史/offline local 恢复。

#### des-p2p-full-sync-read-only-v1

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4、O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3、O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b。已有 `formal_full_state_sync_read_only_v1` 从独立固定的 C0 验证持久历史页链，核对 minimum 与精确旧 head，安装目标 cut 的 manifest、record 及 retained material 闭包；边界见[本机生命周期](blockchain/local-node-lifecycle.runbook.md)。状态是 verified read-only，不构造 writer authority、BFT voting membership 或 current serving 资格。

provider 只是材料来源；目标 cut、原请求身份和全程预算在换 provider、分页或重试时保持。逐页/对象验签、hash、连续性及权限，完整闭包与本地持久安装前不推进对外 verified head；失败保留上一次合法 cut 和明确 blocker。private full-state 读权及撤权按[安全治理主责](blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-read-authorization)执行，同步授权不免除最低水位或 proof 验证，也不能被 public observer 权限替代。

<a id="formal-read-error-classification"></a>
**正式读取错误与路由映射。**Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b。view、full-sync manifest/object 与共享 resolver 从产生处保留请求/绑定拒绝、实际读授权拒绝、publication/runtime 未就绪、合格历史不可用、内部 I/O/缓存/编码/不变量失败、历史完整性/副本分歧六类；诊断文字仅用于展示，不参与 status/code 决策。用户输入解析失败属于请求拒绝，服务端缓存材料解析失败属于内部失败或完整性失败；经过授权函数不能将其中的 source、安装、编码或响应构造失败全部改为授权拒绝。底层已有结构化错误时按其分支和调用来源转换，不先转为文字推断。

| 产生位置与内部类别 | view status/code | full-sync manifest/object status/code |
| --- | --- | --- |
| 严格 HTTP 请求解析失败 | 400 / `invalid_request` | 400 / `invalid_request` |
| 请求签名、world/cut/anchor 绑定拒绝 | 400 / `formal_read_rejected` | 400 / `full_sync_rejected` |
| 当前 audience、V1 grant 或 V3 target/lease 实际拒绝 | 400 / `formal_read_rejected` | 403 / `read_grant_rejected` |
| runtime/publication 未就绪 | 503 / `formal_read_rejected` | 503 / `not_ready` |
| 原语义下缺少合格历史或越过可读历史界限 | 410 / `formal_read_rejected` | 410 / `history_unavailable` |
| 内部 I/O、缓存、编码、不变量失败 | 400 / `formal_read_rejected` | 400 / `full_sync_rejected` |
| 历史完整性、proof 或副本分歧 | 400 / `formal_read_rejected` | 400 / `full_sync_rejected` |

native `FormalError::HistoryUnavailable` 只承接缺少合格历史的明确来源；`Io` 保留为内部失败，hash/proof/副本冲突保留为完整性失败。现有 business CAS reader 提供保留 `FormalError` 的读取 hook；旧 String reader 的不透明 source 错误保守归为 I/O，复合 proof 计算用本次调用局部的 source 记录保留来源，无 source 故障的校验错误归为完整性失败，不跨请求共享错误状态。成功响应、proof、grant 和持久材料的编码保持；旧 native 客户端若不能解码新增错误分支，仍须失败关闭，不能把解码失败当成合格历史不存在。

内部分类不自动将旧拒绝升级为 500/503；上表明确纠正诊断中的 `node`、`grant`、`authorized`、`not ready` 等偶然文本命中。formal lookup 保留独立的 503 / `formal_lookup_unavailable`，resolver Err 不变为 `Ok(None)` 或成功 continuation。HTTP helper 保留 api_version、path/request digest 绑定、签名及消息长度限制；request digest 或 response signing 失败仍为 500 / `signature_failure` 的普通错误 DTO，不能声称所有失败都有签名 envelope。V3 gate 保持[原始证据验证、锁内状态比较、响应构造与释放前时间复查顺序](blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-provider-read-gate-v3)，响应构造错误原类穿过 gate；不新增重试、历史恢复写入或 effect-publication 失败后的副作用机会。

#### des-p2p-full-sync-checkpoint-capacity-v1

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4、O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3、O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b。当前[历史页链](../../crates/oasis7_chain_runtime/src/host/node_lifecycle/formal_full_sync_history.rs)从外部 C0 重新验证保留页：上限 65,536 页、4 GiB proof 字节、64 MiB manifest 链；[retained tree](../../crates/oasis7_chain_runtime/src/host/node_lifecycle/formal_full_sync_cas.rs)上限 65,536 节点，[provider](../../crates/oasis7_chain_runtime/src/host/node_lifecycle/formal_full_sync_providers.rs) 单轮总预算上限 512 MiB。这些是当前实现支持边界，不是长期规模承诺。另一路[observer/watch](blockchain/persistent-world-node-lifecycle.runbook.md#cli-与成员查询)的 32 帧/16 MiB cursor 重基仍依赖单次 C0→head inline 历史≤128 步；不能用 full-sync 页 API 证明 watch 已自动续页。

目标 successor checkpoint 是从原信任根导出的加速依据，绑定 world/chain/genesis、前序 checkpoint 与精确 head、位置/roots、profile/epoch/manifest 和 validator transitions、最低已确认水位，以及 snapshot/journal/retained material 闭包。验证必须证明包含原 head/authority 与 minimum，且冷恢复可沿合法链到该 checkpoint 后 canonical replay 并核对 root；仅 provider 签名、高 head 或可回滚本机 pointer 不授予信任。

为避免每次从 C0 全量重放，已验证的 checkpoint 摘要及 minimum 必须再写入不可与数据盘一同回滚的受保护持久记录，或取得由原独立信任政策授权、可验证单调接续的外部更新证明。该记录保留前驱和原 C0 的认证关系，不替换 C0；缺少此条件则继续保留并验证原链，达到容量界限后停止推进。普通 fsync/rename 只解决崩溃原子性，不能承担防回滚认证。

安装顺序为 prepare 新材料 → 验证闭包/连续性 → 持久保存新 checkpoint 与受保护 minimum → 原子提交本机 active pointer → 验证可独立重开 → GC。若受保护 minimum 已前进而 pointer 尚未切换，恢复必须完成同一已保存 checkpoint 的安装，不能选择较旧 pointer；任一步材料缺失则 blocked，不降水位。GC 仅回收已证明可重建的热材料；冗余 archive 先完成重建/损坏负例检查，完整 canonical audit history 和唯一原请求依据不能被删除。

分块、streaming 和断点接续仍受每对象、总字节、证明深度及绝对截止时间预算约束，换 provider 不重置累计预算。状态输出版本化 limits、used/remaining、checkpoint age、重开预算和 lag；阈值前预警，触限保留历史只读并报 stale/catching-up 或 HistoryUnavailable，不能无限分配内存或把历史可读宣称当前可服务。长期接续、容量跨界和恢复 SLO 仍待实现验收，不因本节设计或已有分页代码而完成。

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

#### des-p2p-formal-bft-read-handoff-v1

Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39、O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46。首个正式 BFT 消费切片承接只读历史从受控 H 接续到 BFT H+1；本节是实现合同，不表示该路径已经交付。它不授予新的 Submit、Lookup、Changes、私有完整同步或 validator 签写权限，也不取消这些消费者及真实 custody 的后续义务。

交接验证从既有外部可信受控起点和原 qualified 历史得到 H，不从任意 peer 提供的 H proof 自建信任根。H 的 `ProfileHandoffV1` 必须指定精确的新 profile/version、目标配置 digest 及 H+1 位置。目标配置使用独立版本和域，绑定同一 world/chain/genesis、BFT wire 版本、新 authority epoch、完整初始 validator set 与 proposer/epoch 映射政策；不以本机 selector、operator 身份或安装 ACK 代替该承诺。配置 digest 不包含 H 自身的 record ID 或 H 的输出根，避免与承载它的 H/input/持久结果形成循环；验证后的激活能力另从 H 的已验证记录绑定实际 record ID、位置、原始 Snapshot root 和 governing manifest。目标验证本身不宣称已经恢复或重执行 World。

H+1 的初始 schedule 由该激活能力导出，parent 必须是 H。H 的 governing runtime digest 来自已提交记录，并与恢复 World 的 cognition runtime binding 核对；它不是 BFT 使用的 World manifest hash。BFT parent manifest 必须从 H 的认证 Snapshot 恢复后计算，与 parent semantic root 一样由新 QC 的 execution preimage 绑定；客户端仅验 QC 不等于独立恢复 H。受控 H 的 `after_state_root` 认证原 Snapshot 字节；先核对该原始摘要和 Journal 闭包，再从恢复后的 World 计算 BFT 所需的语义 parent state root，不能把两个不同含义的 root 互换。执行 tick 与 canonical position 分开：tick 从 H 所认证的 Snapshot/Journal 闭包恢复并核对，不把 canonical position 直接当成 simulation tick，也不按 BFT genesis 的 height 1 重新创建世界。现有 engineering V2 的 `parent_tick+1==height` 及 epoch→height 映射保持原义；正式接续必须另有明确的 position/tick 与集合生效映射，缺少适用映射时拒绝，不能仅放松旧校验或静默重置 epoch。普通 engineering V2 genesis 配置不能构造这项激活能力。新 ledger 格式固定原受控信任起点、H 及目标配置；重启重验同一激活关系和完整后继 QC/执行前缀，不把结构性最高记录当作已验证 head。

本节首个版本只支持恢复并核对后确实满足 `World.time == H.position` 的切点；这是显式支持边界，不是从 position 合成 tick。异值切点返回不适用映射的 blocker，保留 H 后旧权威停写，不回退或另建 genesis。初始 schedule 为 H+1、parent 为 H、parent tick 为恢复值、parent semantic root 为恢复 World 的重算值，proposer priority 采用初始集合的规范初值。目标配置与激活关系同时绑定这项映射规则；其他 position/tick 关系需要新的显式映射版本与回归，不能由这个受限读取切片宣称已经支持。

目标配置 digest 为 BLAKE3(`u64be(domain.len)` || `oasis7.formal.bft.profile_target.v1` || canonical target CBOR)。激活 digest 则为含固定 `domain="oasis7.formal.bft.handoff_activation.v1"`、schema 1、target digest、H head/world、H 原始 Snapshot root、H governing runtime digest 和 H intrinsic finalizing-authority ID 的 canonical typed payload 摘要；不包含 reader 路径相关的 authority context 或证明 envelope。相同 H/target 不因合法的历史读取或恢复验证路径不同而得到不同激活身份。

Runtime 只通过显式交接入口接续：由已验证 H/目标配置的能力与精确 schedule 共同校验 H 的保留历史 parent、实际 tick、scope、根和 manifest，再安装 BFT 执行阶段；H 的 retained-history reader 保留用于冷历史读取。实际 H Snapshot/语义根/manifest 的相等检查限于接续与执行开始前；tick 推进后，治理授权继续使用捕获的 H parent policy，并核对交接标记、retained parent/descriptor 与 schedule 的稳定绑定，不能要求已变更的 World 仍等于 H，也不能使用同批次新策略自授权。普通 engineering BFT 的 active-qualified-parent 拒绝规则不放宽，也不把旧受控 H 伪装成新受控执行 parent。独立 ledger 的 CAS pin 以完整 H/目标身份隔离分片，以显式节点数/累计字节预算记录 H 恢复与 H+1 重执行实际读取的索引节点，将校验后的节点与 Snapshot/Journal 一同保存并耐久 pin，再发布不可变提交记录；不能靠重启补 pin 掩盖已提交记录与 GC 间的丢失窗口。重开先在既有 ledger 锁下装载有界、内容寻址校验的依赖缓存，再以同一 H 重新恢复和执行；缓存不是信任根。该入口仍要求专用 CAS；[本地 retention 协调](distfs/distfs-production-hardening.design.md#des-dh-local-retention-coordination)使遵守同一协议的 pin 发布与 GC 串行，成功发布的 pin 不被过期快照删除。启用并发维护前须升级所有 CAS 访问者；旧进程、直接文件系统删除和不支持可靠 OS 锁的文件系统仍须排除。尚未 pin 的候选可被先行回收，安装必须报错且不能发布提交记录；调用者保留同一 QC/操作身份重试。该局部协调不替代完整 GC 闭包与保留资格。未被该固定执行/投影路径访问的冷历史仍由外部 retained reader 保留，这不证明完整历史的独立重建或全量 GC 条件。

H+1 仍使用现有 V2 的签名域、严格权重阈值、独立重执行及 QC 验证；profile 交接不生成一个内部 h+2 validator transition，也不混用旧受控 P/R 回执和新 validator 的票。接续只读 head 前，driver 从 H 的已认证闭包恢复隔离 World，重执行 H+1 的原输入并比较全部已有执行 roots，再持久安装精确 QC 与结果。仅有 QC、Snapshot hash、服务端声明或 transport 成功均不足以发布该 head。

engineering V2 的现有 execution hash 不承诺正式读取的投影树，不能沿用它认证投影。正式 profile 使用独立版本及域的 execution commitment preimage，保留原有 scope、height、parent、epoch、集合、policy、manifest、action/event/state 与 transition 绑定，并增加实际执行结果生成的 visibility-aware projection root 及 result root；其 hash 写入原 V2 block 的 `execution_block_hash`，由原 block ID/QC 承诺。preimage 不包含自身 digest、QC、当前 block ID/CommitRef，或由这些值派生的 root；parent CommitRef 可以包含，输出树须在当前提交身份封装前按既有非循环投影合同计算。只读证明携带该有界 canonical preimage，客户端重算 hash 并核对所有重复字段与 QC block，再验证请求 audience 的投影 inclusion。既有 engineering execution hash 不隐式升级，未经实际 World 重执行生成的投影承诺不进入正式 publication；读取证明无需披露完整私有 Snapshot。

该 preimage 的固定字段集合为 `domain="oasis7.formal.bft.execution.v1"`、`schema_version=1`、`profile="bft"`、`profile_version=1`、`activation_digest`、`scope`、`height`、`parent_commit_ref`、`parent_tick`、`parent_semantic_state_root`、`authority_epoch`、`active_validator_set_hash`、`schedule_policy_hash`、`manifest_hash`（parent）、`post_manifest_hash`、`action_root`、`event_root`、`execution_state_root`、`state_root`、`projection_root`、`result_root`、`transition`、`transition_ref`。使用仓库 `to_canonical_cbor` 的 typed value/map 排序及 self-described CBOR 编码，再取 BLAKE3 摘要；源代码字段排列不是另一种 wire 顺序。解码拒绝未知字段、非规范摘要和超限，并要求重新编码与原字节完全一致。`activation_digest` 固定已验证 H/目标配置关系；H+1 的 parent tick 必须等于该受限切点的位置，parent semantic root 与 parent World manifest hash 是新 QC 所承诺的执行声明，driver 必须先从 H 的认证闭包实际重算核对后才能发布，不能将客户端验 QC 等同于客户端独立重执行。结果索引沿用已有业务结果树合同，不另造只供 BFT 使用的结果索引含义。field 缺失、同义别名或旧域不得默认补齐后通过验证。

正式读取消费者采用显式版本化的证明和协商边界，旧 V1 wire 不增加可被旧客户端静默忽略的新权威语义。消费者先验证受控前缀和 H 交接，再逐高度验证新 profile 的 QC、集合链和投影承诺；response 的 profile、CommitRef、state root、投影及请求锚必须来自同一已验证 cut。原 public/private audience、持钥证明、撤销和释放前闸门继续约束原数据范围；新证明不把公开读升级为私有完整读取。未协商新版本、工程 scope 混入、错 target/parent/root/manifest、缺闭包或旧 profile 在 H+1 继续写入时均拒绝，不降级到 prototype 或 engineering verifier。

首个读取入口为独立签名的 `/v2/world/read`，仅接续到调用者事先独立固定的精确 H+1 `(height, block_id)`，不把 minimum floor 当成精确目标，也不接受 provider 自选更高 head。请求另绑定外部已验证 H 锚、目标配置/激活 digest、world、scope 和观察 nonce；响应重复核对同一目标，携带有界 canonical QC、正式 execution preimage 和投影 inclusion。该接口不宣称 latest/current/fresh；自动向后继高度追随需要另外满足适用的独立 checkpoint 有效期/时间授权合同。历史投影 World 不能充当当前私有授权 World：私有 audience 另要求可用的当前授权状态，缺失时阻断私有读取，公开投影仍按原公开范围验证。

主机提供两个显式入口：`bft-read install` 从有界 canonical QC 文件安装已被独立配置固定的精确 H+1，先验证 H/闭包/目标及证书，再由实际 World driver 重执行并持久保存；输入 QC 不能自行选择信任起点或目标 block ID。`bft-read serve` 只重开已有独立 ledger，重验该固定 cut 后提供 loopback 读取；不创建缺失信任或重新初始化账本。install 不生成 validator 票、不调用 Submit，也不代表网络写入或正式 validator 生命周期已接线。

连续正式 BFT 前缀扩展保留上述 H+1 合同，另以不可反序列化构造的验证状态绑定已认证 H、目标配置和激活 digest。版本 1 的完整 canonical 前缀同时限制为最多 64 个高度和 16 MiB 编码字节。首高度的协议 schedule 由 H 能力与首条正式 preimage 派生，发布驱动仍须从 H 实际重放并逐项核对该 schedule；每个后继高度复用现有 QC token 验证器，且只能取前一个 token 的 next-height schedule。每条正式 execution preimage 必须与对应 block、激活 digest、精确 parent CommitRef/tick、parent semantic root 和 manifest 相符；缺高、重排、冲突或混入 engineering 执行均拒绝。最后一个普通 QC token 不能单独代替从 H 连续验证的能力。该能力认证协议前缀，不认证 World 已执行、当前可服务、持有签名权限或读取新鲜度。

受控 `restore_snapshot_with_attached_formal_history` 继续只恢复精确 H 的原 Snapshot。连续 BFT 的隔离候选从驱动实际重放所得的父 World 派生；在保留原 H reader 的同时，先核对父 Snapshot 原始摘要、semantic root、manifest 与已验证前缀的 tip/QC，拒绝未提交 action/effect 队列或已有执行阶段，并核对隔离前后的 Snapshot/Journal 字节相同。该路径不接受外部 Snapshot/Journal 替换参数，也不给 H 的旧证明增加对后继状态的认证范围。

驱动在恢复时从相同 H 的认证闭包逐高度重执行，核对每条 commitment、实际 Snapshot/Journal、投影和结果树，再公开相应 cut。完整重放入口先验证整个有界协议前缀及外部精确 `(height, block_id)`，再从已恢复的 H 顺序执行全部高度；每步结果树使用刚重算的前一步树及其 root，不合成受控 CanonicalRecord 充当 BFT parent。合法 QC 但伪造执行输出的前缀也必须拒绝。该重放阶段只返回隔离执行结果，不追加旧 H+1 ledger，也不能生成带持久化含义的 publication cut。重放在独立的有界依赖 recorder 上用同一已验证 retained-parent 能力重建 H，保留原 release policy，并核对 H Snapshot/Journal 不变；结果树和 World 冷读都使用这项隔离 reader，缺页直达原 source，而不经旧 H+1 recorder，避免改变其未来提交的依赖集合和预算。每步匹配实际 commitment 后缓存新结果索引节点，执行结果持有累计依赖的 reader 生命周期。每个高度重新捕获刚重放出的父 World controller policy；H 的 reader 继续只承担受控冷历史读取，不能将 H 的 controller policy 沿用为所有后继高度的授权。ledger 对每个高度追加不可变记录并保留累计 CAS 依赖，拒绝截短、覆盖、跳高和用最高结构记录直接生成可信 head；旧单高度 ledger 不静默解释为新格式。完整前缀须同时有累计高度与编码字节硬上限，达到上限返回明确容量不足，不能丢弃旧前缀后继续。新消费者必须协商显式新版本、携带完整有界前缀证据并到达外部精确固定的目标；旧 `/v2/world/read` 仍只支持 H+1。这些接续能力不替代正式 signer/provider 安装、网络共识生产、checkpoint 时间授权或部署 readiness。

多高度 ledger 从已认证 H 与固定目标配置直接初始化，不要求旧 H+1 账本先存在；否则正式 producer 无法在首个 H+1 QC 产生前建立其执行起点。新目录、schema 与 required marker 独立解析，同一记录目录不并存旧 H+1 与新 prefix writer mode，双方启动检测均须拒绝混用或降级。不同目录中的只读 QC 导入副本不产生共识签名，分别持有稳定、持久的本地 ledger 身份；其 CAS scope/shard 绑定该身份与 origin digest，不能仅以相同 H/target 共用 shard 而覆盖另一副本的保留集合。正式 producer 的 Node 身份、受保护 signer fencing 与实际安装仍是另行接线的前置条件，导入 receipt 不授予这些权限。

所有当前 BFT 格式在检查、初始化各自目录前共同持有记录根目录的稳定 `bft-format.lock`，再持有自身 ledger lock，避免不同格式各自加锁后并发通过空目录检查。新 prefix 还在旧 `formal-bft-read-required-v1` 路径发布明确不受旧解码器支持的降级 fence；这不是旧 H+1 账本，旧入口不得据此重建或把新格式解释为旧格式。启用新格式前须停止并升级不遵守共同锁/新 marker 合同的旧进程；该 fence 不证明与已经运行的任意旧二进制可无停机混用。锁载体丢失、被替换或出现额外链接时阻断，不能以新建锁文件恢复一份既有 origin。

初始化先持久发布含随机本机 ledger ID、固定 H 材料 digest 与首高度的 `init-intent.cbor`，再通过同 inode 硬链接发布 origin，持久发布两个格式 marker 后才清理原 init-intent。完整初始化 staging 也固定同一原身份；重试不能另生成 ID 或换 H。已完成 origin 即使没有高度记录也要求两个 marker 完整；缺 origin 且仅余 marker 不能重新初始化。初始化未完成时不能已有高度记录；无法证明的状态保留并阻断。

每个实际高度在发布 CAS/pins 前先耐久保存绑定原 QC、commitment、parent-record digest 和全部引用的 `reservation`。材料及精确 per-height pin shard 全部核验后，将原 reservation 原子改名为 intent；再通过不覆盖的硬链接成为 record，同步目录后删除等价 intent，再同步目录。reservation 尚未发布 pin 时可重新执行同一输入以完成材料；原 shard 已存在时必须核验原 pins 与 CAS，不能重写掩盖损坏。intent 或 record 已存在后的缺失、损坏或不符 pin 均阻断。pin shard 找不到原 record/intent/reservation 身份时也阻断，不能从引用集合猜测或换用另一 QC。

恢复须在稳定 OS ledger 锁下识别原状态：仅 reservation 或 intent 为单链接 regular 文件；record 与 intent 同时存在时须为同 dev/ino、逐字节相同且恰有这两个链接；已完成 record 为单链接文件。已完整写入并 fsync、尚未 rename 的初始化或 reservation staging 固定同一原操作身份，重试先核对规范编码和原输入，不能盲删。只有实际重执行、精确 pin/CAS 核验及原字节相等后才提升或清理等价 staging；部分写入、额外链接、符号链接、冲突、未知文件或无法证明的中间态保持原件并阻断。不得按 PID 或超时猜测锁所有者，也不能直接重写部分记录。较早 cut 的隔离 reader 仅装载截至该高度的实际依赖，不能因账本已存更高记录而开放仅由未来高度保留的节点；整本账本仍须核验容量与既有材料完整性。

协议的 64 高度/16 MiB 上限不能代替执行材料容量。安装与重开还须核对 H 到目标的累计记录、CAS 依赖节点和字节硬上限，在 intent 发布前预检，并将已安装前缀计入同一容量核算；重试或增加后继高度不重置累计容量。固定 cut 的局部重开与 GC 用例只证明实际保留的重放闭包，不替代未访问冷历史的 archive 保留资格或完整同步预算。

主机通过独立 `bft-prefix install --config <prefix.json> --prefix <complete-prefix.cbor>` 和 `bft-prefix verify --config <prefix.json>` 接入上述实际安装与恢复。配置 schema 1 固定 `trust_config_path`、已由 H 承诺的 `target`、独立精确的 `exact_target.height/block_id`、`handoff_material_path`、`cas_directory` 与 `ledger_directory`；不接受 provider 选择目标。输入文件为 owned 0600，CAS 与 ledger 为既有、分离且 owned 0700 的真实目录。verify 不初始化缺失状态，可以在完整重放和 pin 核验后完成已保存的原 intent；它不接受替代 QC。实现为整条前缀设置 512 MiB 材料容量，包含编码/展开的 H、记录元数据和内容寻址 CAS 闭包；节点集合另受既有 4096 节点/64 MiB 索引预算约束。记录字节在解码前受累计读取上限约束，解码后的数量/引用、实际执行结果和材料容量在 intent 发布前核对；达到容量返回明确 blocker，不丢弃旧前缀继续。

两个命令的 `oasis7.formal_bft_prefix_install_receipt.v1` 是同一原安装 cut 的本机持久回读诊断，绑定实际高度、block ID、activation digest 与本机 durable record digest。外部消费者仍需独立验证完整 QC/正式 preimage 前缀；stdout JSON 不是新的外部权威证明。该入口不生成票、不启动服务端新 read wire、不宣称 latest/current，也不代签正式 signer/provider/Node 安装或全量状态可用性。

保留长期离线 checkpoint 的有效期、退出 key/stake 约束及独立 minimum 要求；完整重放不自动免除弱主观信任期限。缺少适用且独立认证的 checkpoint、时间或受保护 minimum 时，自动接续保持阻断，历史固定 cut 的离线验签与当前可服务/可投票资格分别报告。本机测试只证明实际使用的 provider、时间和存储接缝，不代签硬件、磁盘掉电或故障域验收。

回归必须包含真实 qualified 受控 H、实际 World driver 执行的 H+1，以及正式读取 producer 到客户端 verifier 的正向结果；同一场景拒绝 H+1 受控追加、错误目标配置/父引用/根/manifest、未认证闭包和冒充正式历史的 engineering V2。H 已提交而 H+1 未提交的停启保留同一交接和旧写入拒绝；H+1 安装后重开保留精确 cut，后继合法只读请求可继续。未执行的消费者、签写和真实环境义务保持未完成，不由该只读切片关闭 #4975。

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

#### des-p2p-devlocal-retained-commit-recovery

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607 ([AC-DWE-001](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-001)).

An ordinary DevLocal local decision can outlive the Node acknowledgement after its execution record is durable. Before executing that same local committed decision again, the execution hook may recover its original timestamp from the retained schema-v3 record only after checking the complete decision identity against its authoritative external-effect material (world/node/proposer, height, block, action root, slot/epoch and action anchors). The recovered time must also be used for subsequent replication and Node commit metadata. A conflicting or incomplete retained identity fails closed; no replacement action, timestamp rewrite or second execution is permitted. Direct execution replay continues to reject timestamp changes.

This narrow ordinary local recovery seam does not authorize network history, checkpoint-install records, legacy records, or guarded/formal continuations. Those keep their existing explicit original-context and proof contracts. It repairs continuation of an already durable matching decision; it does not make execution and Node persistence a single atomic transaction or certify an online repair.

#### des-p2p-guarded-local-initial-execution-prerequisite

Deployment is deferred by the user. The explicit `dev guarded-world --config <external-json>` engineering command installs the existing NodeRuntime execution hook and existing signed WorldService HTTP protocol. It does not activate the planned manifest-v2 formal profile, claim independent hardware fault domains, create an issuer/world/genesis identity, or enable Viewer/production-launcher consumption. Ordinary startup and ordinary client assurance retain their existing fail-closed boundaries.

Strict configuration version 1 has scope `guarded_local_execution_prerequisite`, `node_id`, existing private `node_keypair_directory`, private `records_directory`, loopback `status_bind`, `node_tick_ms` (1–60000), `trusted_bootstrap_config`, original `activation_evidence`, P/R directory paths, existing writer/P/R key-file paths, and an independently supplied `minimum_runtime_head`. Relative artifact paths resolve against the external configuration directory; the provisioned node-key directory must resolve to an absolute protected path. Existing activation/bootstrap verification runs before ordinary startup mutation. On Unix, P/R and node-key directories require current-user ownership and mode 0700; existing signing key files require current-user ownership, regular non-symlink files and mode 0600. Unsupported platforms reject this branch; it never generates keys or changes permissions of operator inputs. This config is independently authenticated operator trust input, not issuer authorization proof. The branch rejects external transport/provider/consensus overrides, constructs no libp2p/gossip/discovery/replication network, uses the existing single-local-validator decision machinery, and admits only existing authenticated WorldService routes on loopback. Public CAS/checkpoint/observer and legacy submit handlers are absent from this branch.

The Node captures the complete original local decided context, timestamp, predecessor hashes, action reservation, slot/height counters and actual stake totals before invoking the hook. Durable private continuation precedes execution. `Deferred` retains that exact decision and blocks successors; it never substitutes a network decision or reruns proposal/attestation evaluation. Candidates restore detached World::from_snapshot state, preserving no published-world persistence binding. Original signed WorldService input, complete Snapshot/Journal, result ledger and effect material remain exclusively in private operator storage. No candidate read cursor, external dispatcher or checkpoint export is available before qualification. A crash after execution starts but before a complete candidate becomes durable leaves recovery blocked: it fails closed without automatic executor replay, replacement nonce or repair. Only a retained complete original candidate supports the tested Unknown-to-restart continuation; formal PRD §9.2 service recovery remains unfinished.

The versioned generic nine-role closure preserves the existing ScheduleRecipe capture contract. Input holds exact original Node actions including signed request bytes; Manifest binds the original continuation, predecessor checkpoint and separate before/after simulation ticks. Snapshot/Journal include exact predecessor objects and complete journal-prefix continuity. Rules/WASM use concrete existing policies, module registry and module bytes; nonce closure explicitly covers capability nonce records, authenticated gameplay nonces, WorldService results and agent intent ledger. EffectOutbox includes existing pending/inflight state and original execution-effect material. Result and ExecutionHashPayload bind actual chain checkpoint height independently of simulation tick. Existing closed-record, Observer and capture budgets remain unchanged.

P/R `Unknown`/`Pending` retain the same request, ClosedRecord and candidate without successor execution. Qualification independently verifies both receipts, exact package/result/root/module closure and predecessor ancestry before immutable publication. The durable published checkpoint is saved before `Applied`; original continuation remains after Node completion for crash recovery. Restart reconciles the exact retained request against real locked endpoint status/head and the complete immutable published prefix. Missing acknowledged tails, changed config, alien heads and ordinary replication rollback/checkpoint/synthetic-decision recovery fail closed. An in-process monotonic verified-head fence prevents current HTTP reads from regressing after tail deletion; explicit historical pins still verify the entire prefix and exact requested CommitRef.

Graceful stop uses one caller deadline for admission-gate acquisition and Node-worker waiting; it never renews the worker budget after waiting for admission. Stop intent atomically clears readiness and sets stopping before waiting, preventing a new submit from passing its ready check. An already admitted operation may finish under the gate; the Node stop worker is created only after that gate is acquired and released. Gate acquisition uses deadline-aware polling rather than an unbounded mutex wait. Expiry returns retained `Pending`, not successful stop, and does not start a late worker; the runtime, ingress owner and original inflight work remain owned for a later bounded retry. An existing stop worker is never replaced. An unstarted/already-stopped owner without a Node worker does not acquire the active-Node barrier, but still performs its real delivery/server cleanup; `stop(0)` remains idempotent when cleanup is complete. Poison and true worker errors remain errors. This bounds deliberate waits, not OS scheduling or stderr latency, and does not promise every workload completes within one interval. Verification uses the controlled real-owner admission/worker regression and checks completion only after the same owner is actually stopped.

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

本节规定 `controlled_single_authority` profile version 1 的正式发行与消费协议；
已有 native formal 子范围按[持久节点手册](blockchain/persistent-world-node-lifecycle.runbook.md#正式合同层与准入边界)核对，完整部署与各消费面的实现状态分别取证。它沿用两端完整持久判定，规定正式激活链、proof 编码、不可绕过的 signer/storage
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

#### des-p2p-controlled-fault-domain-acceptance-v1

Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39、O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4。P/R 的双端回执证明受信端点作出持久声明；实际抗故障能力由部署事实及故障注入共同证明。inventory 记录 P/R 的主机、存储、恢复/回滚边界和依赖的 signer/provider、单调 floor、issuer witness；至少证明声明中的一个主机或存储域失效不能同时毁损两份完整 qualified 历史。共享电源、区域、管理凭据及备份体系的相关风险须披露；未声称覆盖的同时多域毁损不外推为已保障。

nonexportable key custody、签名决定/epoch floor 的防回滚和独立 issuer witness 必须在真实 provider 上验证。进程锁、两个目录、不同公钥、远程 capability 字符串和本机 mock 均不能替代。provider/witness 不可用或完整性存疑时停写，既有可验证历史可只读；恢复不得退回 exportable key、本机计数器或自授 issuer。

| 故障窗口/动作 | 必须观察的结果 | 判定材料 |
| --- | --- | --- |
| P 或 R 任一真实域损毁；断电/强制退出后冷启动 | 已向用户确认的每个 CommitRef、root、原请求结果及闭包均可由幸存域恢复；缺另一端时无新 committed 回应 | 同窗口域 inventory、确认结果清单、幸存历史/闭包、冷恢复 root 与 receipt 对账 |
| 冻结前、冻结完成、候选写入中、决定/proof 写入中、qualified 后但回应前分别中断 | 未固定 pending 才当前重审；固定 continuation/候选/决定按原身份和原输入续办，回应丢失不创建新请求，重复调用不产生第二结算、LLM 调用或 outbox effect | 各持久 seam、原 identity/nonce、冻结输入/工件、决定与 oracle 对照、效果计数 |
| 旧 writer/signer 仍存活，旧盘或凭据副本在接管后重连 | 最终 signer 与存储入口实际拒绝旧实例；新 epoch/head 不回退，未证明隔离时不进入新写入阶段 | 受保护 floor/lease、控制面隔离事实、旧签写实际拒绝和无新增历史 |
| plan/seal/activation 每次持久动作后故障或重试 | 原 recovery_id、原 tail 和唯一新绑定收敛；缺历史、原 pending、闭包或隔离证据保持 blocked | 前后 epoch/head、原计划/seal/activation、重试回执和拒绝原因 |
| provider/witness 暂停、回滚备份或复制实例 | 无新签名/提交，不降为本机弱实现；合法恢复后仍保留原签名决定与最低水位 | provider 实际响应、counter/witness 原记录、恢复前后同请求结果 |
| controlled 的 H 已提交、BFT 的 H+1 尚未提交时故障 | 只恢复合法历史并保持停写；恢复 issuer、取消计划或旧 signer 均不能恢复 H+1 controlled 写入 | H 证明、交接边界、旧追加拒绝与零双写 |

该矩阵扩充既有 CA-R1/R2/R3、CA-F1/F2、CA-B1，不建立第二套提交规则。Ops 负责真实故障边界与注入，Runtime/P2P 负责原候选、proof/root 和拒绝事实，QA 按同一 world/配置/工件/窗口判定；本机协议测试与真实域演练分别出证。安全与可用性分开：单域故障后已确认结果不丢失可以成立，同时新写入因双端条件不全而暂停。本文不预填 RTO、实际 provider 能力或运行通过结论。

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
| [professional_acceptance: p2p-target-bft](prd.md#p2p-target-bft) | [des-p2p-target-bft](#des-p2p-target-bft) | Propose/Prevote/Precommit仅verified >2/3 active stake cert生效；world/height/round/phase/roots/set和dedup签名绑定 | [testing-manual.md](../../testing-manual.md)；S9A full target场景：错误签名/阈值/集合/round 与缺证拒绝；矛盾票不重复计权，单个成员 equivocation 不阻断仍足额合法的证书，也不触发本地删 stake；锁定/解锁、timeout/new-round、transition、partition/heal/restart；prototype不能证明。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [BFT 证书闸门](prd.md#p2p-target-bft) | [轮次与锁定](#des-p2p-bft-round-lock-v1) | 足额真签、锁与 round 保持唯一历史；活性只在声明条件成立时验 | BFT-R1，固定集合状态机与本机 NodeRuntime integration 已实现；[S9A](../../testing-manual.md) full 同候选继续覆盖 q−1/q、三等权两签拒绝、mixed-value Any-q timeout 与 same-value decision、nil、错 proposer、伪 validRound、相互矛盾 proposal、乱序/分区/heal、旧 round 有效 cert；无网络进展与恢复同步分别判定 | 状态机规则/向量、签名集合/权重、消息调度、唯一 committed roots、模型反例或通过范围 | 本机与集成测试不等于形式化证明、正式 profile 或完整运行验收 |
| [BFT 证书闸门](prd.md#p2p-target-bft) | [签名与恢复](#des-p2p-bft-signing-recovery-v1) | 决定/锁在签名前持久；原签名重发，冲突与回滚停签 | BFT-S1，V1/V2 已接入远端 signer/provider 与签名前 journal；真实 custody 与防回滚验收仍缺，沿 [testing-manual.md](../../testing-manual.md) S9A 的同候选 full 边界继续检查 WAL/reserve/signature/fsync/广播 seam 中断，重启同槽同/异字节、nil→非 nil、旧盘/复制实例、WAL 缺失和 provider 不可用；固定同 key/世界与真实 custody 组合 | durable 决定、签名返回/广播顺序、受保护水位、零冲突签名及恢复 blocker | 普通进程重启和内存 backend 不证明防回滚 provider；远端协议接线不代签真实硬件 custody |
| [BFT 证书闸门](prd.md#p2p-target-bft) | [集合转换](#des-p2p-bft-validator-transition-v1) | h/h+1 旧集、h+2 新集；ACK 不改变全网边界 | BFT-M1，V2 工程路径已提供本机 Node/Runtime/provider 集成回归；完整验收仍沿 [testing-manual.md](../../testing-manual.md) S9A 的同候选 full 边界新增添加/移除/轮换/stake 更新、重复 key/空集/溢出、第二 pending 排程、h/h+1/h+2 重启分区、错 set cert、迟到/丢失 ACK、旧 key 与过期恢复 checkpoint；固定同治理决定及链，现有手册不表示这些新场景已执行 | 决定 CommitRef、排程/安装 ACK、逐高度 set hash/权重、旧 key 拒绝、停签与恢复事实 | 治理 CLI 查询不等于安装；不代签 controlled→BFT 的独立交接 |
| [professional_acceptance: p2p-target-recovery-trust-chain](prd.md#p2p-target-recovery-trust-chain) | [des-p2p-target-recovery](#des-p2p-target-recovery) | manifest/genesis→cert/header+set transition→hash snapshot→canonical replay→root→serve/vote；每环缺失/冲突/回退/异world停止 | [testing-manual.md](../../testing-manual.md)；S9A state-sync/full restore：逐环负例、snapshot/replay/state-sync/pruning/drill；同world+samecandidate+同窗口材料，readonly历史与writable闸门分判。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [完整同步只读资格](prd.md#p2p-full-sync-read-only) | [完整状态同步](#des-p2p-full-sync-read-only-v1) | 同 cut、minimum 和闭包完整后原子发布，不自授权限 | FS-R1，现有[有界 full-sync 测试](../../crates/oasis7_chain_runtime/src/host/node_lifecycle/formal_full_sync_tests.rs)仅作为局部基线；planned 同候选 full 组合补错 C0/旧 minimum/缺 blob/坏 manifest/provider 切换/授权失效，核对原 head 保留与无 writer/vote；本次未运行 | 原/新 cut、页链与闭包校验、实际读权 verdict、只读状态和错误原因 | 本机有界测试不关闭长期容量或部署验收 |
| [长期同步容量](prd.md#p2p-full-sync-capacity-checkpoint) | [checkpoint 与容量](#des-p2p-full-sync-checkpoint-capacity-v1) | 接续不换根、不回滚；容量触限保留 verified 历史 | FS-C1，planned full storage/restore；沿 [testing-manual.md](../../testing-manual.md) S9A 新增合法历史跨现有页/字节/树上限，跨 epoch/profile/manifest 接续，伪 checkpoint/缺旧 head/minimum、安装各 seam 崩溃、旧 pointer、archive 缺失/损坏、GC 前重建、换 provider 累计预算与冷重开 SLO；现有手册不表示这些新场景已执行 | 版本化规模、使用/上限、受保护最低记录、冷恢复 roots、archive 重建、重开耗时与 stale/blocked | 当前页链和 watch inline 不证明该目标；本次未运行这些场景 |
| [单权威提交与恢复](prd.md#p2p-single-authority-durable-commit) | [实际故障域验收](#des-p2p-controlled-fault-domain-acceptance-v1) | 已确认结果经任一单域损失可恢复，缺双端条件停写 | 复用 CA-R1/R2/R3、CA-F1/F2、CA-B1；[S9A](../../testing-manual.md) 与真实独立域演练按本节六行矩阵逐项注入；固定世界/工件/provider/配置与窗口 | 域 inventory、qualified 清单、原请求结果/roots、旧签写拒绝、无第二效果、部署 blocker | 本机目录、mock 与协议测试不可代签实际域或 RTO |
| [professional_acceptance: p2p-target-role-isolation](prd.md#p2p-target-role-isolation) | [des-p2p-layer-authority](#des-p2p-layer-authority) | 治理validator与permissionless服务、light/archive角色隔离；验证材料而非operator身份；prune重建+archive条件 | [testing-manual.md](../../testing-manual.md)；S9A full：compromised服务/错误角色签名不能推进，archive失联禁止未证明prune；Ops同窗口inventory/role与runtime验证oracle。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-service-gate](prd.md#p2p-service-gate) | [des-p2p-target-service-gate](#des-p2p-target-service-gate) | verified-serving→stale/catching-up→readonly→serving；proof冲突→isolated；非serving新intent receipt0、历史不改 | [testing-manual.md](../../testing-manual.md)；S9A + GWSC full同candidate：当前manifest/head/finality逐项负例、三恢复状态+gate rollback；receipt0/1与consumer blocker/next-step必须实际linked附件，现模板不足。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| [professional_acceptance: p2p-replication-recovery](prd.md#p2p-replication-recovery) | [des-p2p-replication-scope](#des-p2p-replication-scope) | writer/epoch从seq1及guard不污染；权威恢复错误与qualified storage-challenge fallback区分 | [testing-manual S4/S9A](../../testing-manual.md)；exact local test source `crates/oasis7_node/src/tests_fetch_blob_chunking.rs`；existing fetch_blob_storage_challenge_empty_provider_routes_probe_bounded_generic_route、fetch_blob_storage_challenge_without_provider_lookup_does_not_probe_generic_route；node局部fixture，不是DHT fetch fallback或QC证明。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | target实现与full组合未证明；外部pending/lineage/manifest/消费者字段未闭合 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[product_requirement: req-dcs-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001) | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | 验证certificate及hash-bound材料，拒绝服务/cache/peer自授authority；runtime state apply仍外部 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[product_requirement: ac-dcs-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) | [des-p2p-receipt-finality](#des-p2p-receipt-finality) | 验证certificate及hash-bound材料，拒绝服务/cache/peer自授authority；runtime state apply仍外部 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: req-dcs-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) | [des-p2p-target-recovery](#des-p2p-target-recovery) | sameworld恢复链每环验证；历史只读与重新服务闸门分开 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: ac-dcs-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | [des-p2p-target-recovery](#des-p2p-target-recovery) | sameworld恢复链每环验证；历史只读与重新服务闸门分开 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[product_requirement: req-dcs-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) | [des-p2p-layer-authority](#des-p2p-layer-authority) | governed validator与permissionless服务隔离；公网IP非通用前提 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[product_requirement: ac-dcs-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-003) | [des-p2p-layer-authority](#des-p2p-layer-authority) | governed validator与permissionless服务隔离；公网IP非通用前提 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: req-dcs-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004) | [des-p2p-target-bft](#des-p2p-target-bft) | verified >2/3 active stake证书才推进；错集或缺少合法 round 证明拒绝；矛盾票不重复计权、不使合法证书失效 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: ac-dcs-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-004) | [des-p2p-target-bft](#des-p2p-target-bft) | verified >2/3 active stake证书才推进；错集或缺少合法 round 证明拒绝；矛盾票不重复计权、不使合法证书失效 | [testing-manual S9A](../../testing-manual.md)；target full同candidate场景按对应DES；DCS001/004的required局部不降低根full组合。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | 只接受该P2P portion；target实现/执行/完整组合未证明 |
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

以下 CA 场景定义完整合同所需的验收范围，不把早期 planned 标签作为当前全部未实现的结论。
已有本机 formal/恢复子范围及其测试按[持久节点手册](blockchain/persistent-world-node-lifecycle.runbook.md#剩余整体验收)逐项引用；没有同候选适用证据的项继续未验证。
`controlled_live`、`replicated_protocol`、`controlled_history` 等本机回归不能代签跨主机
fencing、真实 nonexportable provider/witness 或独立故障域；逐项真实注入按[故障域矩阵](#des-p2p-controlled-fault-domain-acceptance-v1)执行。
实施时由 Runtime/P2P/QA 绑定同 source/integration/tested tree、world/config、执行工件和
环境窗口；单机故障注入与实际独立域演练分别出证。实际部署 hold 不因文档验收解除。

上述 §11.1 主表的 CA-R1/R2/R3、CA-A1、CA-C1、CA-F1/F2 与 CA-B1
分别承接本节八项目标关系；本锚点仅说明验证范围，不另建第二套映射。

## 12. 决策、长期风险与未决问题

选择保留现有专业合同并精确分配，拒绝以导航/历史标签免除真实设计，也拒绝将local证明升级为full。首个 BFT 明确采纳 Tendermint Algorithm 1、严格权重阈值、签名前持久决定及固定 h+2 集合转换；代价是持久写入、可证明的 signer 独占和集合安装期间可能停签。实现、模型/一致性向量及分区/重启对抗性验收仍未完成。长期 full-sync 选择保留 C0 认证关系和受保护最低 checkpoint，代价是归档、受保护状态与容量管理；不以本机 pointer 替代信任。pending/lineage/manifest/consumer字段及DC5 structured attachment仍外部未闭合。

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
