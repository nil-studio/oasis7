# 完整世界历史与无损存储

节点运营者希望世界持续运行时少占磁盘；玩家回来后，仍应能核对原来的工厂、材料、委托和每次已确认结果。存储优化即使缩小了目录，若只能恢复最新画面，却丢失旧请求结果、未完成任务或继续行动所需的状态，就没有实现本专题目标。

## 文档身份

- 所属产品模块：权威世界基础设施
- 上位产品 PRD：[权威世界基础设施](prd.md)
- 生命周期：`active`
- 状态含义：本 PR 候选的目标规范；不表示已实现、部署或发行。
- Owner role：`producer_system_designer`
- Last reviewed：`2026-10-28`
- 适用入口：具有历史保留承诺的正式持久世界及其受权读取、恢复、迁移和节点运维；玩家、Agent、Viewer 与 pure API 通过既有世界服务消费结果。
- 专业域权威：[P2P 权威与持久条件](../../p2p/prd.md#p2p-authority-profiles)、[Runtime 恢复与服务](../../world-runtime/design.md#runtime-recovery-design)、[新格式增量状态与历史](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-scope)、[既有格式的存储效率](../../world-runtime/runtime/formal-world-storage-efficiency.design.md)、[验证规范](../../testing/prd.md)。
- 初始设计基线：`nil-studio/oasis7@4220622fde069346a828d42572eea61c0073d95e`；其静态审读属于初始文档证据，不能代替后续实现验收。
- 初始实现审读基线：`nil-studio/oasis7@2676c835cc50f12d7fd16a762eaa21f026ae79d4`；2026-10-17 静态源码审查。下列状态卡区分既有基础与新格式剩余范围；当前实现范围与接受边界见[效果与证据范围](#lws-evidence-scope)，后续 main 前进不自动扩大旧证据。

本文定义“在完整历史、原世界身份和合法资产连续的前提下减少存储”的产品结果。压缩编码、对象分块、索引、事务、协议、具体预算与测试命令由专业设计拥有；本专题不新增玩家收费、世界规则或公开能力声明。

### 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 设计适用性理由：本专题复用既有世界可用性、原请求核对和恢复语义；无独立玩家信息架构、配置流程或新交互入口，必要体验说明已在[使用者体验与决策](#lws-experience)展开。
- 适用边界：仅不另建重复的产品体验 design；九项技术义务仍由专业系统设计逐项承接。若后续新增独立存储管理界面或跨入口恢复流程，须重新审视该判定。
- 结构裁剪：理由：产品体验可在本 PRD 内完整表达；范围：只合并体验说明，不裁剪要求与验收；风险：新增 UI 时可能需要配对体验设计；映射：[体验与决策](#lws-experience)、[验收与证据](#lws-acceptance)、[取舍与未决问题](#lws-decisions)。

## 1. 使用者问题与目标

### 1.1 正常使用情境

玩家完成生产后离线，期间运营者压缩历史、缩小本机热数据，并将服务迁移到新节点。玩家再次进入同一世界，应看到原已确认成果和仍有效的任务；查询较早的操作可能需要等待归档读取，但不能得到“记录过期，所以重新执行”的结果。Agent 继续处理原委托时，不能重新调用已完成的模型请求、再次扣费或漏掉约定的下一次行动。

运营者可以分别选择近期历史的热访问范围和恢复检查点的数量、密度。较少热数据降低本机工作集，更多检查点缩短恢复时间；各自的容量、读取与恢复成本应可核对，两种选择都不改变完整历史及独立持久保障。

### 1.2 成功含义与完备性

“完整”包含以下全部结果；实现可以改变物理表示和放置位置，不能删去任何一项原有承诺。

| 保留内容 | 使用者能够核对的结果 |
| --- | --- |
| 世界身份与合法历史 | 固定的 `world_id`、`chain_id`、创世身份，以及其后合法版本和提交权威的演进连续；旧历史不会被新创世替代。 |
| 已确认状态与历史结果 | 当前资产、设施、资格和已确认效果不丢失；原请求、结果及因果关联可以按原身份核对。 |
| 继续运行所需状态 | 未完成工作、行动顺序、已确定的唤醒或暂停安排、授权与去重记录、待交付效果保持原语义。 |
| 历史解释与验证依赖 | 原输入、实际采用的模型或外部结果、历史规则和执行工件、证明及必要原始字节仍可取得，能够按原版本解释和验证。 |
| 独立持久保障 | 在所支持故障模型内失去一个故障域，仍能恢复已确认效果；同一数据的多处目录不冒充独立副本。 |

完整历史不要求永久保留每个高度的一份展开全量快照，也不要求每个角色在本机保存全部历史。必要内容可以共享或移入归档，但取回后须保持原证据字节、合法状态和连续性；只留下哈希、摘要或重新生成的解释不能代替被引用内容。

完整历史会随真实新增信息增长。本专题约束重复表示、临时材料和热状态的放大，目标不是把无限运行历史塞入固定总容量；达到预算时应扩容、调整有证据支持的放置策略或暂停新写入，不能缩短历史保留承诺。

### 1.3 已知基础与待验证假设

静态审读已看到通用本地内容存储的压缩、正式路径的共享保存、部分业务历史归档和正式双端持久接口。[既有复制检查点的存储放大证据](https://github.com/nil-studio/oasis7/issues/5196)说明，局部能力尚未覆盖所有运行路径。它们可作为新设计的基础，不能代替新格式下完整历史、持续运行和恢复成本的验收。各项来源与剩余范围见[要求状态卡](#lws-requirements)及[证据边界](#lws-evidence-scope)。

设计假设：去掉相邻状态、执行阶段和目录之间可证明的物理重复，能在保持历史不变的情况下减少用量。反证信号包括唯一内容占绝大部分、压缩收益小于新增开销、恢复成本超过适用预算或维护峰值无法承受；`runtime_engineer` 与 `qa_engineer` 按相同业务历史和负载对照验证，未测量前不承诺节省比例。

## 2. 范围与 Non-Goals

本专题覆盖当前正式持久世界的状态、历史、证明、原请求结果、恢复依赖和相关运维材料；包括物理共享/压缩、冷热分层、派生材料回收、格式迁移、存储压力及验收。九项要求是范围内可独立失败的能力清单，不再另设一套功能编号或进度台账。

<a id="lws-new-format-scope"></a>
### 2.1 新格式创建与既有世界维护

新格式创建阶段直接将持续执行所需的当前状态、完整追加历史和恢复检查点分别管理。少量状态变化应只增加相应变化及必要验证材料，已完成历史可以离开常驻工作集；首次接受之后的每个历史位置仍须可核对、恢复，重新提供服务时须继续原工作。[增量状态与历史设计](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-scope)拥有该新格式的技术合同。

| 适用范围 | 保留承诺 | 交付与验收边界 |
| --- | --- | --- |
| 新格式创建 | 从本世界首次接受起保留完整历史、稳定身份、原外部结果及继续运行所需材料。 | 按新合同合法创建和激活；无需读取旧格式数据、复现旧快照字节与标识、兼容旧程序或执行原地迁移。 |
| 既有世界维护 | 既有世界已承诺的历史、身份和已确认效果继续有效，包括采用旧式复制检查点的世界。 | 旧数据导入、迁移与处置属于独立范围；新格式创建的验收不证明既有运行世界已迁移，也不授权删除或重置它。 |

下列九项要求按上述范围适用。新格式自身的物理维护、历史读取、恢复和故障处理仍遵守相应要求；[同世界迁移](#req-lws-006)在后续实际变更已有世界时适用，旧格式导入与旧程序回退不作为新格式创建的前置条件。格式选择不改变原权威、独立故障域或服务资格。

### 2.2 Non-Goals

不在本专题范围内的内容：

- 改变共识算法、经济参数、玩家资产价值、收费规则或创世身份。
- 用新世界、清空目录、历史重签或重新生成模型回答来修复原世界。
- 承诺任意大小历史在一次请求内读完、任意设备都可保存全部历史、零恢复耗时或固定压缩比例。
- 强制记录未进入世界合同的草稿、任意调试日志或凭证；访问、敏感数据和保留义务仍由原专业合同约束，存储优化不扩大可见范围。
- 新增独立 UI、改变服务权威或未经验证删除正式历史。全新世界可以直接使用已实现的新格式；已有世界的维护仍遵循同世界迁移合同，现场部署遵循适用运维入口。

`dev_local` 等工程档位的短窗口不能覆盖正式世界的完整历史承诺。后续单权威到多节点的合法演进仍使用同一世界协议，上层按相同身份、请求、结果和可用性语义工作。

<a id="lws-experience"></a>
## 3. 使用者体验与决策

### 3.1 正常路径与取舍

| 阶段 | 使用者知道什么 | 可以选择什么 | 代价或承诺 | 可观察结果与下一步 |
| --- | --- | --- | --- | --- |
| 运行前与容量规划 | 当前世界、支持的恢复范围、容量和增长证据；未知项目明确标出 | 选择已支持的配置，预留容量或暂缓启用优化 | 较低热点用量可能增加读取/恢复成本；没有降低完备性的选项 | 运营者知道预算适用对象及未证明边界，再决定是否启用。 |
| 正常运行 | 原操作是否已确认、哪些历史已下冷 | 玩家和 Agent 按原流程行动或核对原结果 | 存储维护不产生额外世界结算或再次模型消费 | 同一请求返回原处置，世界成果保持连续。 |
| 查询较早历史 | 查询对应的世界、时间边界与授权范围 | 等待取回、按受支持范围继续读取或取消本次读取 | 冷读取可能更慢；取消读取不取消原业务或删除历史 | 返回原历史及完整性范围，缺证或读限额不伪装成“没有发生”。 |
| 优化与迁移 | 已验证范围、必要暂存空间、可回退边界与当前结果 | 启用已验证策略、等待、扩容或停止推进本次维护 | 新旧材料会暂时并存；回滚软件不撤销世界历史 | 明确维护是否完成、仍在进行、受阻或未知，按原身份继续。 |
| 故障后恢复 | 最后可信世界边界、当前服务限制和原因 | 查看可信历史、等待材料或请求授权恢复 | 恢复历史可读与恢复可写分别成立；不能承诺立即继续 | 条件重新成立后恢复原工作；否则保留只读或隔离。 |

### 3.2 主要失败与恢复

沿用[分布式共识与状态可用性](distributed-consensus-and-state-availability.prd.md#req-dcs-005)的“已验证可服务、已验证只读、陈旧/追赶中、不可用/隔离”，不新增第五套玩家健康状态。

| 失败或限制 | 使用者可见含义 | 允许的下一步与禁止误读 |
| --- | --- | --- |
| 归档读取暂慢或读取预算不足 | 本次请求尚未完整读取；说明已验证范围和真实限制 | 按受支持范围继续、等待或取消读取；不能据此认定原记录不存在。 |
| 新写入空间不足 | 新操作不能获得所需持久条件，或者原操作尚待核对 | 保留原请求、限制新写入并扩容/恢复；不能借清历史制造成功。 |
| 维护已提交、清理失败 | 新持久结果成立，容量尚未收敛 | 重试维护；不把原业务结果改成未提交，也不重复结算。 |
| 提交或迁移结果未知 | 无法确认精确原操作是否完成 | 保留原身份及两侧材料并核对；不换身份重做，不先发布旧状态。 |
| 必要历史、版本或证明缺失/冲突 | 原世界的适用结果暂不可验证 | 保留最后可信边界并隔离受影响能力；不能使用替代世界或补造材料。 |
| 副本失联或独立性不成立 | 当前正式持久条件未满足 | 恢复受支持的独立持久保障后重新核验；两个目录不能被显示为两个独立副本。 |

### 3.3 反馈与授权边界

玩家只需要知道原世界、原请求结果、影响范围和真实下一步。具体压缩算法、文件目录和索引维护不进入普通玩家流程。运营者可以看到容量、增长、维护结果与受限原因，但不能因此取得原本无权读取的私有历史。

状态不能只靠颜色或“完成”一词表达；当前结果、最后可信边界、未知/受阻原因应有文字或等价机器可读说明。原授权、目标切换、重连、重复提交与消费者隔离规则保持；若后续改变入口或交互，仍须取得该入口自身的体验与可访问性证据。

<a id="lws-requirements"></a>
## 4. 产品要求

以下均为目标要求。九个 Trace ID 仅在各要求处声明；已有上位要求继续使用原身份。系统设计状态只评价本专题的目标条款承接，不代签上位 BFT、部署、全部消费者或发行组合。

<a id="req-lws-001"></a>
### REQ-LWS-001：完整历史及依赖可恢复

- Trace ID: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1
- 性质：目标要求。
- 适用条件：正式世界已承诺保留的历史，从合法初始锚到当前已确认边界，以及尚未收敛的持久请求。
- 要求：任何存储优化后，系统必须保留足以恢复、解释和验证该范围全部世界事实与原请求处置的原始内容及依赖；状态、证明、控制安排、未完成工作或历史执行依赖缺失时不得声明完备。
- 理由：玩家成果、历史责任和继续运行能力具有共同的保留边界，单一最新状态不能代表全部历史。
- 上位承诺：[SC-3](prd.md#sc-infra-003)、[SC-6](prd.md#sc-infra-006)、[REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)。
- 专业权威：[完整保留集合](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-001)。
- 验收：[AC-LWS-001](#ac-lws-001)。
- 系统设计状态：已覆盖
- 系统设计依据：[完整性与依赖闭包](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-001)承接保留范围、历史版本、必要内容与缺证拒绝；[新格式历史读取与恢复](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-read-recovery)承接首次接受后的全部历史位置及恢复依赖。
- 实现状态：部分实现
- 实现与验证依据：[正式已发布记录](https://github.com/nil-studio/oasis7/blob/2676c835cc50f12d7fd16a762eaa21f026ae79d4/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/formal_writer_published.rs)及[正式归档读取](https://github.com/nil-studio/oasis7/blob/2676c835cc50f12d7fd16a762eaa21f026ae79d4/crates/oasis7/src/runtime/world/formal_retention.rs)有局部实现；初始设计交付只形成目标合同，尚无本要求完整运行证据。
- 核对日期与基线：2026-10-28；新格式需求为 `112a627e889cb0601ed368b2c0be204e39e7554a` 上本文候选；既有实现依据仍为上述固定 `2676c835cc50f12d7fd16a762eaa21f026ae79d4` 的静态持久/归档审读，未执行新格式 AC。
- 剩余范围：设计目标已承接；新格式从首次接受起的完整范围依赖清单、长期规模及逐历史位置恢复待实现验证；既有格式的多版本恢复继续按原范围验收。

<a id="req-lws-002"></a>
### REQ-LWS-002：物理优化保持原内容与身份

- Trace ID: O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c
- 性质：目标要求。
- 适用条件：共享、压缩、打包或重排任何仍被世界合同引用的内容。
- 要求：系统必须保留该世界已经接受的原始内容及世界、对象、请求、提交、签名与证明身份；后续物理优化须精确读回原字节，不能以语义近似、重新序列化、重签或摘要替换原内容，不能因共享扩大读写权限。新格式创建可以采用新的初始表示与标识；相邻检查点和事务的共同内容必须可共享，少量变化无需反复保存全部累计历史。
- 理由：节省的是物理重复，使用者原来持有的证据和结果仍然有效。
- 上位承诺：[SC-3](prd.md#sc-infra-003)、[SC-6](prd.md#sc-infra-006)。
- 专业权威：[原字节与物理布局](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-002)。
- 验收：[AC-LWS-002](#ac-lws-002)。
- 系统设计状态：已覆盖
- 系统设计依据：[无损共享、编码与兼容](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-002)规定已接受内容的物理转换和错误边界；[新格式对象模型](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-object-model)承接当前状态、追加历史与检查点的独立保存及跨代共享。
- 实现状态：部分实现
- 实现与验证依据：[正式路径物理共享](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_distfs/src/controlled_authority/formal/physical.rs)已有按原内容读回的局部能力；[复制检查点存储](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_node/src/replication_checkpoint.rs)仍保存完整对象。它们不构成新格式当前状态与追加历史分离的交付或验收。
- 核对日期与基线：2026-10-28；需求为 `112a627e889cb0601ed368b2c0be204e39e7554a` 上新格式设计候选；实现为同一固定版本的上述路径，静态审查，未运行新格式场景。
- 剩余范围：设计目标已承接；新格式独立状态与历史、跨代共享及其自身原内容/证明完整性待实现验证，旧格式转换不在新格式创建范围。

<a id="req-lws-003"></a>
### REQ-LWS-003：冷热分层不缩短历史承诺

- Trace ID: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670
- 性质：目标要求。
- 适用条件：改变热窗口、检查点密度、节点保存角色或将历史移入归档。
- 要求：策略变化只能改变保存位置、访问成本和恢复加速程度，必须保持全部承诺历史的合法可读取与可重建性；旧请求、控制链和已确认结果不因离开热窗口变成不存在、失效或可重复执行。新格式必须支持将已完成历史移出常驻执行工作集，同时保留当前授权、去重和未完成工作；近期历史的热访问范围与恢复检查点的数量、密度分别控制和核对。
- 理由：运营者可以交换空间与时间，玩家的历史权益不随本机档位改变。
- 上位承诺：[SC-2](prd.md#sc-infra-002)、[SC-3](prd.md#sc-infra-003)、[REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)。
- 专业权威：[冷热分层与完整增量](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-003)。
- 验收：[AC-LWS-003](#ac-lws-003)。
- 系统设计状态：已覆盖
- 系统设计依据：[归档和恢复加速边界](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-003)承接长期索引与历史定位；[新格式工作集](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-runtime-state)和[历史读取与恢复](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-read-recovery)承接当前状态与累计历史分离、独立热策略及检查点之间的恢复。
- 实现状态：部分实现
- 实现与验证依据：[正式保留规划](https://github.com/nil-studio/oasis7/blob/2676c835cc50f12d7fd16a762eaa21f026ae79d4/crates/oasis7/src/runtime/world/formal_retention_planning.rs)及[归档应用](https://github.com/nil-studio/oasis7/blob/2676c835cc50f12d7fd16a762eaa21f026ae79d4/crates/oasis7/src/runtime/world/formal_retention.rs)已覆盖部分结果、nonce、Agent intent 和事件；本 PR 不认定全部历史类别与角色分层已实现。
- 核对日期与基线：2026-10-28；新格式需求为 `112a627e889cb0601ed368b2c0be204e39e7554a` 上本文候选；既有实现依据仍为固定 `2676c835cc50f12d7fd16a762eaa21f026ae79d4` 的静态归档审读，无新格式长期或跨角色运行。
- 剩余范围：设计目标已承接；新格式常驻工作集、独立热策略、全历史类别及检查点之间的恢复待实现和规模验证；所列既有归档实现不代签这些新增范围。

<a id="req-lws-004"></a>
### REQ-LWS-004：临时材料与冗余表示可安全回收

- Trace ID: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c
- 性质：目标要求。
- 适用条件：回收正式执行尝试、迁移暂存或已存在等价持久表示的重复材料。
- 要求：系统仅可回收已证明不再被任何适用恢复、历史验证、正在进行的操作或合法读取依赖的表示；未确认提交、冻结/准备输入、回滚安全材料和未完成迁移必须保持可用。固定数量、文件年龄、TTL 或空间压力不能代替该证明。
- 理由：正常维护应收敛多余占用，失败过程中应优先保住已确认成果与原请求的恢复路径。
- 上位承诺：[SC-3](prd.md#sc-infra-003)、[SC-6](prd.md#sc-infra-006)、[SC-7](prd.md#sc-infra-007)。
- 专业权威：[保护根与安全回收](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-004)。
- 验收：[AC-LWS-004](#ac-lws-004)。
- 系统设计状态：已覆盖
- 系统设计依据：[终态核对、读者与回收事务](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-004)承接删除前证明、并发/未知保护与失败后的保留语义；[新格式提交发布](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-commit-publication)和[保留与回收](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-retention-gc)承接新格式根与未决材料的生命周期。
- 实现状态：部分实现
- 实现与验证依据：[既有正式物理回收](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_distfs/src/controlled_authority/formal/physical_gc.rs)已保护原逻辑对象并校验回收闭包；新格式提交、检查点、读者和未决材料的联合生命周期尚未交付。
- 核对日期与基线：2026-10-28；需求为 `112a627e889cb0601ed368b2c0be204e39e7554a` 上新格式设计候选；实现静态审读为同一固定版本的上述回收入口，未执行新格式删除或故障测试。
- 剩余范围：设计目标已承接；新格式临时材料与冗余表示回收、读者保护、崩溃恢复和长期收敛待实现验证。

<a id="req-lws-005"></a>
### REQ-LWS-005：独立持久保障与权威隔离不降低

- Trace ID: O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5
- 性质：目标要求。
- 适用条件：共享数据、减少副本、调整节点角色或迁移持久内容。
- 要求：优化后必须继续满足合法提交 profile 的独立故障域持久条件和旧写者隔离；共享物理内容不得把独立副本合并成同一失效点，归档、缓存或提供字节的服务不得取得新的提交/投票权。
- 理由：较小的账面容量不能以更容易丢失已确认效果或产生双权威为代价。
- 上位承诺：[SC-1](prd.md#sc-infra-001)、[SC-2](prd.md#sc-infra-002)、[REQ-DCS-003](distributed-consensus-and-state-availability.prd.md#req-dcs-003)。
- 专业权威：[故障域与角色](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-005)。
- 验收：[AC-LWS-005](#ac-lws-005)。
- 系统设计状态：已覆盖
- 系统设计依据：[单域优化与独立副本验证](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-005)承接共享范围、独立性与服务资格条件；[新格式保留与回收](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-retention-gc)保持原必要持久域和权威边界。
- 实现状态：部分实现
- 实现与验证依据：[正式双端接口](https://github.com/nil-studio/oasis7/blob/2676c835cc50f12d7fd16a762eaa21f026ae79d4/crates/oasis7_distfs/src/controlled_authority/formal/endpoint.rs)提供现有基础；[生命周期记录](../../p2p/blockchain/persistent-world-node-lifecycle.runbook.md#存储与恢复边界)不代表本次优化后真实独立故障域验收。
- 核对日期与基线：2026-10-28；新格式需求为 `112a627e889cb0601ed368b2c0be204e39e7554a` 上本文候选；既有实现依据仍为固定 `2676c835cc50f12d7fd16a762eaa21f026ae79d4` 正式端点的静态审读，未执行新格式真实部署验收。
- 剩余范围：设计目标已承接；新格式单域完全丢失、独立归档取回与旧写者隔离待实现和实际适用环境验证。

<a id="req-lws-006"></a>
### REQ-LWS-006：创建后恢复与迁移保持同世界历史

- Trace ID: O7-REQ-87c0cd77-0b20-4eba-90ff-5a492e85b3a0
- 性质：目标要求。
- 适用条件：新格式世界首次创建后的恢复与维护切换，或将已有正式世界迁移到新物理格式、存储布局或兼容软件。[新格式创建范围](#lws-new-format-scope)不承担旧世界导入或旧程序兼容义务。
- 要求：新格式世界重开后必须沿原身份核对已确认历史与未完成请求，不能自动重新初始化。实际迁移必须可核对原世界的完整历史和正在进行的操作；切换前保留可用旧表示，结果未知时续查同一迁移。回退只回到能理解全部已确认历史的受支持实现，不能撤销确认结果、重签旧证据、丢弃新历史或另建世界。
- 理由：容量优化应是原世界的维护，不要求玩家重新获得资产或重做委托。
- 上位承诺：[SC-3](prd.md#sc-infra-003)、[SC-6](prd.md#sc-infra-006)、[SC-7](prd.md#sc-infra-007)。
- 专业权威：[兼容迁移与回退](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-006)。
- 验收：[AC-LWS-006](#ac-lws-006)。
- 系统设计状态：已覆盖
- 系统设计依据：[新格式创建与恢复](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-activation-recovery)承接首次创建后的同世界重开、未知结果和受支持范围；[迁移事务与原身份续做](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-006)继续承接实际既有世界迁移的切换与回退边界。
- 实现状态：部分实现
- 实现与验证依据：[既有正式物理代](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_distfs/src/controlled_authority/formal/physical_generation.rs)提供分批维护与选择的局部基础；新格式首次创建后的身份核对、恢复与维护闭环尚未交付，已有物理代不证明旧数据导入能力。
- 核对日期与基线：2026-10-28；需求为 `112a627e889cb0601ed368b2c0be204e39e7554a` 上新格式设计候选；实现静态审读为同一固定版本的上述存储路径，无新格式恢复或迁移运行。
- 剩余范围：设计目标已承接；新格式创建后的同世界重开、未决核对和维护中断待实现验证；旧世界迁移与旧程序回退为独立范围。

<a id="req-lws-007"></a>
### REQ-LWS-007：恢复后继续原工作且不重复外部效果

- Trace ID: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709
- 性质：目标要求。
- 适用条件：经过压缩、归档、回收或迁移后，恢复同一世界并准备继续服务。
- 要求：在相同合法权威、版本、权限、资源和后续输入条件下，恢复必须延续与不中断分支一致的新请求、任务身份、模块行动时点和结算；历史读取/恢复只消费原已记录模型或外部结果，不重新请求模型、重复收费或重复派发已确认效果。尚未收敛的外部交付沿原幂等身份核对。
- 理由：可读旧状态之外，玩家还需要原委托继续执行；“至多一次”不能用全部拒绝来满足。
- 上位承诺：[SC-7](prd.md#sc-infra-007)、[AC-DCS-002](distributed-consensus-and-state-availability.prd.md#ac-dcs-002)。
- 专业权威：[恢复与继续运行](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-007)。
- 验收：[AC-LWS-007](#ac-lws-007)。
- 系统设计状态：已覆盖
- 系统设计依据：[恢复路径及后续效果对照](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-007)承接原外部结果与续跑边界；[新格式历史恢复](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-read-recovery)、[提交发布](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-commit-publication)和[执行工作集](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-runtime-state)承接完整变化、未决状态与恢复后继续工作。
- 实现状态：部分实现
- 实现与验证依据：[正式恢复](https://github.com/nil-studio/oasis7/blob/2676c835cc50f12d7fd16a762eaa21f026ae79d4/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/formal_writer_recovery.rs)已有精确候选/快照路径；[非空事件后缀合同](../../world-runtime/design.md#runtime-journal-suffix-recovery-design)仍要求独立控制状态与续跑验证，本次未执行。
- 核对日期与基线：2026-10-28；新格式需求为 `112a627e889cb0601ed368b2c0be204e39e7554a` 上本文候选；既有实现依据仍为固定 `2676c835cc50f12d7fd16a762eaa21f026ae79d4` 的静态恢复审读，无新格式业务/模块/外部交付运行。
- 剩余范围：设计目标已承接；新格式非空历史控制状态、新请求和模块行动正向对照、未知外部交付核对待实现验证。

<a id="req-lws-008"></a>
### REQ-LWS-008：容量与读取限制产生可核对的降级

- Trace ID: O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c
- 性质：目标要求。
- 适用条件：写入、读取、归档、迁移或恢复遇到空间、内存、工作量或响应预算限制。
- 要求：系统必须区分未能开始、仍在进行、已完成但维护失败、结果未知与材料不可验证，保留适用原操作身份、已确认事实和真实下一步；不能把读限额解释为历史缺失，把写限额解释为已知未提交，或通过自动删历史解除限制。新格式的历史范围读取、同步和完整恢复分别受明确的工作量与峰值内存预算约束；较长操作可按已验证进度续做，不能因读取一小段历史而要求一次加载全部累计历史。
- 理由：运营者能处理容量压力，玩家与 Agent 不会因错误反馈重做操作或误用陈旧状态。
- 上位承诺：[SC-7](prd.md#sc-infra-007)、[REQ-DCS-005](distributed-consensus-and-state-availability.prd.md#req-dcs-005)。
- 专业权威：[预算、背压与读取边界](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-008)。
- 验收：[AC-LWS-008](#ac-lws-008)。
- 系统设计状态：已覆盖
- 系统设计依据：[容量预留、降级与有界查询](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-008)承接未知保护和可观察结果；[新格式资源预算](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-resource-budgets)和[执行工作集](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-runtime-state)分别约束范围读取、同步、完整恢复与运行态开销。入口呈现复用既有 DCS 语义。
- 实现状态：部分实现
- 实现与验证依据：[正式材料读取](https://github.com/nil-studio/oasis7/blob/2676c835cc50f12d7fd16a762eaa21f026ae79d4/crates/oasis7_distfs/src/controlled_authority/formal/endpoint_committed_material.rs)和[保留规划](https://github.com/nil-studio/oasis7/blob/2676c835cc50f12d7fd16a762eaa21f026ae79d4/crates/oasis7/src/runtime/world/formal_retention_planning.rs)有局部容量边界；统一写入/迁移/恢复与消费者结果尚未完成，无本条整体运行记录。
- 核对日期与基线：2026-10-28；新格式需求为 `112a627e889cb0601ed368b2c0be204e39e7554a` 上本文候选；既有实现依据仍为固定 `2676c835cc50f12d7fd16a762eaa21f026ae79d4` 的局部预算静态审读，无新格式满盘或消费者验证。
- 剩余范围：设计目标已承接；新格式按读取范围及执行工作集划分的预算、满盘/读取受限、续做与消费者反馈待实现验证；现有局部预算不证明新格式已支持相同或更大规模。

<a id="req-lws-009"></a>
### REQ-LWS-009：容量收益与完备性可共同复核

- Trace ID: O7-REQ-cb320711-af0a-44f3-98d4-bc7e8c1d0916
- 性质：目标要求。
- 适用条件：选择或宣称任何正式世界存储优化有效。
- 要求：必须以相同业务历史、工作负载、独立副本范围和配置的证据，同时说明实际存储量、增长、暂态峰值、读取/恢复代价及完备性结果；新格式对照须记录各自格式身份及历史事实的对应关系，不要求旧格式对象标识相同。当前执行工作集、完整历史、范围读取和完整恢复的成本分别核对；缺测量不报节省比例，部分目录缩小或仅最新恢复成功不构成整体通过。
- 理由：运营者需要知道实际节省、代价和适用规模，避免容量被转移到未统计的目录或副本。
- 上位承诺：[SC-2](prd.md#sc-infra-002)、[SC-3](prd.md#sc-infra-003)、[SC-7](prd.md#sc-infra-007)。
- 专业权威：[度量与组合验证](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-009)。
- 验收：[AC-LWS-009](#ac-lws-009)。
- 系统设计状态：已覆盖
- 系统设计依据：[容量、性能与证据范围](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-009)承接全生命周期口径；[新格式验证](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-validation)承接不同初始格式的同业务历史对照、增长、范围读取、执行工作集和恢复成本的独立判定。
- 实现状态：部分实现
- 实现与验证依据：[现有存储 metrics](https://github.com/nil-studio/oasis7/blob/2676c835cc50f12d7fd16a762eaa21f026ae79d4/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/storage_metrics.rs)提供部分尺寸与引用信息；[本地物理计量](../../../crates/oasis7_distfs/src/storage_inventory.rs)与[容量操作入口](../../world-runtime/runtime/formal-world-storage-efficiency.manual.md#v-lws-capacity)补充静止树的分域 inode 去重、allocated bytes 和不完整扫描报告；[离线 shadow 打包](../../world-runtime/runtime/formal-world-storage-efficiency.manual.md#v-lws-codec)可构建并精确读回相同对象命名空间的紧凑候选，提供源/目标 allocated 比较；它不激活、不删除源，不证明净释放。完整生命周期、真实独立端点与同业务历史容量收益仍未验证。
- 核对日期与基线：2026-10-28；新格式需求为 `112a627e889cb0601ed368b2c0be204e39e7554a` 上本文候选；既有实现依据仍为上述 metrics 与物理计量的固定审读范围，无新格式基准执行。
- 剩余范围：设计目标已承接；新格式完整采样、真实独立副本、业务规模矩阵、工作集/范围读取/恢复联合验证及运营者报告待实现验证。

<a id="lws-acceptance"></a>
## 5. 验收与证据

以下为待实现/待执行的产品场景，不是已存在或已通过的测试名称。每项适用结果独立判定；准确输入、故障注入、命令和证据格式由配套系统设计及 QA 拥有。

<a id="ac-lws-001"></a>
### AC-LWS-001：完整历史及依赖核验

- 覆盖：[REQ-LWS-001](#req-lws-001)；Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1。
- 给定：有已确认业务、未完成工作、旧版本执行与原外部结果的固定正式世界。
- 当：执行优化并从其保留材料恢复、查询早期与当前历史。
- 则：范围内每类原事实及必要依赖均可核对，原请求结果、当前状态与继续运行材料完整；分别移除唯一必要历史内容、原版本工件或证明时必须指出缺口，不能仅凭新快照或哈希宣布完备。
- 证据边界：完整性检查按固定样本全范围执行；少数抽样读回只能说明抽样成功，不能证明整个保留集合完整。

<a id="ac-lws-002"></a>
### AC-LWS-002：原字节与证明兼容

- 覆盖：[REQ-LWS-002](#req-lws-002)；Trace refs: O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c。
- 给定：多阶段重复内容、大小不同的对象及受不同读权限保护的正式材料；新格式场景含多次少量状态变化及相邻检查点。
- 当：共享、压缩、打包、重开并通过所有受影响的受权读面读取。
- 则：相对于该世界已接受的材料，原字节、对象与提交身份、签名/证明校验相同；新格式的小变化场景减少重复保存的共同内容，全部历史仍可读取。损坏、截断、错配或不支持的表示被拒绝；共享不授予原无权限的读取。无收益内容保留原合法表示也可通过。
- 证据边界：新格式以自身首次接受的历史为基线；旧数据导入和旧程序兼容不在新格式创建的验收范围，实际节省按 [AC-LWS-009](#ac-lws-009) 判定。

<a id="ac-lws-003"></a>
### AC-LWS-003：越过热窗口后仍可核对原历史

- 覆盖：[REQ-LWS-003](#req-lws-003)；Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670。
- 给定：历史已超过原热点与单次读取范围，包含旧请求、目标/控制链、多次业务结算和仍需继续的工作。
- 当：分别缩小热访问范围、改变恢复检查点数量和密度，重开服务并按原身份读取早期记录及检查点之间的历史位置。
- 则：旧结果与关联完整，两项热策略分别反映各自成本；新格式不把全部已完成历史重新装入常驻执行工作集。受限读取明确范围和继续条件；目标/控制历史下冷不使新工作因丢失前驱而误拒绝；被释放的展开状态有已验证的等价恢复路径，完整历史保留承诺不变。
- 证据边界：从一个近期检查点恢复不能代签全部深历史；具体角色与归档依赖各自验证。

<a id="ac-lws-004"></a>
### AC-LWS-004：维护收敛且中断不误删

- 覆盖：[REQ-LWS-004](#req-lws-004)；Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c。
- 给定：成功尝试、未决提交、冻结输入、合法读取、回滚安全与未完成迁移同时存在。
- 当：多轮成功维护，并分别在提交确认前后、清理中和读者持有材料时中断。
- 则：成功终态的可证明冗余材料最终收敛，受保护内容一直可用；未知期间保留所需两侧材料；清理失败允许占用增加但不撤销提交、不发布旧状态。仅改变文件年龄或保留数量不得改变可删除性。
- 证据边界：目录变小、一次无并发回收或只检查最新快照均不足以通过。

<a id="ac-lws-005"></a>
### AC-LWS-005：失去一个独立故障域后不丢确认结果

- 覆盖：[REQ-LWS-005](#req-lws-005)；Trace refs: O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5。
- 给定：优化后的正式世界和按适用 profile 验证的独立持久部署。
- 当：完整失去任一要求覆盖的故障域，并尝试由旧写者或非权威存储服务推进世界。
- 则：仍可恢复全部已确认效果，未决请求按原身份核对；旧写者和非权威服务不得推进历史，缺少规定的独立持久条件时停止新确认。
- 证据边界：本机两个目录、同盘链接、模拟端点或进程退出只能证明对应局部行为，不能证明真实独立故障域。

<a id="ac-lws-006"></a>
### AC-LWS-006：创建后恢复及迁移保持原历史

- 覆盖：[REQ-LWS-006](#req-lws-006)；Trace refs: O7-REQ-87c0cd77-0b20-4eba-90ff-5a492e85b3a0。
- 给定：已完成首次创建、具有已确认历史和未完成原请求的新格式世界；实际迁移场景另准备明确兼容范围的新旧实现。
- 当：新格式在创建后写入、维护和重开时中断或丢失回应；实际迁移另在准备、验证、切换及清理阶段分别中断并尝试回退。
- 则：沿原世界身份恢复已确认历史并核对未完成请求，未知不被判成未提交，不重新初始化。实际迁移续做原操作，未完成切换保留有效旧表示；旧实现不能理解已发布历史时明确停止，不能删除新历史来满足回退。
- 证据边界：新格式创建后的恢复必须验证；旧数据导入与旧程序回退属于实际迁移范围，不能成为新格式创建的隐含前置。实际迁移仍须覆盖两端进度不一致、期间新增的已确认历史和空间峰值。

<a id="ac-lws-007"></a>
### AC-LWS-007：非空历史恢复后继续业务与模块行动

- 覆盖：[REQ-LWS-007](#req-lws-007)；Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709。
- 给定：相同合法条件的不中断分支，以及较早完整检查点加非空原历史的恢复分支；包含已记录模型结果、未完成任务与模块行动安排。
- 当：经过优化后恢复，并向两分支提供相同合法新输入；另分别覆盖已持久候选、仅有原冻结输入和外部响应丢失。
- 则：新请求身份、配方结果、模块唤醒/暂停、费用和结算与对照一致，至少一条合法新配方完成一次；历史读取/恢复不再调模型或重复收费。未确认交付核对原幂等身份，不能由恢复重放已确认效果。
- 证据边界：完整目标快照往返、当前状态根相同或全部请求被拒绝，不代替两项正向续跑；外部系统不具备幂等确认条件时保持受阻，不声称物理恰好一次。

<a id="ac-lws-008"></a>
### AC-LWS-008：预算耗尽后保留原结果与真实下一步

- 覆盖：[REQ-LWS-008](#req-lws-008)；Trace refs: O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。
- 给定：容量与工作预算分别接近限制，且存在在途操作和已确认历史。
- 当：在新写入、归档、适用迁移、冷读取、同步与恢复阶段分别耗尽预算或丢失回应；对新格式分别执行窄范围读取、连续同步和完整恢复。
- 则：各读面给出适用服务限制、原操作结果或未知、已验证范围和下一步；各类操作的工作量与峰值内存保持在声明预算内。无未授权新效果、历史删除或静默部分成功；恢复容量后沿原身份及已验证进度续做，已确认结果不重复；读取被限制不等于原历史不存在。
- 证据边界：只验证错误返回不证明原结果及消费者含义一致；不新增无限资源或固定完成时限承诺。

<a id="ac-lws-009"></a>
### AC-LWS-009：同一业务历史的容量与恢复对照

- 覆盖：[REQ-LWS-009](#req-lws-009)；Trace refs: O7-REQ-cb320711-af0a-44f3-98d4-bc7e8c1d0916。
- 给定：固定的业务输入、原外部结果、历史保留范围、存储角色、独立副本、配置及测量环境；不同初始格式记录业务事实的对应关系。
- 当：分别运行基线与优化候选，经历少量状态变化的多个检查点、活跃状态与历史分别增长、跨热窗口、归档、重启和失败恢复；已有世界迁移按其独立范围另验。
- 则：记录实际占用和增长来源、暂态峰值、常驻执行工作集、范围读取与完整恢复的资源成本、每项完备性判定及未覆盖范围；收益只在相同保留内容和保障水平下比较。缺少目录/端点、测量不可用或某完整性项失败时，不能给出整体节省成功结论。
- 证据边界：合成样本只支持所用规模与数据形态，不能把旧工程样本、表观文件长度或单目录下降推广为正式部署收益。

### 5.1 追踪

| 叶子要求 REQ | 上位承诺 | 专业 owner 与权威 | 验收场景 AC | 验证证据与测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-LWS-001](#req-lws-001) | [SC-3/完整恢复](prd.md#sc-infra-003) | `runtime_engineer`；[DES-LWS-001](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-001)；[新格式](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-read-recovery) | [AC-LWS-001](#ac-lws-001) | 两份适用设计的验证映射；`test_tier_required` 局部、`test_tier_full` 完整历史恢复；新格式待执行。 |
| [REQ-LWS-002](#req-lws-002) | [SC-6/同世界历史](prd.md#sc-infra-006) | `runtime_engineer`；[DES-LWS-002](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-002)；[新格式](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-object-model) | [AC-LWS-002](#ac-lws-002) | 两份适用设计的验证映射；`test_tier_required` 局部、`test_tier_full` 原内容/身份及新格式共享；新格式待执行。 |
| [REQ-LWS-003](#req-lws-003) | [SC-2/存储角色](prd.md#sc-infra-002)、[SC-3](prd.md#sc-infra-003) | `runtime_engineer`；[DES-LWS-003](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-003)；[新格式](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-runtime-state) | [AC-LWS-003](#ac-lws-003) | 两份适用设计的验证映射；`test_tier_required` 局部、`test_tier_full` 独立热策略、工作集与深历史；新格式待执行。 |
| [REQ-LWS-004](#req-lws-004) | [SC-3](prd.md#sc-infra-003)、[SC-7/服务恢复](prd.md#sc-infra-007) | `runtime_engineer`；[DES-LWS-004](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-004)；[新格式](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-retention-gc) | [AC-LWS-004](#ac-lws-004) | 两份适用设计的验证映射；`test_tier_required` 局部、`test_tier_full` 实际崩溃、并发与安全回收；新格式待执行。 |
| [REQ-LWS-005](#req-lws-005) | [SC-1/正式提交](prd.md#sc-infra-001)、[SC-2](prd.md#sc-infra-002) | `runtime_engineer`、`blockchain_ops_engineer`；[DES-LWS-005](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-005)；[新格式](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-retention-gc) | [AC-LWS-005](#ac-lws-005) | 两份适用设计的验证映射；`test_tier_required` 局部、`test_tier_full` 真实独立故障域；新格式待执行。 |
| [REQ-LWS-006](#req-lws-006) | [SC-6](prd.md#sc-infra-006)、[SC-7](prd.md#sc-infra-007) | `runtime_engineer`；[DES-LWS-006](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-006)；[新格式](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-activation-recovery) | [AC-LWS-006](#ac-lws-006) | 两份适用设计的验证映射；`test_tier_required` 局部、`test_tier_full` 创建后重开及未知核对；既有世界迁移另按实际范围验证；新格式待执行。 |
| [REQ-LWS-007](#req-lws-007) | [AC-DCS-002](distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | `runtime_engineer`；[DES-LWS-007](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-007)；[新格式](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-read-recovery) | [AC-LWS-007](#ac-lws-007) | 两份适用设计的验证映射；`test_tier_required` 局部、`test_tier_full` 业务、模块与外部效果续跑；新格式待执行。 |
| [REQ-LWS-008](#req-lws-008) | [SC-7](prd.md#sc-infra-007) | `runtime_engineer`；[DES-LWS-008](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-008)；[新格式](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-resource-budgets) | [AC-LWS-008](#ac-lws-008) | 两份适用设计的验证映射；`test_tier_required` 局部、`test_tier_full` 范围读取、同步、完整恢复及消费者；新格式待执行。 |
| [REQ-LWS-009](#req-lws-009) | [SC-2](prd.md#sc-infra-002)、[SC-3](prd.md#sc-infra-003) | `runtime_engineer`、`blockchain_ops_engineer`、`qa_engineer`；[DES-LWS-009](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-009)；[新格式](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-validation) | [AC-LWS-009](#ac-lws-009) | 两份适用设计的验证映射；`test_tier_required` 局部、`test_tier_full` 同业务历史容量与资源对照；新格式待执行。 |

<a id="lws-evidence-scope"></a>
### 5.2 效果与证据范围

初始设计及状态卡中的固定源码链接只证明当时的局部实现可审读。后续源码承接范围包括保持原 bytes 的共享压缩、统一读取与分批物理代，完整快照增量和新索引版本，任务控制冷链、原生执行工件闭包，以及发布后独立读回再回收尝试输出；具体合同及参数由[专业设计当前入口](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#current-storage-implementation)维护。不得把存在源码或文档检查扩成行为场景通过，状态卡的“部分实现”也不授予部署接受。

既有复制检查点的运行世界证据与新格式世界的验收分开记录；前者不证明数据已迁移，后者不要求旧数据兼容。本专题尚无新格式完整业务与真实独立故障域的组合验收证据。原始命令、source/tested tree、配置、平台、实际结果和未覆盖项保存在实现 PR/验证记录；本地双目录、测试签名者、进程 SIGKILL 或合成执行环境分别只证明其实际范围。原生环境不完整的平台须报告不可用，不能用另一环境替代原工件来完成验收。

容量比较至少覆盖状态变化少但提交多、活跃实体持续增长、长业务与控制历史、大型执行工件、归档边界和适用维护中断等不同数据形态。必须统计完整节点与独立持久端的真实物理范围；同域复用只计一次，独立副本分别计入，缺失数据保留不可用状态。具体计量、规模和阈值分别由[新格式验证](../../world-runtime/runtime/incremental-world-state-storage.design.md#iws-validation)与[既有格式验证](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-009)确定。

完备性、存储收益、读取/恢复性能、故障域保障和消费者体验分别判定。局部 required 通过不降低上位 `test_tier_full`、真实部署或服务资格要求；终止进程不等于掉电实验，本机端点模拟不等于独立故障域。公开 claim 继续由[根 README](../../../README.md)与发行权威决定。

## 6. 权威与相邻专题

本专题细化存储优化的独立义务，不替代已有上位要求。Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4（[REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)）、O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3（[REQ-DCS-003](distributed-consensus-and-state-availability.prd.md#req-dcs-003)）、O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b（[REQ-DCS-005](distributed-consensus-and-state-availability.prd.md#req-dcs-005)）。同世界历史、角色隔离与服务等级保持原 authority 和原验收。

| 主责 | 本专题中的职责 |
| --- | --- |
| `producer_system_designer` | 完备性范围、使用者取舍、可见结果与上位组合一致性；不选择存储算法。 |
| `runtime_engineer` | 正式材料、物理布局、恢复控制状态、事务、版本迁移和受权读取；结合 P2P 权威合同实现。 |
| `blockchain_ops_engineer` | 真实存储角色、独立故障域、容量规划及同窗口恢复/隔离事实；不由运维配置自授权威。 |
| 消费者专业 owner | 复用世界服务语义，按真实入口显示结果、限制和下一步；不把存储可达当当前权威。 |
| `qa_engineer` | 九项独立验收、故障与性能范围、真实证据及非证明项；不以结构检查代签功能。 |

[旧运行态存储治理](../../world-runtime/runtime/runtime-storage-footprint-governance.prd.md)继续拥有工程路径及原预算合同。其“保留窗口内 replay”不能减少正式世界完整历史；较低层允许的诊断启动也不能绕过正式恢复与服务闸门。冲突由产品 owner 与实际专业 owner 按更完整的历史/权威边界处理，未解决前保留材料并限制受影响优化。

<a id="lws-decisions"></a>
## 7. 设计取舍与未决问题

| 选择 | 理由与承担的代价 | 重新评估条件 |
| --- | --- | --- |
| 新格式创建直接分离当前状态与完整历史 | 降低少量变化重复保存全部历史的成本；需要独立证明全部状态、控制安排和历史位置可恢复。 | 新格式接受范围、恢复材料或执行边界变化时重新核对，不受旧格式兼容约束。 |
| 既有世界维护保留原承诺 | 旧数据导入、迁移和处置另定范围，避免把新格式创建当作现场世界迁移。 | 实际启动同世界迁移时，适用原字节、兼容与回退合同。 |
| 热状态有界，完整历史按真实信息增长 | 保障长期世界历史，允许较慢冷读取和持续容量成本。 | 增长证据支持扩容、角色或放置策略变化；不以容量压力重新定义历史权益。 |
| 保留独立持久保障 | 同域共享节省冗余，独立副本继续承担必须的存储成本。 | 只有合法 profile 的故障模型与权威合同演进后，才能评估新的布局。 |
| 迁移期间允许新旧材料共存 | 提高峰值占用，换取可核对切换及原请求恢复。 | 旧表示的最后依赖和受支持读者已核实退出，才可回收。 |
| 读取与恢复采用有界工作 | 长历史可能需要多次受支持读取或更长恢复，避免一次操作耗尽资源。 | 真实数据验证新的工作预算；不得用增大常量冒充无限规模支持。 |

未决项由本次文档 PR 承接设计输入，后续实现任务可以直接是 PR，不强制另建 Issue。以下不影响既定完备性底线，未满足前相应优化保持关闭或只在已验证范围使用。

| 尚需确定的问题 | 影响与决策 role | 所需信息、解决触发和临时边界 |
| --- | --- | --- |
| 各实际世界的必要内容占比与重复来源是多少？ | [REQ-LWS-002](#req-lws-002)、[REQ-LWS-009](#req-lws-009)；runtime/ops/QA。 | 在选择默认策略前测量同负载全路径；未测量不公布节省比例或扩大默认范围。 |
| 哪些旧版本可提供完整控制状态和原历史工件？ | [REQ-LWS-001](#req-lws-001)、[REQ-LWS-003](#req-lws-003)、[REQ-LWS-007](#req-lws-007)；runtime/WASM 专业 owner 与 QA。 | 减少完整快照前逐版本验证依赖与续跑；不完整旧区间保留可证明的材料，不推断补齐。 |
| 哪些新旧程序与存储表示组合可安全迁移/回退？ | [REQ-LWS-002](#req-lws-002)、[REQ-LWS-006](#req-lws-006)；runtime/发行专业 owner 与 QA。 | 仅在实际既有世界迁移范围内，开放前完成兼容和中断矩阵；新格式创建不依赖该独立范围。 |
| 生产容量预留、冷读取和恢复目标怎样设定？ | [REQ-LWS-008](#req-lws-008)、[REQ-LWS-009](#req-lws-009)；ops/runtime/QA。 | 根据实际世界、设备和业务负载确定预算；默认值不构成所有规模 SLO，不通过删历史满足目标。 |
| 真实独立故障域和归档位置如何获得同窗口证据？ | [REQ-LWS-005](#req-lws-005)；ops/P2P 专业 owner 与 QA。 | 降低本机保存量或切换实际部署前完成独立取回及隔离验证；本机模拟仅作为局部协议证据。 |
