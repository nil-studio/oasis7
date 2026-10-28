# DistFS 反馈账本与复制设计

- 对应需求文档：[DistFS 反馈账本与复制](distfs-feedback-ledger-and-replication.prd.md)
- Owner role：runtime_engineer；canonical repository：eng-cc/oasis7
- 固定源码审读基线：f97f2253cdcb163bf984e3c90d59d792fe1c9b19（2026-10-15）。current 指静态实现/合同，planned 指未执行场景，历史完成不等于本候选通过。

## 1. 问题、目标与非目标

公开反馈须保留作者可验证的 append-only 记录，以轻量 announce 发现新记录，再按 hash 取得 blob。设计将本地账本、复制提示、内容校验与 NodeRuntime 失败观测分开，避免把提交返回、publish 或重复幂等误作全节点交付。

本专题是公开反馈存储/复制，不是Agent学习反馈或工业结算反馈，不改变共识、世界finality、state-sync、恢复或经济规则。不承诺生产拓扑、SLA、隐私分级、灾备或release。

## 2. 上游约束与相关角色

配对PRD的[接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data)拥有既有专业义务，[验证与追溯](distfs-feedback-ledger-and-replication.prd.md#验证与追溯)规定回归范围。DistFS拥有记录/签名校验，NodeRuntime拥有启动与tick，replication/network拥有fetch授权、角色与lanes；QA判定实际运行证据。未发现该公开append-only服务的独立玩家承诺；将来的产品消费需要重新审读，不能借同名“反馈”接入其他产品REQ。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | append-only root/event与tombstone保历史 | [des-feedback-ledger](#des-feedback-ledger) | DistFS/NodeRuntime与replication/network；QA验证 | 产品适用性：N/A；reason=公开反馈存储复制未独立定义玩家世界结果；scope=append-only root/event与tombstone保历史；不取消签名/授权或本行专业义务 |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | 作者签名、nonce、expiry与基础输入校验 | [des-feedback-signature](#des-feedback-signature) | DistFS/NodeRuntime与replication/network；QA验证 | 产品适用性：N/A；reason=公开反馈存储复制未独立定义玩家世界结果；scope=作者签名、nonce、expiry与基础输入校验；不取消签名/授权或本行专业义务 |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | 公开读、audit与限流的有限能力 | [des-feedback-public-read](#des-feedback-public-read) | DistFS/NodeRuntime与replication/network；QA验证 | 产品适用性：N/A；reason=公开反馈存储复制未独立定义玩家世界结果；scope=公开读、audit与限流的有限能力；不取消签名/授权或本行专业义务 |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | announce只含元数据；found、payload、hash与size验证后入库 | [des-feedback-fetch](#des-feedback-fetch) | DistFS/NodeRuntime与replication/network；QA验证 | 产品适用性：N/A；reason=公开反馈存储复制未独立定义玩家世界结果；scope=announce只含元数据；found、payload、hash与size验证后入库；不取消签名/授权或本行专业义务 |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | 重复announce幂等及冲突拒绝 | [des-feedback-idempotence](#des-feedback-idempotence) | DistFS/NodeRuntime与replication/network；QA验证 | 产品适用性：N/A；reason=公开反馈存储复制未独立定义玩家世界结果；scope=重复announce幂等及冲突拒绝；不取消签名/授权或本行专业义务 |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | replication、network及BlobState publish+subscribe前置 | [des-feedback-start-gate](#des-feedback-start-gate) | DistFS/NodeRuntime与replication/network；QA验证 | 产品适用性：N/A；reason=公开反馈存储复制未独立定义玩家世界结果；scope=replication、network及BlobState publish+subscribe前置；不取消签名/授权或本行专业义务 |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | start前挂载、effective peers与显式topology fail closed | [des-feedback-network-wiring](#des-feedback-network-wiring) | DistFS/NodeRuntime与replication/network；QA验证 | 产品适用性：N/A；reason=公开反馈存储复制未独立定义玩家世界结果；scope=start前挂载、effective peers与显式topology fail closed；不取消签名/授权或本行专业义务 |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | 有界内存outbox、每tick发布及失败回排 | [des-feedback-outbox](#des-feedback-outbox) | DistFS/NodeRuntime与replication/network；QA验证 | 产品适用性：N/A；reason=公开反馈存储复制未独立定义玩家世界结果；scope=有界内存outbox、每tick发布及失败回排；不取消签名/授权或本行专业义务 |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | 每tick处理上限、drain语义及局部失败观测 | [des-feedback-incoming-limit](#des-feedback-incoming-limit) | DistFS/NodeRuntime与replication/network；QA验证 | 产品适用性：N/A；reason=公开反馈存储复制未独立定义玩家世界结果；scope=每tick处理上限、drain语义及局部失败观测；不取消签名/授权或本行专业义务 |

## 3. 当前状态、目标状态与差距

记录/签名、hash fetch、幂等、启动gate及tick接线在固定源码中存在；本轮没有执行Rust行为测试。已有模块fixture的断言范围见§11.1。

待明确的实现/验证差距包括：有界outbox不持久、写账本和enqueue非原子、入站drain后take丢弃溢出条目、回排并发容量、全部错误下tick进展、真实peer/admission组合。PRD每tick上限不能解释成完整CPU/内存预算或可靠消息交付；本次不更改上限、默认能力或授权策略。

## 4. 边界与结构

### 设计定位

本地append-only store是反馈记录层；gossip announce只是发现提示，replication fetch是数据传输路径。公开读不代表匿名写，作者签名不授予网络fetch或世界写权限。不同world/topic、节点签名、allowlist与lane的完整组合仍由其owning contract核验。

### des-feedback-ledger

`feedback.rs` 以 FileStore 保存 root、events、nonces、audit；删除是 tombstone 事件，公开视图返回 tombstoned 与 reason，不物理移除历史。root/event 写入与nonce/audit是多个本地操作，不能推断多文件原子事务。

### des-feedback-signature

本地 create/append/tombstone 对 canonical payload 验 Ed25519，并检查时间范围、当前expiry、时钟偏差、nonce、内容及附件限制；append/tombstone 还要求作者匹配。复制入口复验签名与签名时间范围，并采用幂等nonce；它不重复本地提交的当前时间expiry/clock-skew和IP/pubkey限流。不能将“同一签名约束”扩写成两入口所有拒绝条件相同；安全政策如需改变必须另审。

### des-feedback-public-read

`read_feedback_public` / `list_feedback_public` 提供公开视图；audit 用于IP/pubkey时间窗基础限流，不能推出内容审核、隐私分级或敏感材料保护。`mutate_with_audit` 先执行mutation再写audit；audit失败可使API报错，即使此前mutation已发生，不得按Err自动认定无效果。

### des-feedback-fetch

`FeedbackAnnounce` 包含version/world/feedback/event/action/actor与blob reference；announce不是内容或共识提交。`feedback_runtime.rs` 经现有replication request构造器fetch，要求found且有blob、BLAKE3匹配；`feedback_p2p.rs` 再验证hash、size、版本及标识，对root/event执行对应解析和actor/ID核对，再调用replicated ingest。作者签名不替代fetch requester签名、allowlist或lane准入。

### des-feedback-idempotence

复制root已存在且记录相同则返回已有结果，不同记录拒绝；event按feedback_id/event_id定位，同记录幂等、冲突拒绝。新增event仍核作者、event hash、签名与nonce；append受tombstone约束。它是记录级幂等，不是网络exactly-once或所有写操作可回滚。

### des-feedback-start-gate

`NodeRuntime::start` 要求feedback配置同时具有replication config、network endpoint与角色的BlobState发布/订阅权限，缺失拒绝启动。配置正上限由NodeFeedbackP2pConfig校验。storage角色和feedback作者只拥有各自被准入的存储操作，不因此成为validator或拥有canonical写入权。

### des-feedback-network-wiring

`oasis7_chain_runtime.rs` 在start前调用attach_default_replication_network并挂maintenance DHT。默认listen为loopback ephemeral；非空CLI peers优先，否则加载network-tier manifest peers。仅effective列表为空才设置local handler fallback。`libp2p_replication_network/net_impl.rs` 在无connected/admissible peer且fallback禁用时返回NetworkProtocolUnavailable；这不证明远端部署或所有拓扑已验证。

### des-feedback-outbox

`node_runtime_core.rs` 先完成store mutation，再构造announce并enqueue；队列容量为max(outgoing,1)的64倍（饱和乘法），满队列返回Replication错误。因写账本在前，错误不等于账本未写，也没有自动回滚或持久outbox重建保证。`feedback_runtime.rs` 每tick仅drain出站前缀，publish失败的条目排回剩余pending之前。outbox是Mutex<Vec>内存状态；enqueue容量检查不证明并发发布失败回排时的严格瞬时上界、重启不丢或最终交付。

### des-feedback-incoming-limit

`NetworkSubscription::drain` 移除该topic的整个inbox，`FeedbackAnnounceBridge::drain` 将全部payload解码成Vec，runtime随后take(max(incoming,1))。因此上限约束本轮进入ingest的条数，并不约束此前全部解码工作；超出take的剩余元素被丢弃，未保留到下一tick。单条decode/fetch/ingest失败继续处理后续条目并汇总错误；未实现入站持久重试或端到端不丢保证。NodeRuntime将返回错误用于last_error，不把此局部错误直接退出主tick。

## 5. 关键运行流程

### 数据流

1. 本地请求校验并写账本、nonce/audit，产生mutation receipt。各文件写入和receipt不具有跨步骤事务保证。
2. receipt转换为announce，尝试进入内存outbox；饱和错误可能发生在账本已经写入之后。
3. 启动gate通过的NodeRuntime每tick发布有限出站前缀；publish失败项回排，错误用于诊断。
4. 入站先drain整个订阅并解码，再只对前limit条fetch/hash/size校验和replicated ingest；溢出条目不保留，单条失败不阻止本轮其他已选条目。
5. 相同记录重复ingest幂等，冲突拒绝；记录可读、已announce、已fetch与全网交付分别判断。

## 6. 接口与数据合同

root/event/nonce/audit的布局、canonical签名payload及announce字段继续由PRD和源码定义；不新建schema或协议版本。announce version/world/actor/ID与blob ref必须按§4顺序校验，fetch响应必须found且有payload；成功不能只依赖元数据中的hash字符串。

## 7. 状态、事务与持久化

账本文件与内存outbox是不同状态域；nonce、root/event、audit、enqueue之间没有可由当前入口证明的统一事务。失败时先查询实际记录和稳定拒绝原因；报错不等于无效果，也不承诺自动补发。复制幂等减少重复记录，不能补足丢失的announce或重启中的pending。

## 8. 部署、安全与运行约束

### 运维与安全边界

缺replication、network或BlobState权限时修正被授权的配置，不以重启绕过gate。default loopback/no-peer只服务有限本地wiring；显式topology不能以local handler掩盖peer/admission失败。公开读取不适合敏感内容，基础限流不等于审核或隐私保障。作者签名、fetch授权与canonical写权分别属于不同边界，不以一个校验替代另一个。

## 9. 质量与容量

出站/incoming配置限制当前实现的相应操作条数；入站全量drain/decode及outbox并发回排仍须单独量测。未提供生产吞吐、端到端时延、进程RSS或可靠交付上界，不从单条fixture外推。

## 10. 兼容、迁移与回滚

保留现行append-only、tombstone、作者签名和重复记录幂等，不修改网络版本或默认配置。关闭feedback_p2p不撤销已写记录；回滚二进制不保证恢复内存outbox。新增私密反馈、持久交付或跨节点一致性语义须重新评审。

## 11. 验证设计与可追溯性

以下均为静态核读的test source或planned场景，未执行。局部回归入口为现有DistFS/Node模块测试；组合使用[testing-manual](../../../testing-manual.md)的适用分层，不以fixture升级真实网络/恢复/release结论。执行记录在实际PR/task绑定source、tested tree、配置、角色、lanes、peer、world、命令及输出；没有执行结果时不填pass。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | [des-feedback-ledger](#des-feedback-ledger) | append-only root/event与tombstone保历史 | [testing-manual](../../../testing-manual.md)；`feedback/tests.rs::feedback_create_append_tombstone_public_read_roundtrip` 核公开内容、append事件、tombstone与reason；审读定义未执行。 | 同source/config的断言、状态前后及真实exit/log；尚未执行 | 不证明跨机可靠交付、恢复、finality或release |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | [des-feedback-signature](#des-feedback-signature) | 作者签名、nonce、expiry与基础输入校验 | [testing-manual](../../../testing-manual.md)；`feedback/tests.rs` 的 feedback_author_control_rejects_non_author_append 与 feedback_replay_nonce_is_rejected 前者核非作者拒绝；后者改变feedback_id却复用旧signature，仅断言验证失败，不能隔离nonce拒绝；有效重签同nonce及完整坏签名/expiry/clock-skew/attachment组合仍planned，按`feedback.rs`与`feedback/replication.rs`分别构造本地与复制入口oracle。 | 同source/config的断言、状态前后及真实exit/log；尚未执行 | 不证明跨机可靠交付、恢复、finality或release |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | [des-feedback-public-read](#des-feedback-public-read) | 公开读、audit与限流的有限能力 | [testing-manual](../../../testing-manual.md)；`feedback/tests.rs::feedback_rate_limit_blocks_excessive_submissions` 与 feedback_rate_limit_skips_stale_audit_paths_before_reading 核基础限流和过期audit路径跳过；audit写失败后账本状态与隐私范围没有完整执行证据，故障注入planned。 | 同source/config的断言、状态前后及真实exit/log；尚未执行 | 不证明跨机可靠交付、恢复、finality或release |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | [des-feedback-fetch](#des-feedback-fetch) | announce只含元数据；found、payload、hash与size验证后入库 | [testing-manual](../../../testing-manual.md)；`feedback_p2p.rs::feedback_announce_ingest_rejects_blob_hash_mismatch` 只断言hash mismatch返回错误；feedback_announce_build_encode_decode_ingest_roundtrip覆盖构建/编码/解码/复制后公开视图。found=false、missing payload、size/actor/world隔离及未写入完整负例planned；网络授权由replication拥有。 | 同source/config的断言、状态前后及真实exit/log；尚未执行 | 不证明跨机可靠交付、恢复、finality或release |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | [des-feedback-idempotence](#des-feedback-idempotence) | 重复announce幂等及冲突拒绝 | [testing-manual](../../../testing-manual.md)；`feedback/tests.rs::feedback_replicated_ingest_is_idempotent_for_root_and_event` 与 feedback_replicated_ingest_rejects_event_id_conflict；`feedback_p2p.rs` roundtrip重复append后断言append_events.len()==1。 | 同source/config的断言、状态前后及真实exit/log；尚未执行 | 不证明跨机可靠交付、恢复、finality或release |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | [des-feedback-start-gate](#des-feedback-start-gate) | replication、network及BlobState publish+subscribe前置 | [testing-manual](../../../testing-manual.md)；`crates/oasis7_node/src/tests_hardening.rs::runtime_start_rejects_observer_feedback_p2p_without_blob_state_lane_access` 核InvalidConfig及lane错误原因；全部缺config/network/role组合planned；测试不证明全服务角色权限矩阵。 | 同source/config的断言、状态前后及真实exit/log；尚未执行 | 不证明跨机可靠交付、恢复、finality或release |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | [des-feedback-network-wiring](#des-feedback-network-wiring) | start前挂载、effective peers与显式topology fail closed | [testing-manual](../../../testing-manual.md)；`crates/oasis7_node/src/libp2p_replication_network/tests.rs` 的 libp2p_replication_network_request_rejects_without_connected_peers_by_default 与 libp2p_replication_network_request_falls_back_to_local_handler_when_enabled 分别核默认拒绝、显式允许返回handler结果；binary的CLI/manifest优先级和start前接线完整组合planned。 | 同source/config的断言、状态前后及真实exit/log；尚未执行 | 不证明跨机可靠交付、恢复、finality或release |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | [des-feedback-outbox](#des-feedback-outbox) | 有界内存outbox、每tick发布及失败回排 | [testing-manual](../../../testing-manual.md)；`crates/oasis7_node/src/tests_hardening.rs::runtime_feedback_submit_publishes_and_peer_ingests` 仅核in-memory network单条publish/peer公开读取及重复announce；队列满后账本已写、失败回排顺序、并发容量与重启边界按node_runtime_core.rs/feedback_runtime.rs故障注入planned。 | 同source/config的断言、状态前后及真实exit/log；尚未执行 | 不证明跨机可靠交付、恢复、finality或release |
| [professional_acceptance: 接口 / 数据](distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data) | [des-feedback-incoming-limit](#des-feedback-incoming-limit) | 每tick处理上限、drain语义及局部失败观测 | [testing-manual](../../../testing-manual.md)；静态入口`crates/oasis7_proto/src/distributed_net.rs`、`feedback_p2p.rs`、`crates/oasis7_node/src/feedback_runtime.rs`和`lib.rs`；planned入站超过limit、坏条目混合好条目、错误汇总及后续tick进展断言。单条复制happy-path测试不覆盖这些条件。 | 同source/config的断言、状态前后及真实exit/log；尚未执行 | 不证明跨机可靠交付、恢复、finality或release |

## 12. 决策、风险与未决问题

### 非承诺

采用存储层记录+发现提示，避免将反馈事件绑定到共识或世界结果。主要风险是账本成功而enqueue/audit失败、瞬时队列压力、drain溢出丢弃、重启pending丢失、错误/冲突条目及显式topology无peer。现有错误观测不能独自保证重试、补偿或不丢消息；这些限制必须在使用该服务时显式考虑。

运行前提或安全/产品语义需要变化时，由DistFS、NodeRuntime、network owner与QA重新审读。当前源码与目标差距保持可见；不借文档规范化修改代码、产品要求、签名/expiry政策、收费规则或CI。
