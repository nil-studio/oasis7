# 指挥中心本机工具集成

- 文档 ID：`DESIGN-ENGINEERING-COMMAND-CENTER-LOCAL-TOOLS`
- 状态：`draft`；实现及验收以实际 PR 为准
- owner_role：`runtime_engineer`
- module：`engineering`
- 上游：[指挥客户端产品设计](command-center.prd.md)、[系统设计](command-center.design.md)
- last_reviewed_at：2026-10-17

## 1. 目标和当前范围

在现有 macOS 指挥中心增加“本机工具”，使用普通用户 Rust 宿主复用 context 资料库，管理同包 CLI，接入已有兼容 signer。用户提供的 2026-10-12 本机工具设计是完整目标；本文记录当前实现合同及实际剩余范围，不将目标写成完成事实。

用户于 2026-10-12 明确暂停 Apple 代码签名和管理员助手工作，待背景讨论后继续。因此 SMAppService／认证 XPC／Authorization Services、signer 初装、密钥与 policy/grant 管理、受保护加密密钥导入导出和 signer 备份计划恢复均未开放。当前 Mac 未检测到可用 Apple 代码签名身份；此观察不等于所有 Mac 均不支持。已有兼容安装的普通 caller 不受该暂停影响。通用升级、自动更新、其他 purpose UI、远程安装和动态插件仍不在首版范围。

<a id="DES-CC-TOOLS-001"></a>

<a id="2-普通用户能力"></a>

## 2. 普通用户能力与边界

context 的 `application` 应用接口共享显式资料 root、完整原文校验、条件保存、历史恢复身份校验及原快照实现；CLI 不在 GUI 进程中运行。`info --json` 固定 schema 1，未知构建来源返回 null；它不打开资料库或 custody。caller 的 info 和其 context info 分别表示各自能力。

资料绑定持久保存真实 root、设备号/inode、store ID、原 store 项目 ID、面板关联项目 ID 和单调版本。日常条目读写按原绑定核对目录及 store 身份。Git 历史读取与恢复通过持有的原目录身份执行，并在完整读取操作期间核对绑定路径和 Git 元数据目录／gitfile 目标身份；资料根目录或 Git 元数据被更名、替换时拒绝，不从复用路径读取另一个资料库。普通项目和私有资料目录双向包含均拒绝，普通索引、缓存回退、搜索、阅读历史访问和来源打开都检查私有隔离；用户历史数据保留而不经普通入口暴露。正文只按需读取，不持久复制到普通文档索引。Git 元数据采用各子命令前后及返回前的持续身份重核验，实际 Git 仍按路径访问元数据；不将该核验描述为原子快照，也不声称消除在全部核验之间发生并恢复的瞬时 ABA 替换。

简化表单修改完整原文中的负责字段，保留其余合法元数据。新建是 create-only，修改提交读取时 revision；CLI 与 GUI 使用原跨进程写锁。冲突保留草稿，重新读取后可再次保存；已确定未写入的错误不能锁死目标。保存、回读或重新加载期间的新编辑以草稿世代检查保留；继续保存只采用本次确切保存修订做 CAS，不采用未展示的外部较新修订覆盖并发修改。新建绑定失败且已证明无副作用时只回收当前保留记录世代中所有操作声明均已证明无副作用的新建私有目录保留记录，不解除既有绑定或其他运行中／未知操作的隔离。资料落盘、Git 提交和远端同步是独立事实。

加密资料快照沿用 context 的明确选择范围、create-only 输出和新目录恢复。口令通过绑定窗口／操作／用途的随机短时句柄进入宿主内存，一次消费、两分钟过期，不持久化到任务或日志。恢复在目录和项目校验前消费正确窗口／操作／用途的句柄；后续拒绝或对话框取消时销毁已消费口令，错误窗口／操作／用途不消费原请求的句柄。快照在独立阻塞 worker 中执行，不持普通项目索引锁。 恢复不要求原资料库仍可读取：已连接库只消费保存的 store／项目身份，新机器可不提供旧绑定，由口令认证的同一份密文确定资料库身份。当前普通项目关系由宿主固定；新目标先登记私有排除，再写入明文并建立绑定。恢复准入同时固定独立暂存目录名称；暂存创建后出错按可能已写入处理，核对最终目录缺失时也核对原暂存，不将保留明文判为无副作用。恢复、证据和核对始终绑定原密文摘要与条目修订，不因源目录丢失重放未知操作。

可选 CLI 与 App 同次构建，catalog 固定提交、release、target、大小和 SHA-256；桌面目标架构必须同步传入 CLI 构建及清单，不支持的聚合目标在构建前拒绝；仅复制核对后的资源到用户受管版本目录，不覆盖不同内容的已有文件。实际绝对路径可调用，PATH 可见性单独显示；不临时下载或从项目执行安装脚本。

### 2.1 需求承接与分配表

| 上游条款 | 独立义务与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖 |
| --- | --- | --- | --- | --- |
| [REQ-CC-015](command-center.prd.md#REQ-CC-015) | 本机资料、保存、历史、快照和同包 CLI；产品适用性：N/A；reason=普通开发者本机资料与工具集成，不定义游戏授权、结算或发行能力；scope=本机资料、保存、历史、快照和同包 CLI | [DES-CC-TOOLS-001](#DES-CC-TOOLS-001) | context 领域库、runtime_engineer | 不将落盘当作 Git 远端同步 |
| [REQ-CC-015](command-center.prd.md#REQ-CC-015) | 已有兼容 signer 的普通文件签名；产品适用性：N/A；reason=普通开发者本机资料与工具集成，不定义游戏授权、结算或发行能力；scope=已有兼容 signer 的普通文件签名 | [DES-CC-TOOLS-002](#DES-CC-TOOLS-002) | signer caller、原 worker | 用户暂停特权初装/管理 |
| [REQ-CC-015](command-center.prd.md#REQ-CC-015) | 未知操作、资源占位及冻结退出；产品适用性：N/A；reason=普通开发者本机资料与工具集成，不定义游戏授权、结算或发行能力；scope=未知操作、资源占位及冻结退出 | [DES-CC-TOOLS-003](#DES-CC-TOOLS-003) | ToolJobs、原 context/signing 持久记录 | 不从进程消失推导回滚 |
| [REQ-CC-015](command-center.prd.md#REQ-CC-015) | 显式私有输入和持久线程属性；产品适用性：N/A；reason=普通开发者本机资料与工具集成，不定义游戏授权、结算或发行能力；scope=显式私有输入和持久线程属性 | [DES-CC-TOOLS-004](#DES-CC-TOOLS-004) | 实验 runtime、agent_engineer | 不保证原 runtime/外部 CLI 全部转存追踪 |
| Trace refs: O7-REQ-fd3b34c6-4fb4-4614-8d7b-744a86d29c3a；[REQ-CC-015](command-center.prd.md#REQ-CC-015) | 从工作进入工具、明确选择操作范围并核对结果；产品适用性：N/A；reason=开发者本机辅助工具体验，不定义游戏或玩家功能；scope=资料、快照、已有 signer 与 CLI 的任务入口 | [DES-CC-TOOLS-005](#DES-CC-TOOLS-005) | Solid UI、ToolJobs、原 context/signing 接口 | 不扩大已有工具权限，不把打开工具当作工作完成 |
| Trace refs: O7-REQ-76d54ed7-4028-460a-b22b-37a9088ad245；[REQ-CC-010](command-center.prd.md#REQ-CC-010) | 工具表单就地反馈、可访问错误和诊断分层；产品适用性：N/A；reason=开发者桌面交互质量，不定义游戏或玩家功能；scope=本机工具表单与操作结果 | [DES-CC-TOOLS-005](#DES-CC-TOOLS-005) | Solid UI、原生文件选择与焦点管理 | DOM 测试不替代原生交互验收 |

<a id="DES-CC-TOOLS-002"></a>

## 3. 已有 signer

后端只读取固定安装描述并核对普通 caller UID、受保护实际路径、release manifest 和 executable SHA-256。未知或不兼容安装保留原状；不经普通 App 读取 custody。

绑定证据从同一个持有的资料目录读取库配置和目录身份；执行核对原身份，初始化、发布与绑定持续使用原对象，拒绝采用后来出现的不同库。恢复后的绑定直接读取已发布目录句柄，不再按路径重开。快照导出也固定原输出父目录；以目录句柄祖先身份排除资料库内部输出，目录别名或源目录改名不能绕过限制。

桌面 CI 对普通 PR 和 main 推送均监听 Cargo 包装驱动、实际 Python/helper 输入及工具链配置，避免只改包装驱动时跳过 App/CLI 验收。CLI 包装只接受可核验的已提交源输入，构建前后检查 tracked、untracked、资料 CLI 源目录中被忽略的输入及 HEAD；清单查询失败禁止发布。CLI 安装和中断核对共用普通用户目录中的跨进程锁；原安装仍活跃则保留未知，无最终发布时解除原占位供明确新安装，残留暂存和其他版本不删除。

文件签名要求明确 file、job、signer、grant 和完整合法 SignContext；不自动填示例值。只读准备校验明确确认无副作用时，表单保留草稿并允许修正；进入原 caller 准备边界、旧操作或结果未知时仍冻结原 Job，先核对，不能另建操作绕过未知结果。原 caller 第一次 prepare 前重新核对原准入的源文件摘要、大小、设备和 inode，禁止采用替换文件的新身份。第一次 prepare 建立私有 job，原 Rust 类型生成 context、file envelope 和 canonical JobInput，第二次 prepare 固定原请求。提交和 inspect 绑定原 job；界面核对结果时传入原预览指纹，已核实 committed 后才允许显式开始新请求并清空表单及临时操作 ID，原 Job 和账本保留。未知或未完成结果不能清空绕过；准备响应丢失时核对原 operation ID、安装、源文件和冻结 envelope，成功后恢复原 prepared 身份及提交入口，表单仍冻结且不重放 prepare。提交把原 IPC 请求及安装发行身份摘要传入 caller；caller 在实际请求构成后、调用 worker 前核对，旧安装缺少 frozen_submit_v1 时在工具操作准入前拒绝提交，不创建 unknown 资源占位；实际发送边界再次核验，不能降级绕过。准备的非秘密固定输入与原 operation 在同一事务保存；重启后从操作记录显式恢复原请求，重新核对源文件、安装、原输入和同一份已验证准备快照的指纹，随后显式 inspect 原 Job 才开放提交。没有恢复证据的旧记录明确拒绝，不从表单猜测，不重放 prepare。退出冻结拒绝只在锁内证明没有原操作、没有同资源运行中或未知记录且未写入时返回无副作用；旧记录或不确定存储错误保持未知。准备不等于 worker 已授权。独立验签重建相同 context 的 envelope，并核对用户明确的预期公钥。选中文件必须是有界普通文件，不接受 FIFO、符号链接或读取中变动。

缺少密钥、policy、grant 或 purpose 配置时明确显示未配置；当前暂停的管理员集成不能用 shell/sudo 透传代替。

<a id="DES-CC-TOOLS-003"></a>

## 4. 操作与退出

工具操作先在 SQLite 事务中原子登记 operation ID、输入指纹、不可变目标资源及动作。相同 ID/输入回读既有记录，不重放；不同输入拒绝；已有 running/unknown 占位防止重复副作用。signer 的 ASCII Job 资源按大小写保守共用占位，以覆盖常见 macOS 目录别名；回读核对也使用同一 ASCII 资源别名规则，核对原操作的全部不可变指纹后才解除 unknown。原输入指纹和 Job ID 仍精确核验，其他工具资源保持精确比较。终态不倒退，崩溃中的任务转 unknown，不由进程消失推导回滚。

快照恢复从原证据核对父目录身份，并通过持有的目录句柄完成写入和发布；替换路径不接收正文。父目录在执行中改名时，结果可能位于改名后的原目录，操作保持未知并拒绝绑定替换位置。

完整退出冻结同一准入机制，并检查所有项目的 Codex 和 ToolJobs；运行中或未知效果阻止安全退出声明。窗口关闭仍留后台。Codex 新动作、单轮中断及原请求回复与冻结使用同一锁协调；等待退出时不发送批准、回答或新的中断操作；明确“停止并退出”由退出协调入口执行，取消退出后保留原请求和未发送草稿。已登记的后台任务继续回读。退出状态检查失败时解除工具冻结；执行数据库不可用时仍可取消等待并使用普通工具，但历史终止保持未知，不声明安全退出。冻结、取消和退出批准在同一执行状态锁下串行核对。口令和正文不作为恢复参数。

<a id="DES-CC-TOOLS-004"></a>

## 5. 显式私有资料输入

在原自管会话选择资料并预览来源修订与整条或明确行范围。发送时后端重读同一 binding/version/revision，变动要求重新预览。草稿、资料正文及来源说明合成后的 UTF-8 总量不得超过 32768 字节，不截断；界面显示可用草稿预算，后端发送前再次核验。只读资料/修订校验及纯输入超限明确报告无发送效果并保留草稿；执行或 RPC 结果不明继续冻结。发送正文前持久登记线程私有来源；失败不发送。原线程后续普通轮次、恢复、模型派生请求与审批仍保持私有属性。

事件写入及历史读取从独立私有来源表恢复标记，迟到 snapshot 不能清除。普通 runtime 投影只过滤属于私有线程的会话、请求及操作；无线程 ID 的普通管理操作仍保留，未知建会话回执不能误隐藏。专用运行界面保留必要批准详情。既有数据库可能包含由私有输入派生的请求内容；不能直接复制整份 SQLite 并称为普通配置备份。原运行时或外部 CLI 的转存不在本 App 完整追踪范围内。

<a id="DES-CC-TOOLS-005"></a>

## 6. 按任务组织工具入口与结果反馈

本节是目标交互合同。`main@0ec10379419b0aaedb9d16157b81c92f4e6e3495` 的 [LocalTools](../../../tools/oasis7-command-center/ui/src/LocalTools.tsx) 已提供基础表单和操作回读；任务入口、独立口令表单、就地反馈及完整往返尚未完成。本节复用 §2–5 的接口、ToolJobs、私有隔离和退出规则，不建立第二套工具授权或操作账本。

### 6.1 入口、能力与操作范围

本机工具首页按“资料”“备份与恢复”“签名与验签”“CLI”提供任务入口，先说明可完成的结果、当前可用性及一个下一动作；仅打开选中任务的表单。未连接、读取失败、不兼容和用户暂停分别显示，按每项任务的真实前提开放动作：建库/连接不要求已有绑定，新机器恢复不要求旧库可读，安装同包 CLI 不要求已有 CLI，独立验签不要求安装或连接 signer。暂停能力保留原因及原合同入口，不展示可提交的配置表单；已有兼容 signer 的普通 caller 仍可使用。

从统一工作详情进入时，传入原工作引用、明确执行环境及返回位置；这些只是导航上下文，不携带授权或推导签名输入。工具表单从宿主读取当前真实绑定与能力，不能以页面缓存启用动作。退出工具返回原工作和筛选；普通详情最多显示获准公开的操作类型、状态及核对入口，不复制私有库名、路径、条目、口令、完整请求或派生正文。私有资料与签名结果仍在其原受控入口阅读。

| 用户任务 | 提交前可见的选择与核对 | 操作结果与下一入口 |
| --- | --- | --- |
| 保存资料 | 选中资料库、条目及编辑修订；冲突保留本次草稿并显示重新读取入口 | 明确“资料已保存”；Git 提交和远端同步分别核对，不跟随保存自动执行 |
| 导出加密快照 | 明确选择“当前条目”或“整库”，显示范围、可得数量、原修订、输出位置及独立的导出口令输入 | 显示已核实输出及范围；范围变化要求重新预览，不因清空选中条目静默改成整库 |
| 恢复快照 | 密文文件、新目录和独立的恢复口令输入；说明不会覆盖现有目录 | 由原恢复操作核对发布及绑定；失败或结果未知时进入同一操作的检查入口 |
| 文件签名 | 显式选定源文件、job、signer、grant 和完整合法 SignContext；准备后展示原预览与授权范围 | 准备、提交、worker 已核实结果分别显示；未知状态继续核对原 Job，不新建请求绕过 |
| 独立验签 | 显式文件、完整 context、预期公钥与签名 | 显示匹配或具体不匹配项；通过仅证明该组输入，不赋予执行授权 |
| 安装同包 CLI | 已验证包版本、目标绝对路径及当前安装差异 | 安装完成与 PATH 可见性独立显示；原版本保留规则沿 §2 |

导出与恢复口令分别绑定当前窗口、操作及用途。表单只申请自己的短时句柄，不读取另一任务的输入；取消、完成、过期或离开该口令步骤时清除对应 UI 临时值和未消费句柄，重新进入重新输入。已消费句柄与正在执行的原任务按 §2–4 处理；清除 UI 不取消、回滚或重发操作。口令不进入普通草稿、返回栈、错误样本或日志。

首个导入入口只接收用户明确选择的完整 SignContext JSON，复用 [SignContext](../../../crates/oasis7_local_signer/src/protocol.rs) 的严格字段与校验，不另创请求格式。文件读取沿 [file CLI](../../../crates/oasis7_local_signer/src/file_cli.rs) 的普通文件、身份、摘要与 64 KiB 预算；预览固定读取到的原文及摘要，内容改变须重新导入。job、signer、grant 和源文件仍由用户分别明确选定；已准备请求通过原 operation/Job 的恢复入口打开，不从任意 JSON 导入成已准备或已授权状态。前端逐字段显示来源、缺失和校验结果；后端仍对完整输入、实际文件、安装和原请求重新校验。缺 job、grant、purpose/context 等输入时指出具体缺口，不生成示例授权、不从工作目标推导，也不将 JSON 解析成功等同 worker 授权。

### 6.2 每次操作的反馈与恢复

每个任务表单拥有自己的提交状态、字段错误、结果区域和原 operation 引用。主要按钮只描述当前可执行步骤；不可用时就地给出实际原因与修复入口。错误留在发起位置附近，定位相关字段；异步结果通过可访问状态区域通知，不抢用户正在编辑的焦点。用户显式查看结果或返回时，才定位相关操作、结果或原触发控件。成功结果先用用户可理解的结论和下一动作表达，原始 JSON、完整摘要与协议字段在诊断详情按需展开。

提交前校验失败保留草稿且不声称已执行；宿主证明无副作用的失败允许修正后再次明确提交。已经运行或效果未知时显示原操作的核对入口，遵守原资源占位与冻结，不能把一般“重试”映射成新操作。迟到结果必须匹配窗口、原操作、输入版本及表单世代，不能覆盖用户新任务或新草稿；回读可更新原操作历史。独立工具的局部失败不清空其他资料或冻结无冲突任务；完整退出仍由既有统一协调入口决定。

界面职责拆为工具总览、单任务表单和操作结果组件；共用 bridge/ToolJobs，不在 UI 再执行 CLI、重算签名授权或推断持久化成功。读取状态可以独立刷新；写操作返回后优先回读受影响对象，不以连续重读全部工具状态作为显示本次结果的前提。

<a id="11-验收与验证"></a>

## 11. 验收、验证与剩余风险

| 合同 | 验证入口 | 仍需真实环境核对 |
| --- | --- | --- |
| 资料条件保存、未知元数据和外部修改 | `core/tests/context.rs`、context CLI/core tests、LocalTools UI tests | 同包 App 原生选择、重开、实际 CLI 并发 |
| 双向隔离、目录替换和旧索引/历史 | `core/tests/context.rs`、foundation/document_lifecycle tests | 已有用户数据库的有界迁移观察 |
| 快照范围、错误口令、新目录恢复 | context snapshot regressions、native finite command checks | 同包 App 真实导出和恢复 |
| CLI catalog、create-only、拒绝替换 | tools CLI regressions、打包脚本 | App 资源内实际 binary/info 与用户安装路径 |
| 文件签名输入和预期公钥验签 | tools 文件边界测试、signer detached tests | 已有专用 signer 的真实 submit/inspect；首次配置受用户暂停 |
| 幂等、资源占位、冻结和终态 | tools jobs/secrets regressions、native quit tests | App 长操作/退出交互 |
| 线程私有标记及后续轮次 | privacy unit tests、codex stdio fault replays | 用户显式启用的实验 runtime；本次不启动付费模型任务 |
| 管理员桥和 signer 初装/管理 | 用户明确暂停，未开放 | Apple 签名、系统批准、认证 IPC、gate 和专用空安装 |

普通 CI 不能代替原生系统授权、真实 signer 或 Finder 验收。验证结果、候选版本和未执行项目在实际 PR 记录；不新增通用流程台账。

### 11.1 验证映射表

| 上游条款 | 本设计条款 | 独立义务与适用条件 | 验证方法与当前入口 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-CC-015](command-center.prd.md#REQ-CC-015) | [DES-CC-TOOLS-001](#DES-CC-TOOLS-001) | 独立库、CAS、元数据、快照及 CLI | [context tests](../../../tools/oasis7-command-center/core/tests/context.rs) 数据隔离和条件保存；context snapshot/CLI 回归；真实 App 新建与重开 | 实现 PR source/build/机器和实际结果 | 未执行原生场景不得计通过 |
| [REQ-CC-015](command-center.prd.md#REQ-CC-015) | [DES-CC-TOOLS-002](#DES-CC-TOOLS-002) | 同一请求 prepare/submit/inspect 和公钥验签 | [signer 授权反例](../../../crates/oasis7_local_signer/tests/qa_m0_negative.rs) 精确 purpose/context/hash；detached 单元测试验证 envelope；tools 输入反例和已有专用 signer 回读 | 实现 PR request/job 和真实结果 | 无测试安装时不证明 worker 实际提交；管理员用户暂停 |
| [REQ-CC-015](command-center.prd.md#REQ-CC-015) | [DES-CC-TOOLS-003](#DES-CC-TOOLS-003) | 幂等、冻结、未知效果、口令句柄 | [runtime 故障回放](../../../tools/oasis7-command-center/core/tests/codex_runtime.rs) 未知/退出；tools jobs 单元测试覆盖事务/冻结/查询可用性；secrets 和 native quit tests | 实现 PR 故障与退出回放 | 强退不保证后台继续 |
| [REQ-CC-015](command-center.prd.md#REQ-CC-015) | [DES-CC-TOOLS-004](#DES-CC-TOOLS-004) | 私有标记先于发送及后续轮次/恢复 | [runtime fault tests](../../../tools/oasis7-command-center/core/tests/codex_runtime.rs) 登记失败不发 frame、后续普通轮次和重开过滤；PrivateContextInput UI tests | 实现 PR 原线程/来源和回放结果 | 本次不启动模型任务，不证明模型输出质量 |
| Trace refs: O7-REQ-fd3b34c6-4fb4-4614-8d7b-744a86d29c3a；[REQ-CC-015](command-center.prd.md#REQ-CC-015) | [DES-CC-TOOLS-005](#DES-CC-TOOLS-005) | 明确导出范围、分离口令、签名不造授权、工具往返和私有隔离 | 扩展 [LocalTools UI tests](../../../tools/oasis7-command-center/ui/src/LocalTools.test.tsx)：切换条目不改为整库、导出不借恢复口令、取消清理、缺 grant/context 拒绝；真实同包 macOS App 从工作进入工具后返回，核对导出/恢复及原操作结果 | 实现 PR source/build、场景、实际结果 | UI fixture 不证明原生文件对话框、实际 signer 或恢复成功；签名仍需已有获准测试安装 |
| Trace refs: O7-REQ-76d54ed7-4028-460a-b22b-37a9088ad245；[REQ-CC-010](command-center.prd.md#REQ-CC-010) | [DES-CC-TOOLS-005](#DES-CC-TOOLS-005) | 就地反馈、独立错误、键盘返回及未知操作核对 | 扩展 [LocalTools UI tests](../../../tools/oasis7-command-center/ui/src/LocalTools.test.tsx)：字段失败、后台结果、迟到响应、未知冻结与其他工具可用；真实 App 键盘/读屏及长表单错误定位 | 实现 PR 原生窗口、操作与卡点 | 源码/DOM 检查不证明实际可访问体验或耗时 |

### 2026-10-18 日常闭环候选核对

本 issue 范围的任务入口/默认主题与诊断层级已在候选 `52dd4aed915ce5ca75e00ddff6ba5e1918ebdec6` 实现，实际来源、自动化和原生证明见[日常闭环验证](daily-work-verification.manual.md)。真实 IME 候选和 VoiceOver 仍未验证，不能由 UI fixture、AX 或源码代签。未据此扩大专题其他尚未开放的范围、外部控制权、Apple 签名或管理员安装。
