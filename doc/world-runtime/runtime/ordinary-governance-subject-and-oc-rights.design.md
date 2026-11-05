# 普通治理主体、事项 OC 权利与纠错执行设计

- 设计 ID：DES-WR-OG。
- 状态：active，目标专业合同；本文不表示账户、锁定、投票、纠错或正式入口已经实现。
- Owner role：runtime_engineer；producer_system_designer / gameplay_designer 审查原制度边界，blockchain_ops_engineer 审查身份、控制关系与 OC 来源，qa_engineer 审查方法。
- 固定来源基线：nil-studio/oasis7，2731f781536018a857e5daf2cfa7ede504e9cd69。当前源码事实与下述新 ordinary-subject-v1 制度版本分别说明。
- 上游：[GCB004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)、[GCB005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)、[GAME019 普通治理玩法合同](../../game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md)。

## 1. 问题、目标与非目标

普通票局中的 Agent、内部 stake 字段、外部 OC 余额和一次登录都不能单独建立普通治理主体。目标是在原共同决策权限内，完整生产经批准账户绑定的治理主体、实际控制人聚合、事项内线性权利分配、可撤回代表权、固定快照、有限纠错和原结果。世界资格与外部资产分别有来源；同一事项不能因转账、换 Agent、拆账户、换代表、重连或纠错产生第二份影响。

锁定对象是**本事项的权利分配**，不是新增 OC 托管、外部资产冻结或货币扣减。原 GAME019 要求外部 OC 仍可转让；同一事项的来源切点、每个钱包的独占分配及消费记录阻止转让后的重复资格。锁定、当前可投、已投、事项通过、获准执行与实际世界生效分别记录。

本文完成 GCB004/005 的专业生产和消费合同，复用[普通分类与宪制闸门](governance-classification-and-constitutional-execution.design.md#governance-classification)、[事项唯一生效](governance-classification-and-constitutional-execution.design.md#governance-commit)。不修改普通白名单、宪制保护、validator/finality、账户登录/custody、OC 发行/转账/桥定价；不把普通治理设为首局门槛。高影响保护动作由[独立保护合同](protective-governance-authority-and-recovery.design.md#protective-authority)拥有。本文不选择制度迁移试点，不新增万能审批、世界扫描或可改任意账本的管理接口。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | 经验证账户完整聚合 approved 玩家身份、managed signer、external wallet；冲突及私下控制断言可核，不以登录代资格。 | [来源与主体](#og-subject-source) | P2P identity/account registry；原授权 issuer | 不重定义登录、私钥托管或社会身份真实性。 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | 同一控制关系的线性锁定、单跳可撤委托及双重 cap 不因拆分、代理或循环而增权。 | [锁定与代表](#og-lock-delegation-cap) | 原事项 policy、原 OC 单位及范围 | 不把旧 sqrt/stake 算法称为线性 OC，不补政治默认值。 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | challenge、独立 review、适用 merge/revocation、计票 correction 与 appeal 保留历史并有有限结束。 | [纠错与申诉](#og-subject-review) | 有独立 subject-review / subject-appeal 范围的原 authority | 不借纠错取得资产、登录账户合并或高影响处罚权。 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | 私下控制证据与必要公开委托、计票、原因分层，当前读权和失败下一步可读。 | [读面](#og-read-model) | 原 Viewer / pure API / Agent 受众合同 | 不公开邮箱、密钥或全账户关联图。 |
| [REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5 | 同一不可变 source cut、原钱包独占分配、snapshot 与退出阻止转账/解绑后双投、追溯改票及额度重开。 | [快照与退出](#og-snapshot-transfer-exit) | 合格 canonical OC cut、binding/controller cut、原事项时界 | 不使外部 OC 不可转让，不由转账自动转移世界权利。 |
| [REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5 | OC receipt、事项权利及 operator-managed quota 三条独立状态；额度和 token_key 不影响治理资格。 | [桥隔离](#og-quota-isolation) | 原单向 bridge、chain receipt 与世界结果 authority | 不恢复 LetAI 开发 hold，不新增兑换、赎回或退款。 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | 主体/委托/纠错的业务身份、全写集与旧 World 隔离保持至多一次。 | [事务与恢复](#og-transaction-recovery) | 原 G1、正式 serving、source/权限与 module payer | 本地记录不代 canonical 结果；query 不重发效果。 |
| [REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5 | 锁定、snapshot、退出与重连不重开原额度或因未知结论推导未提交。 | [事务与恢复](#og-transaction-recovery) | 原 native nonce、世界 operation 与历史索引 | 不统一三个 authority 的 nonce/receipt。 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | 完整主体、受理程序与来源闭包有界，容量/费用不足在新承诺前拒绝，不截断当事方。 | [容量与费用](#og-bounds) | 真实 manifest / host / world 更严限额与原计费 | 工程上限不是已测规模、政治阈值或免费服务承诺。 |

### 2.2 直接来源与主责

[GAME019 §3–4](../../game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#3-普通治理主体与资格选择)拥有玩家循环、线性锁定/控制人上限、纠错及 OC/事项隔离。[P2P 账号模型](../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#model)和[身份授权上下文](../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#world-sessions)拥有 namespace、账户、批准绑定、安全版本与世界会话；[custody 身份分工](../../p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#4-目标身份模型)拥有 signer/external-account 的真实来源。该领域没有实际投产的来源不能自报为 approved。

[charter 双资格](regional-charter-authority-and-recovery.design.md#charter-authority-interfaces)消费本设计的 GovernanceQualificationRef，但仍独立核本地贡献；[情境声誉当前消费者](contextual-reputation-evidence-and-review.design.md#reputation-all-consumers)不把合同、claim 或战争累计兑换为普通治理 bonus。P2P 的[validator proof](../../p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-governance-authorization)不能任命本设计的身份或审查 issuer。

## 3. 当前状态、目标状态与差距

| 直接来源 | 当前事实 | 本设计目标及真实差距 |
| --- | --- | --- |
| [账号来源](../../../crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_account_identity.rs)、[账号 store](../../../crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_account_store_backend.rs) | hosted account/player 与登录恢复有自身合同；批准的全部 signer/wallet 及实际控制人 closure 不能由一个 session 推出。 | 新目标 adapter 验原完整 binding/source 及专用批准；不能用空列表或任意外部地址补齐。 |
| [当前权重](../../../crates/oasis7_world_state/src/state.rs)、[票局记录](../../../crates/oasis7_world_state/src/gameplay_state.rs) | 当前 Agent snapshot 使用 sqrt(stake_locked)、旧 reputation/10 和 cap 100；缺 profile 有默认 cap。 | 新制度只对明确采用它的新事项启用；账户聚合、线性原始单位与 controller cap 不能冒称当前算法已提供。 |
| [内部 profile setter](../../../crates/oasis7_runtime/src/world/governance.rs) | stake_locked 是内部 profile 字段，赋值不证明 OC 来源或独占锁定。 | 不把 setter、validator stake、restricted starter、vested 或 bridge quota 当本事项 OC。 |
| [原 MainToken 状态](../../../crates/oasis7_world_state/src/main_token.rs)、[货币写点](../../../crates/oasis7_world_state/src/world/main_token_monetary_publication.rs) | 原单位、余额、nonce、发行与转账独立存在；默认 symbol 为 OC、decimals 为 9。 | 精确 source identity 和原始整数单位来自其配置；显示小数不改计量。锁权不改变这些资产状态。 |
| [原 proposal publication](../../../crates/oasis7_world_state/src/world/governance_meta_publication.rs) | Open/Passed/Rejected 和同 voter 改票已有执行。 | 新主体 slot、来源 cut、委托与纠错必须经新 schema/adapter 接入；不能把新增名词写入旧 Agent map 即称兼容。 |

## 4. 边界与结构

### 4.1 制度记录与真实授权

记录属于受治理 L2 institution，沿用[Kernel/Institution 分类](../prd.md#native-surface-分类与冻结规则)。不新增 ordinary-governance 顶层 WorldState、核心 Action 分支或任意写者。原 module command 进入合法激活的 instance；reducer 返回自身 new_state 和一条结果 emit，Kernel/原 Host adapter 验来源、当前 capability、完整写集、payer 及根提交。module 自报 verified、自由文本或运维配置不能形成 authority。

正式授权复用已有可核链路，不以“有权者”形成循环定义：(1) 原合法制度决定给出本次普通治理 scope 和任命；(2) [CapabilityAuthorityRecord / CapabilityAuthorityFinalityProof](../../../crates/oasis7_world_state/src/capability_authorization.rs)将 issuer/key/epoch、完整authority record、world/branch、finality与原receipt绑定，沿[proof-bearing安装与单调撤销/替代/轮换](../../../crates/oasis7_runtime/src/world/capability_authorization_admin.rs)写入 AuthorityInstalledWithProof；无proof旧入口仍拒绝；(3) 该已验证issuer签[CapabilityGrantV2](../../../crates/oasis7_wasm_abi/src/capability_v2.rs)，精确绑定本专题module/version、namespace=ordinary-governance、object_kind=command、明确object_name、operation=execute、subject/owner-generation、audience world/branch/instance、entity/resource selectors、policy_class、payload界、有效期及当前撤销版本；(4) [trusted command](../../../crates/oasis7_runtime/src/world/capability_authorization_command.rs)核当前catalog/declaration/schema、issuer/proof、当前subject与audience、完整scope和commit前重验，制度module再核本案真实产品范围。technical finality只证明issuer链，不自行授予账户聚合、review或事项业务权限。

PolicyPublish、BindingImport、ControllerAttest、SubjectReview、SubjectAppeal、MatterOpen、Lock、Delegate、Revoke、Cast、Exit分别有本来命令和selector，不接受保护模块的role.bind/case.resolve grant替代。原[grant parent链](../../../crates/oasis7_runtime/src/world/capability_authorization_validation.rs)要求同issuer/subject/audience及scope/时长衰减；本设计的玩家代表权不能把origin的grant换subject发给代表。代表须由原合法issuer取得自己的精确Cast grant，并同时消费R/M中的实际delegation事实。账户session、节点登记、普通Passed或artifact安装均不补这一链。

一个 identity namespace 的受权 registry instance R 保存主体/控制关系索引及有限 review；一份事项由独立 instance M 保存来源 cut、分配、snapshot、ballot 与原操作结果。R 维护完整 active-matter 反向目录，同一受权 registry epoch 同时最多 4 个 M。普通主体更新只写 R；事项 snapshot/关闭写 R+M；跨主体纠错写 R 与全部受影响 active M，至多 5 次 invoke，必须同根提交，不能逐个事项先改一半。未完成事项不能搬到新 instance 以重开名额、权利或程序预算。

### 4.2 固定政策输入与可验证切点

ordinary-subject-v1 的 PolicyV1 在新事项接受前由原合法授权发布并固定：world/branch/genesis、原分类及 matter/revision/effect digest、registry/instance/artifact/schema/activation、identity namespace、允许的 identity/control/OC source issuer 与可验证签名 policy、具体 scope、原始 OC 单位、正整数 weight_unit、每 controller 的合法影响上限 C、原 voting window/quorum/pass threshold、资格 active/warmup 条件、完整有序时界及 review/appeal 的角色/阈值。

时界为 source_cut 已确认位置 C0、申请区间 [T0,Topen)、投票区间 [Topen,Tclose)、最迟程序结束 Tend；均为原世界权威 tick/position 的已声明转换，不用机器时间。C0 必须已正式包含且不晚于 T0；事项自身不能选择一个未来 cut 后按各人提交时间补截面。review 的答辩/初审/申诉/终审截止必须严格有序并不晚于 Tend，已受理请求不因重连或换案号延长。参数不合法、原授权不覆盖或缺来源时 policy 不可激活；不暗填 cap=100、默认一票、固定 24 小时或新的政治阈值。

数值单位和 C 是原合法制度 policy 的输入，公式、端点和缺项处理由本文冻结。验收 fixture 可明确采用 unit=1 个原始单位、C=100、C0=t10、T0=10、Topen=20、Tclose=30、Tend=40；这些不是新的生产默认、OC 价格或实际平衡。普通分类、财库/charter 消费和宪制确认仍走其原 authority，不由本 policy 自行扩大。

<a id="og-subject-source"></a>
### 4.3 approved bindings 与实际控制来源

BindingSetV1 由该 namespace 原 account/identity authority 生产，完整绑定 account_id、world_identity_scope、player identities、managed signer refs、external account bindings、每项 source kind/原 exact ID/批准及撤销版本、account policy/security epoch、有效区间、来源 commit/receipt 与完整性承诺。账户 owner 对新增关联按原敏感动作/钱包 possession 路径确认，issuer 验证后提交原批准记录；本 adapter 只能导入已批准结果，不能自行批准 external wallet、导出 signer 或以邮箱猜账户。某一类型合法为空可明确证明；来源缺页或未接入不是空集。

治理主体键为 (world_identity_scope, identity namespace, original account_id)。同一账户的全部批准身份/多个 Agent/多个设备只映射到这一个主体；binding 唯一索引按 (source namespace, kind, canonical exact key) 约束，摘要碰撞仍比较原件。一个 binding 同时指向两账户时保留冲突并阻断其新增资格，不先给两边额度。撤销/恢复/换 key 不分配新 subject，不清除原 matter/ballot/history。

ControllerClosureV1 是**专用且最小化的私下事实声明**，由原 policy 中获准的控制关系 authority 按具体证据/理由生成，绑定 complete subject set、equivalence-class ID、成员/排除依据、来源与复核版本、有效区间、受众和签名。生成过程读取全体相关批准账户声明与可验证控制事实，检查 issuer 的本次范围、各方同意/适用裁定、重叠 component 和未决冲突；canonical 排序后产生完整 closure。相同控制声明相交时必须求完整并集或拒绝冲突，不能按请求者提交的一个子集给第二个 controller cap。不能根据 IP、邮箱、共同组织标签、LLM 判断或“每账户默认独立”生成真实控制结论；也不承诺技术推断出未知社会关系。

资格 adapter 验来源原件、完整性及当前有效性，形成受信内部 VerifiedSubjectInput；客户端不能提交一个 verified=true 替代。原件持有者负责在合法受众内提供准确最小证明，普通公开读面只见资格结论、适用范围和复核理由类别。没有合法来源时返回 missing_binding_source / control_relation_unknown，不捏造高 cap；这表示未提供或未实施的目标来源，本文已规定 producer 的批准、输入、输出、校验和恢复合同。

R 的账户、binding、controller component 和 active matter 均有按 exact key/版本的完整索引。读取通过原受权 source cut 和完整目录进行，不扫描全世界猜遗漏；分页不能在第一部分成功后宣布 complete。导入分块只存不生效的 staged directory，最后核完整 root/count、唯一性及原来源签名一次激活；任何块缺失、重复冲突或当前来源撤销均无半份资格。

## 5. 关键运行流程

<a id="og-snapshot-transfer-exit"></a>
### 5.1 同一 source cut、锁定与事项快照

1. PrepareMatter 验普通分类、原 authority 与 PolicyV1；从**同一** C0 取得该 scope 的完整 eligible subject/binding/controller 目录和各真实 OC 来源账户的余额。目录按 policy 的资格谓词生成，不由提案者删选投票人。C0 绑定 chain/world/genesis、ledger/account、原 asset/单位、commit/state root 与合格 proof；同名外链余额或单独 transfer ACK 不替代该 cut。实际 source 若是外部链，原 P2P adapter 必须先验证其相应 canonical/finality proof 与允许的精确资产；没有此接点的候选明确 source_unsupported。
2. 对每个 source wallet 建立 WalletAllocation=(matter revision,C0,source namespace,exact wallet,original eligible subject,liquid_at_cut,allocated,retired)。只取合法液态部分；vested、restricted starter、他人 held/已扣部分和服务 quota 不算。别名/多 signer 同钱包共用一条 allocation。cut 后到账、新钱包绑定或新 controller 声明不能为本事项扩充分母。
3. LockIssueRights 在 [T0,Topen) 绑定该主体、原 wallet allocation、数量、完整预计 scope、policy 与 expected revision；同根验当前发起 authority、原完整 cut 可核、当前适用资格、未占/未 retired 的原来源额度。金额必须完整落在原 available 内，超额整笔拒绝，不裁剪。成功消费该 wallet 的本事项 allocation，生成稳定 lock_id；它不转账、不写资产 nonce、不抵押或冻结外部钱包。之后外部余额变化不重新生成本事项来源。
4. OpenSnapshot 只在原 Topen 边界消费完整已接受 lock set。当前必需 binding/controller/保护输入发生相关漂移时返回明确 blocked/stale，不按一部分仍可验证的主体悄悄重建快照。各已登记申请须有确切 accepted/withdrawn/rejected 结论；不明原提交先恢复。合法的零权重主体有明确原因，不是缺页的默认零。完整来源仍未就绪则本 revision 未开票，使用原政策的有界过期结果；改 C0、范围或窗口是新 revision，须原分类/批准，不迁移旧签署。
5. Snapshot 固定 C0、每 subject/controller 的原 q、完整分配目录、当前 policy 与 ballot key space；冻结不免除每个新动作的当前权限和保护检查。后续 transfer/unbind 不重算原权利、不移动旧票，也不允许新 owner 使用 sender 的 slot。
6. ExitIssue / UnbindGovernanceScope 撤销本主体未来使用/代表 authority，保留已确认票、原来源、退出 receipt 和 retired tombstone。本事项已经消费/退出的来源额度不重开，不能换 Agent/账户/instance/lock ID 回收；未投票的释放仅结束本事项权利，不生成 OC 返还或新余额。新事项的 C1 可在其原资格和批准下读取新的真实 owner/余额，不能伪称旧事项的恢复。

固定反例：C0 时 A 钱包100、B钱包0；A锁100后转给B100，B即使当前余额100也只能消费其 C0 的0。A退出后同事项重锁100、B晚绑定A旧钱包、先退出再反向转回，均不能重开 allocation。新事项 C1 的合法 B 锁定与旧事项原票是不同权利；跨事项不会把旧 receipt 当新增分配。转账失败不撤销原投票，锁定失败不回滚独立已确认转账。

<a id="og-lock-delegation-cap"></a>
### 5.2 线性额度、完整代表权与当前 cap

令 Ls 为 subject s 在本事项已合法锁定的原始整数总量，LG 为同一实际 controller component G 的各 Ls 之和，u 为本 policy 的 weight_unit。先在 G 聚合，再计算 PG=min(floor(LG/u),CG)，不按账户分别套 cap、sqrt、reputation 或最低一票。所有加乘除使用 checked u128；溢出/单位混用整项拒绝，不饱和到最大资格。

在完整 lock set 上按 Ls/LG 分配 PG：qs=floor(PG×Ls/LG)，剩余整数依 (乘积余数降序,canonical subject ID升序) 各加一，至总和恰为PG；LG=0时所有qs=0。不重建账户/改名以打破同项稳定排序。该取整不因拆账户扩大同控制人的总 PG；其他地方贡献/active/warmup 仍是独立条件，不从 OC 数量补齐。

ordinary-subject-v1 每个 origin subject、每个事项至多一个现任代表，代表接收该 origin 的**完整 qs**，范围仅该事项/修订/有效窗口，不再转委托、不拆出多个代表或借用登录 session。授权与撤回由 origin 当前原 authority 确认；代表同意其精确范围及真实费用/责任，取得 origin controller 来源不会变成其个人声誉或资产。所有更换为追加 delegation revision；同 origin 的 ballot key 始终为 (matter revision,origin subject)，不按代表或请求 ID 另开。

检查两个上限：origin 的份额不得超过其固定 qs/原 controller PG；representative controller还须对自身可用权利与所有外来代表权共同封顶。下列计算针对完整候选写集X，不修改原qs/PG：as=min(qs,当前适用subject cap)，当前资格/退出使其不可用时as=0；AG=min(PG,当前group cap,各as之和)，减少量不重配其他origin。VG是X后仍有效的本组origin ballot总量，包含已委托他人投出的票。自身占位baseG=max(AG,VG)，自身对外委托不从AG扣除。外来每origin在同一representative controller上取max(当前有效delegation可用量,仍有效旧代表ballot量)，加总为foreignG；这使同origin的grant和票在同controller只计一次，但换代表时旧controller的有效旧票仍占位。RG=baseG+foreignG，拟增加权限或换选项后的完整候选必须≤当前group cap；超限整拒，不按余量裁剪。来自同controller的代表只改变合法签署者，不重复加foreign。没有当前限制且自身权利未退出时AG=PG，正是原PG加外来权利的上限。

例：A锁80、B锁40，二者同G，u=1、CG=100，得 qA=67/qB=33；独立H的R锁60，PH=60、CH=100。R接B完整33后占用93，再接A67整笔拒绝；R同控制下第二账户不提供第二个100上限。若B的33尚未投票，撤代表立即释放这33代表占用，但不释放 wallet allocation；若已投票，原 ballot/代表占位保留，直到原合法显式替换、独立 correction 或事项关闭，不因撤回先释放再接下一人的票。

Cast / ReplaceBallot 核当前 origin/代表关系、实际授权、完整预期 snapshot/delegation/ballot revision 及原窗口。原规则允许改票时，用同 origin key 追加 revision，在单根里先移除旧有效贡献再加入新贡献；旧 receipt 和原选择保留为历史，不能产生第二份票。原请求丢失响应只查同一 revision 结果。转账或解绑本身不触发改票；代表失权后不能用旧签名代投，origin 仍须当前可验证 authority。

当前新动作还消费[有期限保护限制](protective-governance-authority-and-recovery.design.md#protective-action-catalog)：有效 scope 内取原合法 cap 与所有 restriction cap 的 min，freeze 为0；每条独立限制在到期/解除时只移除自身贡献。旧 snapshot q=7、已确认 ballot7 遇 cap3 时均不被自动改成3；单origin示例当前A=3/V=7/base=7，原合法显式同key替换7→3的完整候选为A=3/V=3/base=3，才可接受并释放4有效票占位，原q仍7且历史不改。新Cast/合法Replace必须requested≤当前as并满足完整候选组占用，超量整拒而非裁剪；旧整份delegation仍记原qs，但当前可用量受as限制，恢复限制也不自动投票。其他保留旧票使组占用仍超当前cap时，新的增加/换选项/重分配不能通过；不另设可用7→5旁路cap3的减少投票动作。原Revoke/Exit只终止自身后续使用或未消费代表占用，仍保留已确认ballot；合法读、撤回、退出、challenge/appeal不因新增权利cap=0自动被禁，其各自原权限仍需核验。

<a id="og-subject-review"></a>
### 5.3 重复主体 challenge、纠错与 appeal

ChallengeSubject 只提交原 subject/controller/事项范围、具体理由和有权使用的 source evidence refs；它不合并主体、不冻结、不扣钱、不改变票。重复请求使用同一 case identity；同 evidence/targets 在同一原 case 继续，不能另开案重置时界。无受理 capacity、真实 payer/预算、程序角色或合法证据入口时返回未受理及下一步，不伪造 review 已开展。

AcceptReview 由原 policy 单独授权的 subject-review authority 核管辖、最低材料、冲突声明、完整当事方和当前 source。申请人、被审主体、其实际 controller、直接经济关联者不得同时担任该案独立 reviewer；primary 与 appeal 所需成员集合/签名阈值由已合法发布的 policy 固定，不能由本案一方选择临时阈值。每个签名绑定 world、case/revision、精确输入/证据目录、拟 disposition、affected-matter 全目录、policy/source版本及截止；普通 vote、validator签名或安装模块不任命 reviewer。

受理后同根记录通知/答辩窗口、每方的有界完整回应槽、证据目录、primary/appeal时界、两阶段费用/关闭保留及 scope 内的程序性 review_hold。hold只阻受影响事项的最终化/新执行，不改变原票或无关世界活动；缺真实受理 authority 时不得挂 hold。通知由提交后的 durable outbox 发出，传输失败不重新建立案件。各方看到自身通知/理由/材料权限和原截止，消息重发不重置时钟。

每阶段保留全部当事方一次完整答辩及最多32个独立支持材料引用；可在截止前用同一 response/evidence slot 追加合法修订，仍服从完整程序预算，不用新ID续无限窗口。证据既可为原批准/控制断言的冲突，也可为经原 authority 核过的反证；相同文件 hash 不等于真实控制事实。材料不足、冲突来源不可核或独立签名不齐得到明确 not_decided / needs_evidence，不默认目标有罪。

可裁定 disposition 为 confirm_distinct、merge_governance_subjects、revoke_governance_binding_scope 或 correct_controller_component。merge只改变该治理 namespace 的 alias/聚合与计票投影，保留所有原 account/subject IDs、责任和历史；不合并登录账户、转移余额、接管 signer 或删 P2P binding。需要身份域真实撤销时，先取得该域独立合法结果，再消费其版本；本案不能冒充那一写入。不得用 correction 附带处罚或任意资金返还。

PrimaryDecision 是可申诉的明确候选，并锁定完整 correction candidate；它不提前写为有效tally/alias或成为事项执行依据。若在原截止前无人提出合法appeal，ConfirmReview才确认该决定；若已受理appeal，另一独立集合复核权限、证据范围及程序，维持、撤销或替换primary，所有原材料和决定仍可追溯。任何阶段没有合法决定，不构造merge/revocation。已受理case（包括primary未决或appeal未决）到原终审硬截止仍无合法结论时记review_unresolved，只移除本案临时hold与未生效candidate，保留未被本案改写的原记录及primary/appeal/失败原因；不能盲恢复旧投影覆盖后来合法binding/controller/其他案决定，已撤销/过期/解绑authority、当前cap及独立protective限制均照常重验。hold在硬截止的权威时间谓词上失效，不依赖caller继续付费或后台关案；原保留关闭预算负责追加终态与可读原因。缺响应不产生永久冻结或自动加罚。补证/下一合法事项/独立申诉入口可用性按真实authority告知，不能换case重开同案程序。

明确决定在投票结束但尚未最终化前，可对原 C0/lock facts 重新形成正确 controller closure，仍用§5.2公式，在 R+全部受影响 active M 同根安装 alias/资格、当前计票 correction、程序终态、费用及 receipt。纠正历史分配必须证实 C0 时的原事实有误；cut 后合法 binding/controller 变化只影响当前准入与后续事项，不伪作原分配错误追溯改票。重复 wallet 原额度只取一次，qs修正不能扩大完整原来源或重开retired项；新有效贡献最多为原 ballot requested 与修正qs的min，不自行选新选项或给从未投票者投票。原 ballot receipt 不改写，另加 correction 说明减少/保留了哪些有效贡献。

已最终化事项保留原最终结果和世界效果；追加 audit-corrected tally、原因、原/新来源及未来资格决定，不倒写 winning option、不再执行一次。上述同根写集仅含 R 与完整 active 目录内至多4个受影响 M；已归档事项不做全历史扫描或批量改写，R保留确切 source/case lineage 的已确认 linked决定，有权的单事项读取可用该决定和原件计算只读审计注记，仍服从同一来源/读取预算。尚未生效的通过事项仍服从原当前授权/保护重验；需要纠正既有效果只走原独立合法 redress，而非本案自行回滚。有效 appeal撤销纠错时，只恢复可证实的原 C0权利/投影，不造新wallet额度或第二个ballot；任何新的实际投票仍须原窗口和当前权限。

<a id="og-quota-isolation"></a>
### 5.4 OC、事项与单向额度桥分别恢复

[既有 quota bridge](../../p2p/token/mainchain-token-newapi-quota-bridge.design.md#bridge-recovery-state)仍为operator-managed单向服务，开发hold沿[原主责](../../p2p/token/mainchain-token-newapi-quota-bridge.prd.md#letai-development-hold)。外部转账、bridge_deposit_id/external_order_id/topup receipt、事项 lock/ballot/result 各有本来业务键；没有跨服务“共同成功”或统一nonce。

credit/topup成功只更新服务侧事实；token_key、额度数量、消费次数、套餐及provider缓存不进入 BindingSet、ControllerClosure、PG、snapshot、ballot、quorum或world execution。已credited但验证失败保留已入账/待核对，只按原order继续query verification，不再次充值。OC transfer失败不退回已确认事项，事项退出/拒绝不赎回OC或铸quota；bridge失败不清除合法治理历史。真实上游转账/模块费用仍按原合同发生，不能把本设计的隔离声明理解为全链免费。

## 6. 接口与数据合同

| 新目标记录/命令 | 完整身份、必要内容与producer | consumer及拒绝边界 |
| --- | --- | --- |
| BindingSetV1 / ControllerClosureV1 | 原identity/control issuer产生；namespace、原exact IDs、完整成员/唯一索引、批准/撤销及policy/security版本、证据目录、有效区间、受众、来源commit/receipt/签名 | R导入并由Host核验；不信自报verified、部分列表或未知schema。 |
| MatterSourceCutV1 | 原合法policy选择已确认C0；完整eligible/binding/controller目录和每OC wallet的原单位/原余额/source proof、全部root/count | M固定；后续新到账、source cut漂移和数据缺页不补额度。 |
| LockIssueRights / Delegate / Revoke / Exit | origin、完整scope、精确数量或整份qs、原source/lock/delegation键、expected revision、当前authority、有效窗口 | 仅自身合法范围；整笔匹配，不交集裁剪，不改变资产nonce。 |
| SubjectBallotV1 | matter/revision、origin slot、选择/requested weight、snapshot/delegation/current-authority版本、原签署和operation | 同origin单一当前ballot；历史append，代表不创造第二slot。 |
| SubjectReviewV1 / DecisionV1 | case、全部parties/证据/各阶段角色、disposition、affected-matter目录、截止、结果及appeal refs | R+所有受影响M同根；无无界传播或异步部分纠错。 |
| GovernanceQualificationRef | world/matter/revision、subject/controller、C0/snapshot、scope、原与当前cap/限制、active/warmup、source/authority/protection版本、receipt与期限 | charter、普通投票/执行消费精确当前范围；不代地方贡献、asset资格或宪制权。 |
| ReadOriginalOperation | 原业务身份/摘要与当前合法audience；读取原结果和相关root/source状态 | query-only，不生成lock/vote/review、world费用或外部调用；无query能力时保留unknown与人工定位。 |

source adapter 的目标实现从原已验证 World/source view读取有界原件，在Host内生成不可由客户端构造的 VerifiedGovernanceInputsV1，绑定本次action摘要、调用instance/artifact、C0/当前read-set和base generation。普通command只携带已验证紧凑投影及原件引用；Host必须在最终prepare对同一read-set重验，不能信任模块回传的source值。该adapter、其字段编码及正式接线是待实现目标，不假称现有 ModuleCallInput.ctx已有这些字段。

## 7. 状态、事务与持久化

<a id="og-transaction-recovery"></a>
### 7.1 原请求、全写集和 G1

业务operation键固定为(world,registry epoch,matter或case lineage,kind,origin subject,原业务slot/revision)；完整内容摘要进入原件。相同键同内容只返回原结果，相同键不同内容为identity_conflict；client_request_id、SDK invocation ID或新连接不能替代它。调用先验当前读取权限再查询旧结果；已提交结果不因当前写权过期重新执行。未有原结果时才验当前写权限、窗口、来源及完整候选。

R/M new_state、source/authority read-set、allocation/tombstone、subject/controller/ballot或correction、fee/hold、唯一operation结果、审计索引及outbox必须在原[ExecutionTransaction](../design.md#622-disposition-and-idempotency-contract)的一个root transaction提交。原签名/业务nonce按各自合同消费；读取认证nonce可有原有消耗，不把它混称新增投票或OC转账。跨身份服务/外部链是已确认source输入，不虚构跨数据库原子提交。

| 原持久提交判定 | 允许路径 | 不允许推断 |
| --- | --- | --- |
| NotCommitted | 丢弃完整staged变更和未成立hold；重试仍用原业务键，重新验当前条件 | 不能保留一半source占位或仅已扣module fee。 |
| Committed | 发布同generation全部业务/费用/索引并返回原结果；派生投影/通知落后只恢复原结果 | 不再次运行module、付款、投票或追认缺失半写。 |
| StatusUnknown | 按原G1保留完整候选/expected base generation，隔离整个旧World及其tick/writer，向原durable authority核同一提交的三态 | 不能把本地未落盘当canonical无决定，不能换新World/instance或当前snapshot推未提交。 |

普通提交后丢ACK但durable结果已确定，只恢复该精确操作；不把网络超时一律升级为G1全World故障。反过来，真实StatusUnknown不能仅围住一个matter而让旧World其他tick继续。权威恢复按原版本及完整原件验证；历史canonical apply可以重建原delta，但不发送新的实时请求/外部通知或重复计费。

### 7.2 可关闭状态及保留

registry import为staged/verified-active/conflicted/revoked；lock为allocated/snapshotted/retired；matter为source-preparing/enrollment/open/review-hold/finalized/expired；case为submitted/accepted/primary/appealed/confirmed/not-decided/review-unresolved/closed。名称是新制度状态，不重命名旧Open/Passed/Rejected或将旧操作自动迁入。

终态保存原业务key/digest/result、C0/源/版本、tombstone与完整历史目录；正文可按原受众进入原可审计内容保留链，热state只保留经核完整引用。只有旧matter所有结果可查询、程序已闭且来源/费用/退出索引完备，才允许compact并释放active slot；缺原件不是“已无权利”。新epoch只接真实closed head，不复制未完事项以重开4事项/程序预算。当前读权失效时仍保留内部历史，不向旧客户端继续泄露。

## 8. 部署、安全与运行约束

新schema须由原治理激活；身份issuer allowlist、controller review角色、OC来源及policy在已验证前态取得，本批新增policy不能给同批操作补权。签名用途域至少区分subject-source、controller-assertion、matter-lock、delegation、ballot、review-decision、appeal；完整exact payload、authority版本、nonce及范围都在签名内。合法签名只证明签者与内容，不能弥补不合法任命或来源。

调用沿原正式module提交/原authority执行，不增加public管理端点。hosted、native、pure_api、Viewer或受托Agent只在自己真实支持的路由中提交；mode/登录/操作员身份不能自动获得治理role。没有正式路由时可读真实blocked/source-not-ready，不能伪造动作成功。保护自己的原请求/审计仍按当前权限进行，不公开受限控制证据、长期signer、token_key、邮箱或无关账户。

<a id="og-bounds"></a>
## 9. 质量、容量与费用

### 9.1 先完整接收再承诺

这是有限工程profile，不能作为政治人数上限或宣称任意世界已适用。每R至多64subject、256个approved binding槽、64个controller记录、4个active M、2个同时受理case；每M完整scope至多64subject/256wallet allocation。每controller的完整closure必须包含全部相关subject；超出、目录未完或某个必要source超额，在新matter/case接受前capacity_blocked，不截断为“足够代表”。已有受理case预留所有当事方、两阶段和关闭容量。

每case最多64当事方；每阶段各方一个完整response槽、32个支持证据槽，原件按原受众完整保留，当前紧凑目录可在阶段关闭后引用已验证的原完整材料。完整原件读取每次root操作最多64件、每件16KiB、总1MiB，精确proof/身份/正文过大须在承诺前提供合法有界adapter或拒绝该来源组合；不能只存hash而宣称已验真。每次5-instance操作共用一次source预算与同deadline，不按instance重开；host源验证、staged内存及模块完整传输合计采用8MiB工程预算，实际更严配置优先。

| raw current state 分额（均含制度inner codec开销） | R 上界 | M 上界 |
| --- | --- | --- |
| 64 subject/source项 | 64×160B=10KiB | 64×384B=24KiB |
| 256 binding / wallet allocation | 256×128B=32KiB | 256×128B=32KiB |
| 64 controller / delegation | 64×192B=12KiB | 64×192B=12KiB |
| case / ballot当前目录 | 2×8KiB=16KiB | 64×256B=16KiB |
| 128热operation目录 | 128×128B=16KiB | 128×128B=16KiB |
| 4 active matter目录 | 4×512B=2KiB | 包含于下行 |
| policy/nonce/费用/历史根/编码余量 | 16KiB | 16KiB |
| 合计 | 104KiB | 116KiB |

两者raw state硬限128KiB。8KiB case目录包含64方×64B回应引用、32件×64B证据引用和2KiB程序/角色/决定目录；完整说明、exact身份和签名不塞进64B引用，而在已验证原件内保留。制度inner codec的摘要明确为byte string，不能拿未编码Rust struct大小代替；超过分额整笔不接受。热operation可在实际验证原journal/结果索引完整后归档，原业务tombstone仍可查，未归档完成时不覆盖旧结果。

### 9.2 完整 ABI 与实际下层限制

沿现[ModuleCallInput/ModuleOutput](../../../crates/oasis7_wasm_abi/src/lib.rs)和[canonical codec](../../../crates/oasis7_wasm_abi/src/canonical_cbor.rs)，Host的state/action/new_state仍是普通Vec整数数组；N≤131072时A(N)≤2N+5。新manifest完整input/output各512KiB，max_effects=0、max_emits=1；不改变现ABI序列化或公开HTTP/CLI原请求大小。

完整action是已编码ModuleCommandEnvelope，包含目标typed命令、真实签名/authority、source投影和payload的内层Vec编码，≤64KiB；普通payload≤31KiB且封套其他内容≤1KiB，A(payload)+1KiB<64KiB。action/event只能一份承载，不同字段重复携带时合计仍≤64KiB；outer ctx/键/flags等完整已编码额≤16KiB，不把ctx记0。

- input最坏：A(128KiB)+A(64KiB)+16KiB≤409610B；若两个数组分开承载同一64KiB总额，至多再加5B，为409615B。
- output最坏：A(128KiB)+一条完整已编码emit≤64KiB+其余完整封套≤16KiB，为344069B。
- 两者均小于524288B；真实encode仍须同时满足完整总额及每项子额，不能把raw预算当wire预算。五次完整input+output按512KiB双边界共5MiB，加共享source1MiB和2MiB staged/索引余量为8MiB；该预算不含更上层进程固定开销。

Host adapter将source核验后的紧凑投影装入上述action，不声称现有ctx字段已经支持；原件的有界读取与最终read-set重验同样必须实现。当前executor/Guarded v3的4MiB输出、64MiB内存不是本设计可自行提高的权限；实际world module_limits_max、Host、transport/公开请求和source读层任何更严上限优先。不能让512KiB内部封套放宽公开提交额度；可在原支持的受权原件引用/有限分页中完成输入闭包，不能先提交再异步补必需证明。实际限额装不下时该候选/来源组合不适用，须原合法版本化后再验，不默认提高全局配额。

### 9.3 有限程序与原计费

[原计费公式](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)保持：Data=ceil(I/1024)+ceil(O/1024)+2effects+emits；Electricity=1+effects+emits+是否new_state。preflight用真实完整I与manifest max_output/effects/emits，上述512/512保守reserve上界是1025 Data/3 Electricity每invoke；实际I按完整encoded bytes计算，实际O和结果同原counter计费，不把cap当计量。

正式trusted-command另有独立[capability budget计量](../../../crates/oasis7_runtime/src/world/capability_authorization_state.rs)：以真实envelope.payload长度、max_output、effects/emits预留units，并按实际output结算；它不等于Data/E或OC。31KiB payload、512KiB max_output、0effect/1emit的保守上界为544 capability units每invoke。原[执行路径](../../../crates/oasis7_runtime/src/world/capability_authorization_command.rs)同时验证这套grant budget与完整ModuleCallInput的runtime资源，任一不足均不得先提交其他账。目标多instance事务保留两种hold/settlement及原请求nonce，不能只补Data/E而遗漏capability ledger。

每M普通程序预留512次invoke、独立关闭16次；每个case单独预留primary128、appeal120、关闭8，共256次，包含decision最多5-instance的全部invoke。每R epoch最多4个已受理case（同时最多2个），不能关闭一个case后无限重新受理同一批源争点。primary每阶段64方回应+32证据及受理/决定/索引均计入；appeal独立额度不可挪给primary。R的来源导入/主体与控制声明维护另预留256次invoke、独立关闭16次，按最多16条完整来源项一批有界验证；256 binding+64 subject+64 controller的首次384项最多24批，其他单次变更也扣这个池，绝不挪用已接受事项/程序的关闭额。通知durable outbox、只读query不触发免费重复module调用；真实读取/服务成本仍归原服务合同。

完整R epoch上界为(256+16)+4×(512+16)+4×256=3408次invoke；保守reserve为3493200 Data/10224 Electricity及1853952 capability units。实际费用按真调用和序列化释放余量，不因未用预算、窗口结束或历史replay再扣一次。最多5次invoke的单根纠错保守上界5125 Data/15 Electricity及2720 capability units，各payer与各module的真实授权同事务核验；不存在自动第三方赞助。所有新epoch的来源维护、事项和case分别列真实授权payer和预算，不能把注册/导入排除在总图之外。

所有已接受事项/case必须有明确原Agent payer、合法scope和足够Data/E来源；提前保留最坏有限关闭/申诉额度，不发OC、quota或系统赠款。非法权限/重复原请求/纯查询在invoke前处理；已授权实际计算的收费按原runtime合同判断，不能从“没有投票效果”凭空推免费，也不能因恢复同结果再次收费。工程程序额度用尽只禁止新的未承诺操作；已受理两阶段及关闭继续使用原独立保留，不能删除程序或以无限延期抵赖。

## 10. 兼容、迁移与回滚

新ordinary-subject-v1不改旧Agent snapshot、旧sqrt/reputation/缺profile行为的历史解释。启用前必须同时具备合法PolicyV1、完整来源producer、Host/制度codec/adapter、所有consumer/current-read、原请求查询及G1恢复；只安装模块或展示B04/B05图稿不算启用。旧提案继续按原版本/原snapshot/原ExecutionSlot返回历史，不能以新账户聚合重新计票或复制一份新提案称作恢复。

当前新事项沿[既有声誉消费者政策](../../game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#governance-context-source)，不把合同/战争/claim来源加成带进线性额度。变更unit/cap/controller规则/source cut/schema须新合法版本，只作用于新事项；不能以“纠错”重新选择政治参数。回滚只退未生效候选；已经生效的主体/锁权/票及纠错历史按原版本可读、可恢复，不能删除索引、退费用或放开tombstone。

<a id="og-read-model"></a>
## 11. 玩家状态、验证设计与可追溯性

同一正式快照下Viewer、pure API与Agent读取等价事实：原账户聚合范围、仅自身可见的binding详情、当前controller资格结论、source C0/新鲜度、原/当前cap与不足原因、整份单跳委托及撤回边界、原票和当前计票、review/appeal状态、外部OC/事项/quota三态、原operation结果和下一步。公开面只显示必要的委托关系、最终计票/结果、纠错原因类别与原件允许引用；不输出完整controller成员表、个人控制证据、邮箱、密钥或token_key。

preparing/accepted/locked/delegated/voted/passed/effective分别表达，不把当前余额、界面数值或received ACK写成世界生效。来源缺失/失权/超cap/过期/结果unknown都显示相应补证、恢复原请求、等待原窗口、撤回/退出、申诉或独立事项路径；不能建议换ID重投。原链公开事实沿原公开API，私有World结果按当前audience，当前serving不可用时保留原结果未知，不据旧snapshot断言未提交。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | [来源与主体](#og-subject-source) | 全部approved bindings、完整controller closure及唯一索引；缺源不猜独立主体。 | [GOV-M03](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-03-subject-aggregation)；同候选required静态/adapter正反，full跨identity source。 | 原source/cut、主体目录、拒绝前后差分与受众。 | 不证明现实控制事实真实性或尚未接线identity。 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | [锁定与代表](#og-lock-delegation-cap) | 聚合后线性取整、双cap、整份单跳撤委托及旧ballot占位。 | [GOV-M04](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-04-lock-delegation-cap)；独立67/33/93算例和cap7→3；required，full并发替换。 | 原qs、完整cap占用、同origin ballot及确切当前差分。 | 不证明真实政治平衡、性能或当前票局已升级。 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | [纠错与申诉](#og-subject-review) | 真实独立角色、双阶段、完整受影响事项与一次correction，不改已确认世界历史。 | [GOV-M05](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-05-subject-review)；required正常/无authority/无quorum/appeal，full故障和跨matter。 | 各阶段原件、原/新有效tally、保留原receipt、全写集。 | 不代签真实审查人员或世界处罚authority。 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | [读面](#og-read-model) | 当前受众、公开/私有最小披露、旧票/新cap/下一步同时可理解。 | [GOV-M10](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-10-bounds-surfaces)；required三入口合同，full真实受众/运行。 | 精确候选读面及无敏感字段差分。 | 静态方法不证明真人理解或各入口已可用。 |
| [REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5 | [快照与退出](#og-snapshot-transfer-exit) | 同C0原钱包独占分配；前后循环转账/后绑定/退出不双计，不冻结外部资产。 | [GOV-M06](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-06-snapshot-transfer-exit)；required A100→B/C0-C1正反，full并发/重连。 | OC原余额/nonce、wallet allocation/tombstone与事项原件分别记录。 | 不代新外链proof或当前未实现锁权producer。 |
| [REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5 | [桥隔离](#og-quota-isolation) | bridge独立成功/失败/unknown不进治理数据链，无再次充值或赎回。 | [GOV-M07](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-07-quota-isolation)；required原桥语义/治理差分，full仅原获准环境。 | 三条独立结果及原order/query链。 | 不解除开发hold，不表示真实服务已运行。 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | [事务与恢复](#og-transaction-recovery) | 主体/代表/纠错全写集与原operation稳定，未知不换主体重发。 | [GOV-M09](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-09-recovery)；required重复/错摘要/迟到，full三态恢复。 | 原结果、fees、R/M根与旧World隔离状态。 | 不由静态推断durability、serving或跨节点通过。 |
| [REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5 | [事务与恢复](#og-transaction-recovery) | lock/退出/恢复不重开来源或nonce，历史apply只重建原状态。 | [GOV-M09](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-09-recovery)；required原键归属，fullG1/legacy replay。 | 三authority原业务键、原费用和完整来源目录。 | 不宣称所有现存schema可无迁移接入。 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | [容量与费用](#og-bounds) | 完整128/512KiB封套、最大程序/关闭保留及真实payer不漏算。 | [GOV-M10](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-10-bounds-surfaces)；独立算术/完整encode required，full实际manifest与下层取严。 | encoded I/O、完整source/费用表、超限前后和剩余保留。 | 工程cap及合成shape不等于正式规模、吞吐或预算已批准。 |

## 12. 决策、风险与剩余实施边界

采用不可变事项source cut与独占allocation，代价是本事项不会因后到款/新绑定临时增加权利；这是防前后循环双计并保留外部转让的必要边界，新事项仍按真实新来源进入。采用原controller聚合及完整单跳代表，不制造全局score、资产质押或无限代理链；资格事实不足要保留unknown。

本设计的主体producer、锁权/委托/快照、review/correction/appeal、当前保护消费、原请求恢复、有限参数形状与方法已给定。真实政治数值、issuer/角色任命和具体已授权scope来自原合法policy，不能由代码默认或fixture授权；在缺这种真实policy的候选中该功能保持不可执行，而非等待未来再设计算法。

实际identity/control来源、外部OCproof、Host验证投影、制度codec/module、原子多instance适配、正式入口、历史可用性及受众执行仍需实施和同候选验证。原始控制关系证据与review独立性还需真实来源证明，文档不能证明社会身份或实际公正。若修改线性单位、cap、时界、受理/appeal结果、完整scope或隐私披露，须由原专业policy owner审查与版本化，重核上述负例，不回写旧事项。
