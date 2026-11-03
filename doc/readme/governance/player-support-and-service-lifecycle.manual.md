# 受控预览玩家支持：制作、操作与验证手册

- 主责：`liveops_community`；产品边界由 `producer_system_designer` 审核，候选证据由 `qa_engineer` 复核，技术处置归对应专业 owner。
- 适用范围：[支持内容设计](player-support-and-service-lifecycle.design.md#support-scope)中的现有反馈路径及受控预览轮次。手册不是当前邀请、外发、后台操作或数据删除授权。
- 内容源：[可复用中文文案](player-support-and-service-lifecycle.design.md#support-copy)。操作记录使用当前 PR、原 Issue 或既有轮次记录，不新建平行工单/个人信息台账。

<a id="prepare-support-content"></a>
## 1. 准备输入与选择执行方式

1. 读取根 [README 项目状态](../../../README.md#项目状态)，确认实际候选、primary mode、入口、适用环境及是否确有本轮活动。历史 invite pack 不恢复为当前许可。
2. 核对反馈入口与发布渠道的当前可用性；静态审读只能证明源码入口存在。未获授权或不能连接真实入口时使用下文桌面案例，明确记录未验证真实可用性。
3. 为本次内容记录四类输入：当前事实、产品目标、假设、证据边界。只采用实际支持的事实，不把设计目标或其他候选的通过记录填为当前结论。
4. 确认本轮实际接收 owner 和真实下一步；转交未完成的有效信号仍由接收 owner 承接。没有实际执行者时，不声称受理闭环已经就绪，交 producer 决定继续、暂停或重新评估；不要用未确认人力发出新的邀请或回复时限。
5. 选择设计中的完整文案，填入实际变量。检查内部证据是否可公开；公开稿只保留必要事实和允许公开的来源。没有真实事件时使用标注为“桌面案例”的虚构输入，材料保留在当前 PR，不外发。

本文的只读静态检查和桌面推演不运行游戏、UI、Cargo 或模型，也不提交反馈或发布消息。后续若获准做真实入口验证，按 Launcher/站点原测试方法另取候选证据，不能把本文的文字检查记为运行 PASS。

<a id="support-entry-check"></a>
## 2. 核对入口与提交结果

### 操作

1. 对照[入口矩阵](player-support-and-service-lifecycle.design.md#support-entry-results)读取 native `feedback_window.rs`、`feedback_entry.rs`，Web `feedback_window_web.rs` 及其控制面请求。
2. 核对门控发生在 native helper 调用前；不能从 helper 可单独落盘推断 UI 在链禁用时也会保存。
3. 对照实际结果或下表虚构结果选择一段说明，标出可确认的事实、不能确认的部分和玩家下一步。
4. 检查没有把窗口打开、按钮点击、请求中、本地路径或 feedback ID 当作人工处理、全网交付或世界结果。

### 案例与判据

| 案例输入 | 应选择的说明及通过判据 | 失败条件 |
| --- | --- | --- |
| GitHub 表单仍在编辑，没有新记录 URL | 尚未提交；保留脱敏草稿 | 声称团队已收到或工作已启动 |
| native 链未就绪，窗口拒绝提交 | 未完成；不声明本地包；可在说明中给已有 GitHub 入口 | 凭 helper 行为声称 UI 已保存或已有修复 CTA |
| native Ready，返回虚构 `feedback_id=sample-fb`、`event_id=sample-event` | 只确认远端路径受理；两标识明确属于桌面案例 | 写成全副本已同步、人工已看或游戏完成 |
| native Ready，远端错误后返回本地样例路径 | 只确认本地保存；远端是否部分写入仍未确认；分享前检查 | 写成未曾远端写入、已上传给团队或自动重投 |
| native writer 失败 | 失败；不说完整文件已保存 | 忽略错误并给出成功文案 |
| Web 请求中或代理失败 | 如实说明未确认；不声称 native 文件回落 | 写成已本地保存或自动转到 GitHub |

真实入口验证还须分别取得请求/响应、实际路径或 ID、门控和 UI 证据；本表只验证内容选择与结果语义。

<a id="support-triage-check"></a>
## 3. 受理、分流、告知与复核

### 操作

1. 检查反馈是否已有原记录；有则关联并保留新事实，不反复建同一问题。
2. 从玩家文字提取版本、模式、实际表面、平台、最短步骤、预期/实际及影响。缺少复现必需信息时仅追问缺项；不要请求完整配置、私密记录或个人身份。
3. 保持 `Bug / Suggestion` 原输入类别，按实际影响人工分入 `Blocking / Opportunity / Idea`，保持明确的当前 owner；专业转交尚未确认时仍由实际接收 owner 负责下一步。
4. QA 核实 blocker 时，将同候选受影响入口及原证据交回专业 owner 与 QA gate；producer 决定 continue / hold / reassess。原通过结论不能覆盖已确认的新阻断。
5. 在原记录写分类、理由、实际转交/处置、下一步及关联。只有真实完成时才使用“已转交”“已验证修复”等句子。
6. 收到复核请求时保留原结论和新增脱敏事实，说明由谁核查什么。新的证据可以改变判断；关闭、归并和无证据都不证明世界结果可被改写。

### 桌面样例

使用虚构版本 `sample-candidate-A`、`viewer`、native Launcher，标题“反馈窗口显示区块链未就绪”，步骤“打开窗口，填写标题/描述，尝试提交”，预期“得到真实可用性与结果说明”，实际“门控拒绝，无保存结果”。先判入口拒绝是否符合现有合同，不能仅凭玩家预期把它升级为新 fallback 承诺；若缺少可读说明，再将具体理解摩擦归给对应内容/入口 owner。

再加入三条独立信号：同候选启动崩溃、缺少安装说明、新机制想法。应分别得到基于影响的 Blocking、Opportunity、Idea 及 QA/文档 owner/producer 去向。给一条旧“已修复”记录加入新平台复现后，应请求该平台核查，而非直接复用旧结果。

通过条件：每条有效信号都有原记录定位、分类、明确当前 owner 和真实转交状态、处置理由和下一步；没有新协议字段、五类之外的新 Issue 类型、逐条人工回复或修复时限承诺。

<a id="support-data-check"></a>
## 4. 检查公开性、最小信息与误传处置

### 数据路径核对

1. 读取 `feedback_entry.rs::submit_feedback_remote` 与 `feedback_window_web.rs::submit_feedback`，核对远端只发送 category/title/description/platform/game_version，当前版本字段为 `unknown`。文案仍要求玩家提供可核实版本或如实写未知。
2. 读取 `submit_feedback_report` 和 `capture_feedback_diagnostics`，核对本地包才附带 allowlist 配置摘要及最近最多 200 行自动日志。手写标题/描述不在清洗保证内；已有专业缺口不能被文字检查关闭。
3. 读取 `feedback.rs` 的 root/event/public view 与 `to_public_view`，确认 tombstone 后原 content 和 append events 仍在公开视图中。
4. 读取 `feedback/replication.rs` 和 runtime `status_server_support.rs`，核对复制保留 root 的提交 IP，而 IP 是连接 peer，可能是本地或代理。不得从公开视图省略 IP 推断原记录私密，也不得说系统必然收集玩家公网 IP。核对 signer 来自节点派生，不能把作者公钥或返回 ID 说成已验证玩家身份。
5. 读取 `feedback_submit_api.rs` 的 TTL 使用位置，确认七天用于签名请求 expiry；没有保留期限或自动删除结论。
6. 核对 GitHub/渠道仅按当前已验证平台能力说明；未核实的保留、隐藏、删除或私密受理一律不填“已支持”。

### 误传演练

只使用明显虚构的字符串，例如 `EXAMPLE_NOT_A_SECRET`，不制作或复用真实凭据样本。

| 情境 | 可执行动作与必须告知的边界 |
| --- | --- |
| 敏感内容仍在未提交草稿 | 在持有人控制的草稿移除；无需收集该内容作为证据 |
| 内容在 native 本地包，尚无分享事实 | 检查包，公开只取必要脱敏片段；本地文件操作按持有人权限，不能代替远端撤回 |
| 玩家称已发送到远端，但只有错误 | 记录安全定位和不确定性；技术 owner 核真实写入，不直接判定“没有泄露” |
| 内容已进入公开追加记录 | 停止人工再次转发/引用；核查实际可用处置，明确 tombstone 不隐藏原文、不保证全部副本删除 |
| 需要私密核查且无已验证入口 | 公共记录只留安全摘要；不索取原始材料，owner 确认下一受理方式前保持未核查 |

通过条件：说明同时覆盖用途、字段、访问与保留/删除限制；没有把自动清洗、tombstone、关单、签名到期或停止人工转发说成彻底删除/停止复制；没有使用虚构私密联系方式。实际误传处置需另核授权和执行结果，本演练不执行删除、网络停止或对外通知。

<a id="verify-claim-evidence"></a>
## 5. 检查文案、证据与发布状态

1. 从设计 §7 选取实际要用的文案，填完所有事实变量；以[原沟通模板](readme-release-communication-template.md#meta)记录候选、模式、入口与来源。
2. 在内部记录分别列当前事实、产品目标、假设、证据边界。逐句核对公开稿是否只陈述被来源支持的范围；需要出现目标或假设时明确标记。
3. 核对根 README 与同窗专业/QA 输入。若使用历史记录，写出原版本和窗口；不能把历史邀请恢复成当前开放。
4. 找出本次受影响的活跃公开表面及 CN/EN 配对，检查相同事实、限制和下一步。只交付中文源时明确英文与界面接入未验证。
5. 区分草稿、审核、可发布和实际发布。未取得渠道、时间、URL/message ID 与发布 owner 时，结果只能是文案准备或审核，不能是“已发布”。

负例：把 sample-candidate-A 的结果用于 B；把 pure_api 证据用于 viewer；将“将来计划支持”改为“现在支持”；只纠正中文而保留英文旧开放状态；把 PR 合入写成已通知玩家。任一负例仍被文案接受，本项不通过。

实际外发仅在既有授权内进行，且须发布后回读真实呈现并记录定位。本文编写和桌面验证不执行该动作。

<a id="verify-incident-communication"></a>
## 6. 事故、纠正与恢复演练

1. 使用一个明确标为虚构的场景：sample-candidate-A 的 Web 入口出现无法确认提交结果的报告。首条说明只能写已知现象、范围、当前安全下一步和信息更新时间；原因、损失与恢复时间未知。
2. 假定技术 owner 随后确认影响该入口，producer 决定暂停该范围的新尝试。写暂停说明并列需纠正的旧活跃文案；不能说暂停已取消原请求或停止全世界。
3. 假定技术动作完成但 QA 尚未验证。选择“恢复核验中”，保留限制，不能写“已恢复”。
4. 假定 QA 只验证同候选的指定 Web 路径。只对这个范围写恢复说明，保留其他入口、历史请求和残余风险。
5. 用纠正句说明旧表述何处失效。LiveOps 记录沟通结果，技术 owner 记录实际恢复；消息撤回、软件回滚、QA 结论与世界恢复分别定位。
6. 每步给出实际可安排的信息更新时间；不能确定时明确尚未确认并由 owner 继续落实，不能填假时间或推导恢复 ETA。

通过条件：阶段转换均有来源，玩家知道可做什么、不能从当前信息判断什么；旧文案不继续背书，未复核范围不升级；支持回复没有新增资产、返还投入、重写结果或宣布风险清零。

<a id="verify-world-boundaries"></a>
## 7. 损失、疑似滥用与恢复边界检查

对照[损失说明](player-support-and-service-lifecycle.design.md#support-competition-reports)分别审校三个虚构案例：

| 案例 | 通过判据 |
| --- | --- |
| 权威规则和结果可证的竞争损失 | 说明实际损失、保留身份/历史及当时已支持的恢复选择，不承诺免费复原 |
| 同时声称存在骚扰或越权，但公开证据不足 | 将规则内结果与疑似越界分开；只追问安全摘要，保留未核查和专业 owner，不以竞争合法概括否定举报 |
| 技术修复或支持回复存在，但没有恢复贡献的权威 receipt | 不声称损失已恢复、补偿已取得或恢复项目已完成；按原专业结果核查 |

任何“结束预览即可清空历史”“管理员已保证还原资产”“重新登录自动重做原动作”均判失败。此处仅审校文字，不创建处罚、世界动作或恢复项目。

<a id="verify-pause-and-close"></a>
## 8. 暂停、结束与交接

### 操作

1. producer 确认实际关闭对象、生效时间及仍可用范围；渠道暂停、本地停止、候选入口受限、便利服务中断分别写清。
2. 从原记录找未结反馈，保留分类、owner、当前结论和下一步。没有后续安排就标未结并交接，不能为结束轮次批量宣称已解决。
3. 对进行中行动分别记录“权威确认尚未生效”“结果未知待核对”“已确认结果”；只引用玩家有权取得的信息。没收到回执不是确定失败，停止新尝试不是取消旧请求。
4. 对本地包、公开反馈及既有原始证据分别写实际处理方式。没有执行过的导出、删除、隐藏或全副本清理不得勾为完成。
5. 准备结束文案并按 §5 审校；已授权发布后回读，撤下或纠正受影响的旧邀请，保留实际可用的后续说明定位。
6. 将剩余事项交原 owner 或实际接手者。若持续处理所需人力/窗口无法提供，producer 明确收窄或暂停新的收集，不制造固定客服时限。

### 桌面案例与判据

构造一轮同时有：一条未结 Blocking、一条已归并反馈、一个提交结果未知的世界请求、一份本地包和一条已公开追加反馈。为它们分别写交接与结束文字。

通过条件：每项去向可查；轮次结束不取消世界义务，不抹去未知结果；本地保管、公开历史和处置权限分开；无免费补偿、商用退款、自动导出或新私密入口承诺。另构造“未来一项持续服务将结束”的情形，应识别必须由产品及服务 owner 另定数据、期限、入口和未结义务，不能直接套用本轮结束稿宣称设计完成。

<a id="static-document-checks"></a>
## 9. 静态检查、记录与不通过处理

在选定候选工作区只读检查本次文件的真实路径、fragment、Trace 及表格关系；仓库级治理检查由当前 PR 负责人按现行流程组织。以下命令只读取内容，不运行产品或模型：

```bash
rg -n '^(##|###)|<a id=|Trace refs:' doc/readme/governance/player-support-and-service-lifecycle.design.md doc/readme/governance/player-support-and-service-lifecycle.manual.md
rg -n 'game_version|submit_feedback_remote|submit_feedback_report' crates/oasis7_client_launcher/src/feedback_entry.rs crates/oasis7_client_launcher/src/feedback_window_web.rs
rg -n 'FeedbackRootRecord|FeedbackPublicView|fn to_public_view|submit_ip' crates/oasis7_distfs/src/feedback.rs crates/oasis7_distfs/src/feedback/replication.rs
rg -n 'FEEDBACK_TTL_MS|expires_at_ms|peer_addr' crates/oasis7_chain_runtime/src/host/feedback_submit_api.rs crates/oasis7_chain_runtime/src/host/status_server_support.rs
```

实际 PR 记录受检提交/工作区、所用方法与案例、检查者、结果和未覆盖范围；不用本手册保存每次进度。仅有静态审读时写静态结果，运行栏保持未执行。资料错误修正源文案与受影响消费者；入口、权限、数据或恢复合同冲突交原专业 owner，依赖该结论的发布暂停，其他已获授权内容继续处理。

本方法不证明 native/Web 可用性、隐私清洗完善、文件并发不覆盖、反馈复制可靠、真实玩家支持效果或正式发行。需要这些结论时执行原专业验收并保存实际证据，不能增加一句“已检查”替代。
