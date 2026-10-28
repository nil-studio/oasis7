# CodeQL 操作与排障

本手册面向 CI 与仓库运营者，承接 [CodeQL 设计](codeql-integration.design.md) 和 [设计及验收映射](codeql-integration.design.md)。实现文件存在、CLI 可运行或 fixture 通过，都不证明 hosted 基线、提取覆盖、平台 provenance 或 observe 已验收。代码缺省关闭重型扫描；实际变量、运行与验收状态由对应 PR/CI 和已使用的 Task 记录，本手册不声明当前运营模式。

系统 owner 为 `repository_health_engineer`，验收 owner 为 `qa_engineer`；Rust 提取问题交给 `runtime_engineer`，权限与凭据边界交给 `blockchain_ops_engineer`。真实状态与实例定位写入对应 PR/CI 和已使用的 Task（Issue/Project 按需），不在本手册维护扫描状态总表。

## 运行设置

`.github/workflows/codeql.yml` 读取以下 repository variables。改变它们属于运营动作，不能由候选 PR 自启用。

| 设置 | 值与行为 |
| --- | --- |
| `OASIS7_CODEQL_MODE` | 缺省 `off`：planner 报告 disabled，不启动重型分析。`baseline`：只允许已配置批次的默认分支 schedule/manual 扫描。`observe`：另允许配置为 observe 的 default 批次参与非 draft、非 closed 的 PR 扫描。非法值使计划步骤失败。 |
| `OASIS7_CODEQL_MAX_SLOTS` | 缺省 `2`；只接受 `1` 或 `2`。重型 jobs 共用固定 native concurrency 槽位；matrix 的 max-parallel 不是仓库级上限的证明。 |
| manual `profile` | `default` 或 `extended`。PR 固定 default；六小时 schedule 使用 default，每日 schedule 使用 extended。类别为 `oasis7/<unit>/<profile>`，不能重标旧结果。 |

四个稳定单元是 `actions-repo`、`python-repo`、`javascript-repo`、`rust-repo`。选中单元保留全语言上下文；路由清单不等于 extractor 已覆盖全部文件。维护任务使用完整四语言候选范围并受批次配置约束，manual 必须选可信默认分支。PR 新 head 仅取消本 PR 旧 workflow；不同 PR 与维护 jobs 在共享槽位排队，job concurrency 使用 `queue: max` 且不互相取消。队列容量、取消结果和 runner 竞争仍须 hosted 验证。

## 分批控制

`OASIS7_CODEQL_BATCH_MODES` 缺省 `{}`，八个 unit/profile 批次全部 disabled。它是 repository variable JSON object，例如 `{"actions-repo/default":"baseline"}` 仅允许该目标批次参加默认分支维护（含获授权的首次验收试运行，不代表已验收）；`observe` 仅可用于 default profile，并且全局 mode 也必须为 observe 才允许其合格 PR。删除键停该批，default值从observe降baseline停其PR，global off停全部后续计划。非法/重复键、未知组合、非字符串值或extended observe使规划失败。

manual/schedule仍提出完整四语言候选范围，但实际矩阵受当前profile批次配置约束；未启用范围逐项标明原因，不能当扫描成功。改变全局mode不会自动绕过批次限制。首次交付控制能力不设置外部变量、不发起扫描或上传。首次验收时，经覆盖设置和扫描的授权仅将目标键设为baseline并读回，在可信默认分支试运行；此时pending acceptance，配置不是验收通过，PR仍禁用，失败删除键回退。真实hosted验收完成后才声明已验收并持续运营；native位置切片、fixture、局部helper或其他批次PASS不代签。分批控制不改变八批整体healthy、category、权限、并发或required政策。

## 可信计划与权限

PR workflow 从事件 base commit 取 `scripts/security/codeql-plan.py` 与 `codeql-policy.json`，在临时目录用 isolated Python 执行，避免导入候选 planner。可信 base 缺少任一文件时，workflow 保守提出四语言 bootstrap 候选，再受可信批次配置限制；不能改用候选策略缩小范围。计划同时记录 source head 与实际 analysis checkout，二者不能混用；workflow 验证实际 checkout 等于事件分析 SHA，并按该 checkout 分析和上传。

planner 使用旧、新 Git 路径与 Cargo manifest 清单；未登记输入使规划失败（任意 build.rs/动态 include 的可能消费不构成登记），读不到 Git 身份或更改 symbolic link 会显式失败。未被消费的文档可得到 `not_applicable`；空计划不上传空 SARIF。排除项固定为 `third_party/`、`target/`、`node_modules/`、`dist/`、`.pm/`；维护中的本地 vendor 补丁不能被这些排除项误遮蔽。manifest 清单、动态 include/build 输入与 Rust nightly/features/targets 的实际覆盖仍需 extraction diagnostics 核对。

顶层权限为 `contents: read`；分析 job 增加 `security-events: write` 和 `actions: read`。checkout 不保留凭据，actions 使用完整 SHA，CodeQL dependency caching 关闭。不要引入业务或发布 secrets、特权候选执行或 `pull_request_target`。Rust `build-mode: none` 仍可能执行 build scripts/macros。fork/Dependabot 的有效权限与上传行为必须实测；权限不足是失败或未知，不能宣称上传成功。

## 本地诊断命令

先运行 `python3 scripts/security/codeql-plan.py --help` 与 `python3 scripts/security/codeql-health.py --help`。生产计划应继续由 workflow 的可信 base bootstrap 生成；以下手工命令只用于诊断，调用前须独立验证 planner、policy 与 OID 来自可信版本：

```sh
python3 -I "$TRUSTED_PLANNER" --repo-root "$REPO_ROOT" \
  --base "$BASE_OID" --head "$SOURCE_OID" --checkout "$CHECKOUT_OID" \
  --policy "$TRUSTED_POLICY" --output "$PLAN_OUTPUT" \
  --mode observe --event pull_request --profile default --max-slots 2 \
  --batch-modes '{"python-repo/default":"observe"}'
```

`--pr-number` 仅参与槽位分配；不能建立 task 或 PR authority。维护诊断另指定 `--event workflow_dispatch --default-branch "$DEFAULT_BRANCH" --full`，并使用默认分支相同 source/checkout。CLI 不替代 workflow 对 draft、closed、事件 ref 的验证。

健康报告的 live 模式是只读 API 诊断，需要 `gh` 登录身份有权读取 code-scanning analyses、alerts、Actions runs/jobs：

```sh
python3 scripts/security/codeql-health.py --repo "$REPOSITORY" \
  --ref "$EXACT_REF" --sha "$EXACT_OID"
python3 scripts/security/codeql-health.py --fixture "$FIXTURE_PATH" \
  --now "$TIMEZONE_AWARE_ISO_TIMESTAMP"
```

`--sha` 须为完整小写 Git OID，`--ref` 用完整 refs 名称。fixture 只证明报告逻辑，不能代替 hosted proof。健康 CLI 返回 JSON；退出码零只表示报告生成，须检查 `status`、`errors` 和每个单元字段。live jobs 当前只关联 branch ref 的精确 SHA；PR merge ref 的 execution 可能保持 unknown。

## 状态解释与排障

| 观察 | 解释与下一步 |
| --- | --- |
| `disabled` / `not_applicable` | 分别是模式禁用与无选中单元；均不表示安全扫描通过。确认模式、事件、路径与 fallback reasons。 |
| execution failed / timed_out | 查看 extraction/query 步骤日志、实际 checkout、语言版本与覆盖诊断；保留失败，不使用 continue-on-error 假绿。 |
| cancelled / queued / waiting | 核对同 PR supersession、共享槽位、平台队列与 runner 资源；取消不是完成，也不证明零告警。 |
| execution success、upload failed/unknown | 查询已执行但结果未被确认接收；查 SARIF upload 日志、权限、ref/SHA/category 和处理错误。不得上传空报告补绿。 |
| upload accepted | 须同时有最新 job 的 `run_id` / `run_attempt`、成功上传步骤与唯一匹配的 SARIF association：该 job 的 `upload_sarif_id` 对应 analysis 的 `sarif_id`，且 analysis 接收无 error。精确 ref/SHA/category 或相近时间本身不足以关联本次上传；另读 Security findings 与 coverage。 |
| finding `open` | 精确身份上观察到 open findings，交工程 owner 判断影响与修复；扫描接入不捆绑自动修复任务。 |
| finding `unknown` | API 不完整、未找到 analysis 或没有完整 alert instances；不能转成零 findings。live reader 的 most_recent_instance 不构成完整历史证明。 |
| `healthy` | 八个 unit/profile 各自最新 run/attempt 均有明确关联的 analysis，在 24 小时内且 execution success、upload accepted；旧 attempt 的成功不能补足新 attempt。不同 profile 可来自不同运行。不要求 findings 为零，也不证明 manifest coverage 或完整接入验收。 |
| `incomplete_or_stale` / `unknown` | 分别是完成/新鲜度不足与身份/API 错误。检查每单元 freshness、missing_manifests、coverage_status、errors；缺失 coverage 为 unknown。 |

API 每个 endpoint 的读取有分页预算，精确身份的相关 runs 也有预算；超限或权限/响应错误保留 unknown，不无限枚举或丢弃错误。finding trend 当前为 unknown，不能用相邻报告的零值作趋势结论。

live reader 使用精确 run attempt 的 jobs API，并通过独立验证的 evidence artifact 读取官方上传输出。该正向关联当前只支持可信默认分支的 schedule/manual 基线。PR、fork、Dependabot 与 native platform provider 的正向关联仍不受支持，保留 unknown；artifact 不是合入凭据，也不赋予 native check 豁免。不要由时间、同 SHA 或 job success 推造 SARIF association。查看 `run_id` / `run_attempt` 判断当前尝试，查看 `analysis_id` / `coverage_age_hours` / `fresh` 判断已关联的结果；缺乏 association 时这些结果不能借用历史记录。

分析 job 从官方 pinned upload-sarif action 的 `steps.upload.outputs.sarif-id` 生成闭合 schema `oasis7-codeql-upload-evidence/v1`，记录 repository、workflow path/ref/SHA、run/attempt、job key/name、checkout SHA/ref、unit/profile/category 与真实 execution/upload outcomes。上传未成功时 ID 为空。每个 job/attempt 使用独立 `oasis7-codeql-upload-<run_id>-<run_attempt>-<unit>-<profile>` artifact，唯一文件为 `evidence.json`，由完整 SHA 固定的 upload-artifact action 上传且不覆盖。取消、超时或 runner 丢失可能没有 artifact，不能补写或借用旧输出。

consumer 不信任 JSON 自述或 artifact 名称：先独立验证 live repository/default branch、可信 workflow、精确 run/latest attempt、唯一 job/check/provider/suite，并确认执行时 workflow 字节与当前默认分支 workflow 相同。无关 main 前进且 workflow 字节不变不使结果失效；workflow 改动、候选执行或读取失败保持 unknown。随后完整、有界读取该 run 的 artifact 清单，要求唯一、未过期、元数据匹配，并在下载后复验 run/attempt 与 artifact 身份。

archive 下载上限 1 MiB、超时 30 秒，并校验平台 `sha256` digest；ZIP 不解压到文件系统，只接受一个常规 `evidence.json`，JSON 上限 64 KiB，拒绝路径穿越、额外/重复项、符号链接、加密、错误编码、重复键及未知字段。官方输出 ID、全部 payload 身份与真实 extraction/upload 成功必须一致，才能与唯一精确 SHA/ref/category 的 CodeQL `analysis.sarif_id` 且无 analysis error 关联。分页溢出、大小超限、digest 不匹配、缺失/重复/过期 artifact 或失败的新 attempt 都保持失败或 unknown；该证据不证明零 findings 或 manifest coverage。

`previous_analysis_id` 与 `previous_analysis_age_hours` 只是精确 ref/SHA/category 下最近可见 analysis 的历史参考，不保证属于当前 attempt，也不能证明 latest upload 成功或覆盖新鲜。新 attempt 排队、取消、超时或上传失败时，保留该 attempt 的真实状态；旧 analysis 即使仍新鲜也不使报告 healthy。缺少有效上传关联证据时须保留 unknown 并交验收 owner 核查；artifact fixture 通过或 health 报告生成不能据此宣布 hosted 验收通过。

CodeQL 扫描保留实际发现和运行来源；required/advisory 由 GitHub CI 声明及生效保护决定。必需安全检查未成功时不得合入，不通过本地解释改写远端结果。

## Tauri 上下文生成覆盖例外

Rust 的 CodeQL、容量及 CFG 诊断入口精确忽略 `tools/oasis7-command-center/src-tauri/src/generated_context.rs`。该文件只包装 `tauri::generate_context!()`，普通构建继续使用真实生成上下文；入口与命令实现仍参与分析。新增业务逻辑不得放入该文件。结果注明这个生成代码覆盖例外，不把排除后变快记为上游算法修复或无例外完整覆盖。旧诊断使用固定业务 SHA，其中旧宏位置不因新路径配置而消失；后续验证须明确采用含隔离文件的新源码身份。

验证使用 CFG 诊断固定的新源码与原完整官方 bundle，在同一新数据库中查询三个精确文件的 AST 和宏展开节点计数：隔离文件须为零，`main.rs` 与 `commands.rs` 必须存在正向 AST 见证。保留原始 BQRS、解码行、源码摘要和结果；未到、失败或结果不完整一律不记通过。见证只证明数据库中的 AST 覆盖边界，不证明 Rust Analyzer 没有执行工作区语义准备，也不单独证明性能改善。容量入口默认仍使用原冻结源码，新 pin 仅用于这个明确诊断。

## 大型 JSON 测试夹具覆盖例外

经批准的例外是 `crates/oasis7_runtime/src/tests/agent_cognition_live_persistence_projection_fixture.rs` 中的完整测试数据构造函数 `cognition_projection`。持久化测试及其断言仍参与分析，普通构建使用原样实现；不扩大到其他 `json!` 或目录。结果须注明此测试夹具覆盖缺口。CFG 诊断固定包含拆分的源码，和 Tauri 例外一起验证五个文件：两个隔离文件 AST 为零，Tauri 入口、命令及原持久化测试文件均须有正向 AST。完整图摘要用于检查旧大型 scope 是否消失；它不能单独证明整个模型阶段的性能改善。容量入口保持原冻结源码，不能用其旧结果代签新排除。

## Rust 公共模型性能诊断

本入口用于已授权的候选源码测量，遵循[有界公共模型性能诊断](codeql-integration.design.md#72-rust)，不执行安全查询集、不上传 SARIF、不修改批次设置或 required policy。源码与诊断程序分别固定 OID；每轮开始前记录探针要区分的假设、预期观察和停止条件，保留旧失败，不自动重试。

`scripts/security/codeql-rust-model-diagnostic.py --help` 列出实际参数。`--probe` 仅选择 `node-only`、`endpoints`、`peer-df`、`driver-non-target`、`driver-target`、`calls` 或 `full`。先用最小前置探针确定公共 Node、调用解析和局部流的成本边界；已有宏 AST 成功证据不要求每轮重做。`endpoints` 的存在性结果辅助解释 wrong-branch/wrong-state 等负例，不能取代原负例。`full` 仍须完整标签 union、全部正负期望和精确 Files 身份通过。

每个分区单独记录开始/结束、退出码、BQRS/解码表、指标判定和缓存状态，成功后立即解码；后续超时保留此前结果并标明完整批次未通过。一次模型阶段的所有求值、预热及解码共同受原 180 秒截止控制，全轮最多 1080 秒；多个独立轮次的局部结果不能拼成原完整批次的预算内成功。日志汇总优先使用官方 `generate log-summary`，共享阶段去重后的 wall 时间也不是 CPU 时间。

资源样本必须说明进程身份、采样间隔、观测峰值与真实峰值的区别，以及退出时的缺口；采样不到不能记作零。纯结构化 evaluator 日志、progress、tuple 信息、官方摘要、遥测和结果共受 64 MiB 限制。证据空间不足或身份/清理异常时终止后续子进程，保存具体失败；不删除旧证据、不循环探测空间、不通过丢弃失败原文伪装满足限制。

官方版本对照固定同一业务源码、Cargo 图、实际 Rust 工具链和提取配置，各用完整官方 bundle 与独立新数据库、缓存。先比较相同探针；新版能完成后，再做原完整公共见证验证。提取缺失而变快不算改善。若 Node-only 已呈现同一昂贵前置、新版受控对照仍未完成且成本仍集中在官方共享阶段，结束没有新机制的本地改写，准备上游材料：源码/manifest/lock/vendor 身份、bundle/pack 摘要、完整 QL 与期望、原始日志和官方摘要、资源样本、终态及可重建命令。上游公开提交另需授权。

性能问题解决与 Rust 启用是两项验收：本入口即使完整 candidate 通过，也不代签 legacy、Linux image、全仓覆盖及运营验收。具体运行身份、结果和未知项写对应 PR/Issue，不在本手册追加运行历史。

诊断由 Rust workflow 的 `workflow_dispatch` / `run_mode=codeql_rust_model_diagnostic` 显式触发，`model_probe` 选择上述探针，`model_bundle_version` 只接受固定 allowlist 中的 `2.27.2` 或 `2.27.3`，默认旧版；本地入口对应 `--bundle-version`。完整 bundle URL/摘要及解析包闭包随选择绑定，不能混用新版 CLI 和旧版库，也不接受任意下载 URL。派发前冻结诊断 source SHA 与业务 checkout SHA，读回唯一 run 后沿该身份观察，不重复派发。

`model_probe=trait-scope-plan` 仅在完整官方 2.27.3 上做隔离库编译计划实验：将 trait 可见性图的 ItemNode 域限制为目标点所属作用域及其完整反向外层闭包，保留原作用域跳转、图边和相关 trait/element 配对。有限图差分和路径等价证明不代替实际 Rust 语义验收。入口先核固定官方文件摘要，仅在私有副本中替换一处；两版分别编译公共 localFlow 与 getStaticTarget 查询，使用独立冷编译缓存、同一 360 秒共享编译预算及原 1080 秒/20 分钟总门。没有数据库、提取或求值，不读取业务源码，不上传 SARIF，不安装无需使用的 Rust 工具链。完整 RA 必须独立复核作用域约束是否进入图边构造，以及是否只是把工作移到 relevantItemNode/getADescendant；无有效减算证据时淘汰或记 INCONCLUSIVE，runtime_go 始终为 false，不能因编译成功自动启动后续求值。官方包原件及其他诊断模式保持不变。

## 生产资源容量评估

`.github/workflows/codeql-rust-capacity.yml` 是独立的手动入口，`execute` 默认为 false；只有明确授权并确认 true 才执行。它固定 `rust-repo/default`，不接受扩展档或旧单探针替代完整工作负载。派发前冻结评估源码 main SHA、诊断实现 SHA、官方完整 bundle 和公共见证锚点，读回唯一 run 后沿同一身份观察；一次完成或失败即停止，不自动重跑。

任务复用生产固定 CodeQL Action 的初始化、资源自动选择及默认安全查询，保留全仓 Rust 上下文、独立 manifest、vendor、features、targets 与模板。记录实际线程/RAM、runner 和 cgroup、项目及分析工具链、归档标准库来源、默认查询清单与结果。分析设置 `upload: never` 并关闭数据库上传，不设置运营变量，不调用平台 SARIF 上传；本地 SARIF/原始查询结果只作为容量证据。

准备阶段记录 manifest 对应 Cargo.lock 的初始存在性、每次既有 metadata 后及分析前的身份与摘要；新增锁不白名单放行，首次观察阶段不等于写入者证明，解析影响未知时继续拒绝。分析后再次比较初始锁文件，ignored 锁新增或变化、观察缺失同样阻断验收。大日志可按设计上限流式 gzip 保存完整原字节，原始与压缩摘要分开核验；达到任何上限仍为 PARTIAL，不能仅因末尾存在成功 footer 改记完整。总 artifact 256 MiB、日志落盘 128 MiB、原 job 截止及无扫描重试/无平台上传边界不变。

分析前要求各独立 `[workspace]` 已有锁；缺锁直接停止，不通过预生成未跟踪锁改变旧业务基线。固定官方提取器会复制锁，故容量入口还在实际 Cargo 进程边界强制解析命令使用 `--locked`，并核验有界开始/终态记录，拒绝失败或缺失调用；不能仅靠源锁未变化或 `cargo_extra_args` 判定解析已冻结。私有 Cargo 代理位于业务树外，真实子命令恢复原 Cargo home 并尊重提取器选择的工具链；不改变官方 bundle、原缓存、生产扫描设置或现有截止。代理拒绝或 rust-analyzer fallback 不能改记成功。

入口先把固定工具提交的最小 Python 闭包及摘要保存到 `RUNNER_TEMP`，再在同一 `GITHUB_WORKSPACE` 根 checkout 固定业务提交。`collection-layout.json` 必须证明当前 cwd、workspace、Git 根及 source 是同一业务树，工具在采集树外且摘要未变；失败时不启动 init。`source-root`、`checkout_path`、模板路径全部指向该业务根。预检只证明布局；真实 `trace-command` / `index-files` 起点仍从数据库原始日志核对。

容量 helper 为 `scripts/security/codeql-rust-capacity.py`。 当前固定业务源码为已合入 Tauri 生成上下文排除的 `c036927d33b917a036a731f071209e16f6deb033`；checkout、布局检查和 prepare/capture/finish 显式绑定同一版本，仍按 45 分钟完整 default 与公共见证合同验收。旧业务版本仅可显式选取，旧结果不能代签本配置。其覆盖校验和公共见证使用同一真实数据库，包括 candidate、legacy、common 协议数据流及 Linux image 的完整规定正负结果；缺失、重复、额外标签或来源不符不算通过。默认查询执行成功与语义/覆盖通过分别记录，未到达的阶段保持 NOT_REACHED，不用旧局部 PASS 补签。

分析后的独立 `capture` 步骤先保存已有数据库日志，再分别读取 HEAD、工作区状态和 Git 根，不能由身份或资源错误跳过。`database-log-capture.json` 列出原始长度、已保存字节及摘要、变化/截断/未到达原因；部分证据可解释失败，但不能支持完整验收。步骤 outcome、最初失败与后续采证错误分别记录；失败后不继续 CodeQL 解码或公共见证。官方 Action 的进程清理保持 `UNKNOWN_RUNNER_OWNED`，需最终 runner 日志回读，不能套用 helper 自有进程组的清理结果。

45 分钟从实际 job 启动计时，涵盖 Action 准备、下载、提取、查询、解码和结果保存，内部为收尾及 artifact 上传预留时间。线程和内存不沿用旧诊断的 2/3072 参数，数据库也不沿用旧诊断的单文件或总数据上限；额外的证据收集上限及观测缺口必须单列，不把采集失败当生产计算失败。实际余量不足 270 秒时记 NEAR_LIMIT，不能直接进入 PR 扫描运营。

超限日志使用同一字节上限内的首尾片段；`source_ranges` / `omitted_ranges` 解释源字节位置，不能将拼接输出当作完整原始日志。进度摘要仅计保留片段内完整行，不代表完整查询结果。manifest 列表缩略、未闭合或采证部分完成时，`missing_found_manifests` 为 null，枚举完整性为 UNKNOWN；已观察到的 manifest 仍保留。Git raw status 保留，tracked 状态与未跟踪锁文件证据分列；未跟踪 Cargo.lock 不自动豁免 clean gate，其来源及依赖解析影响没有证据时保持 UNKNOWN。

预算记录同时保留固定 job 截止、准备耗时、实际分析分钟数及后续 600 秒预留。分析 step 触及其内部限时只能说明本次分配未完成；没有完整、隔离且可核验的执行结果时，生产容量保持未知。修复入口与离线故障回归不构成新一轮容量运行授权。

上游成本研究使用同一 workflow 的显式 `mode=official-default-research`，仍须 `execute=true` 且单独获准运行。固定原业务 pin 与官方 bundle/资源，整个 job 360 分钟（含准备、提取、查询、官方结果解码、证据上传及 post）；不生成或执行公共自定义见证。分析分配为剩余时间减 600 秒收尾预留后的整分钟，无 32/35 分钟早停；内部限时仍不等于整个六小时已耗尽。`RESEARCH_*` 与 Action outcome 分列，dirty/不完整证据/结果失败保持阻断。一次成功或失败后停止，不重试、不分享数据库；生产 45 分钟容量、180 秒诊断与 Rust HOLD 不因研究完成改判。默认 `mode=capacity` 保持原入口合同。

按以下边界作决定：完整默认查询和规定公共见证均通过且预算有余量，只证明该冻结配置下计算与覆盖可行，下一步才是 hosted baseline；查询完成但覆盖缺口保留相应 HOLD；完整但接近上限先评估成本和频率；超时、OOM 或计算结果不完整说明该配置未通过容量评估，停止追加运行并保留生产规模证据。无上传不证明平台关联、provider、运营队列或启用完成。原 180 秒优化问题继续独立跟踪，不因容量结果改判成功。

## Hosted 验收与回退

授权运营者开启 baseline 前，读回默认/advanced setup 的有效状态并处理重复扫描，核对 required checks、rulesets/code-scanning protections、最小权限、action pins、fork/Dependabot 和缓存隔离。随后记录四语言 default/extended 的实际 execution、upload、native Security findings、manifest/extraction diagnostics、控制样本、覆盖缺口及 owner；验证共享槽位峰值、跨 PR 排队、同 PR 取消与实际来源。将样本耗时、runner 竞争和 required CI 延迟作为实测结果，不能用 fixture 或静态 YAML 代替。

observe 只能在 hosted 验收完成且明确授权后设置，并读回实际变量与非 draft PR 行为。当前文档不证明这些步骤已发生。平台 provenance 无法验证或覆盖缺口未解决时，保留能力不足事实并交验收 owner，不能宣布完整接入。

资源压力时可经授权先将 slots 降为 `1`；需停 PR 扫描时退至 `baseline`；需停全部重型执行时退至 `off`。变量设置影响后续计划，已有运行须另行核对或按授权取消。读回设置并确认新计划行为，保留既有 alerts、analysis 与失败证据；不要删除告警、改 category 或改 required policy 来制造回退成功。

该变更面向工程运营，没有玩家功能或世界规则变化。发布说明可描述“新增默认关闭的观察性扫描实现”；在取得实际验收前，不能声称仓库安全通过、漏洞为零、四语言覆盖完成或 observe 已上线。任何对外公告与玩家承诺需另行授权并由 LiveOps 与产品 owner 核对。


### CFG 关系规模诊断

`CodeQL Rust CFG Diagnostic` 是默认 `execute=false` 的独立手动入口，使用容量入口的固定业务 pin、官方 2.27.3 完整 bundle、原生依赖和源码隔离检查。它只 finalize 数据库并执行 CFG 定向测量，不调用 analyze、不运行默认安全查询、不启用批次、不上传 SARIF/数据库。一次完成或失败即停止，不自动重试。整个 job 45 分钟，包含准备、提取、查询、图分析和收尾；结果不替代生产容量或完整 Rust 覆盖。

先审阅私有 QLL 可见性补丁和查询实际解析路径，再执行 scope 汇总及有界 top scope 图检查。结果必须分别记录全图汇总、选中 scope 覆盖、跨 scope/未知归属、分量与可达对计算是否完整；只输出达到限制前的局部数据时保持 INCOMPLETE。保存工具/业务/官方源码身份、精确 QL、解码结果、原始日志、退出码和固定时钟；不保存或导出完整巨型闭包。新运行只能说明新冻结数据库，不能复原旧 run 的图。
