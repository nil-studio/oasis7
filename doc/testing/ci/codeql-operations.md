# CodeQL 操作与排障

本手册面向 CI 与仓库运营者，承接 [CodeQL 设计](codeql-integration.design.md) 和 [设计及验收映射](codeql-integration.design.md)。实现文件存在、CLI 可运行或 fixture 通过，都不证明 hosted 基线、提取覆盖、平台 provenance 或 observe 已验收。代码缺省关闭重型扫描；实际变量、运行与验收状态由对应 PR/CI 和已使用的 Task 记录，本手册不声明当前运营模式。

系统 owner 为 `repository_health_engineer`，验收 owner 为 `qa_engineer`；Rust 提取问题交给 `runtime_engineer`，权限与凭据边界交给 `blockchain_ops_engineer`。真实状态与实例定位写入对应 PR/CI 和已使用的 Task（Issue/Project 按需），不在本手册维护扫描状态总表。

## 运行设置

`.github/workflows/codeql.yml` 读取以下 repository variables。改变它们属于运营动作，不能由候选 PR 自启用。

| 设置 | 值与行为 |
| --- | --- |
| `OASIS7_CODEQL_MODE` | 缺省 `off`：planner 报告 disabled，不启动重型分析。`baseline`：只允许已配置批次的默认分支 schedule/manual 扫描。`observe`：另允许配置为 observe 的 default 批次参与非 draft、非 closed 的 PR 扫描。非法值使计划步骤失败。 |
| `OASIS7_CODEQL_MAX_SLOTS` | 缺省 `2`；只接受 `1` 或 `2`。重型 jobs 共用固定 native concurrency 槽位；matrix 的 max-parallel 不是仓库级上限的证明。 |
| manual `profile` | `default` 或 `extended`。PR 固定 default；六小时 schedule 为非 Rust default，UTC 03:29 的每日时段仅选 Rust default，UTC 18:43 为既有 extended。类别为 `oasis7/<unit>/<profile>`，不能重标旧结果。 |

四个稳定单元是 `actions-repo`、`python-repo`、`javascript-repo`、`rust-repo`。选中单元保留全语言上下文；路由清单不等于 extractor 已覆盖全部文件。维护任务提出完整四语言候选范围，再与时段语言范围及批次配置取交集，manual 必须选可信默认分支。PR 新 head 仅取消本 PR 旧 workflow；不同 PR 与维护 jobs 在共享槽位排队，job concurrency 使用 `queue: max` 且不互相取消。队列容量、取消结果和 runner 竞争仍须 hosted 验证。

## 分批控制

`OASIS7_CODEQL_BATCH_MODES` 缺省 `{}`，八个 unit/profile 批次全部 disabled。它是 repository variable JSON object，例如 `{"actions-repo/default":"baseline"}` 仅允许该目标批次参加默认分支维护（含获授权的首次验收试运行，不代表已验收）；`observe` 仅可用于 default profile，并且全局 mode 也必须为 observe 才允许其合格 PR。删除键可停用该批次；default 值从 observe 降为 baseline 可停止该批次的 PR 扫描；global off 停止全部后续计划。非法/重复键、未知组合、非字符串值或 extended observe 使规划失败。

manual/schedule 仍提出完整四语言候选范围，但实际矩阵受当前 profile 批次配置及维护时段范围约束；未启用范围逐项标明原因，不能当扫描成功。改变全局 mode 不会自动绕过批次限制。首次交付控制能力不设置外部变量、不发起扫描或上传。

首次验收时，经覆盖设置和扫描的授权，仅将目标键设为 baseline 并读回，在可信默认分支试运行；此时为 pending acceptance，配置不代表验收通过，PR 仍禁用，失败时删除键回退。真实 hosted 验收完成后才声明已验收并持续运营；native 位置切片、fixture、局部 helper 或其他批次 PASS 不代签。

分批控制不改变八批整体 healthy、category、权限、并发或 required 政策。

Rust default 默认分支维护使用 90 分钟总预算。init 后读取当前 run/attempt 的唯一 Rust job，以 GitHub Date 和 started_at 计算剩余时间，再分配最多 75 分钟分析，预留 600 秒用于上传和采证；余量不足一分钟直接失败。只增加 `rust-repo/default=baseline` 即可允许已授权的每日或手动验收，不能将全局 observe 改回 baseline 而停掉其他语言的 PR 扫描。失败回退仅删除本次 Rust 键。日频共享 slot 1，排队影响仍需观察；24 小时健康判据保持原样，调度或执行波动可能造成短暂 stale。

该场景使用固定 CodeQL 2.27.4 Rust bundle 和隔离的 qualifier-lines-v3 库修补，保持官方原始 37 项默认查询。官方 Action 负责初始化与提取，生产 helper 校验实际库闭包后执行源码查询并生成 SARIF，仍由原上传步骤关联 Security。库准备、提取、查询与结果校验共用一次分析截止；PR 和 extended 保持各自原路径。技术合同见[生产库接入与首次验收](codeql-integration.design.md#726-rust-default-生产库接入与首次验收)。

首次验收同时检查独立的 `oasis7-codeql-rust-production-*` artifact：工具和修补身份、当前 checkout 的 manifest/锁观察、实际 Cargo 解析、完整查询结果及覆盖诊断。`production-run.json` 的执行成功不代表覆盖通过；coverage 的 REVIEW/UNKNOWN、WASM 或源码归属缺口继续记录。该 artifact 与只包含 `evidence.json` 的上传关联 artifact 分开，后者仍用于唯一 SARIF/platform analysis 关联。只有覆盖及运营验收闭合后才将该批次声明为已验收日频基线。

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

### Rust default 平台只读补采

本地身份无法读取 code-scanning API 时，可从可信 `main` 手动运行 `CodeQL Platform Observation`，填写已完成生产扫描的 `producer_run_id`、最新 `producer_run_attempt` 和完整 `producer_sha`。该入口固定读取本仓 main 的 Rust/default，checkout 采证 workflow 自身的 main SHA；只有 contents/actions/checks/security-events read 权限，不启动扫描、不上传 SARIF。producer 仍在运行、attempt 已变化、身份或分页不完整时失败并保留 unknown，不退回旧 attempt。

采证 job 最多 7 分钟，其中 API 共用 240 秒。读取复用 `codeql_upload_association.py` 与 `codeql-health.py`，从可信 artifact 的官方 SARIF ID 精确查询 analyses，保留平台 `error` / `warning` / `results_count` 与 open alerts 观察；失败也尝试上传独立 `oasis7-codeql-platform-observation-<collector_run>-<collector_attempt>` artifact 中的 `platform-observation.json`。退出零仅表示目标上传有唯一、无错误的平台关联且所请求读取完成，仍须查看 warnings、findings 范围与覆盖证据。

报告的 `analysis_results_count` 属于已关联 analysis；`observed_open_findings` 是读取时 most_recent_instance 恰好匹配目标 SHA/ref/category 的现存 open alerts，不能称为该 attempt 的新增告警。最多读取 10 页、每页 100 条；超限或 API 失败丢弃不完整 findings 并报告 unknown。即使未观察到匹配告警，也不声明零告警，因为此入口不枚举完整 alert instances。完整启用验收仍需单独审查生产覆盖与后续持续运行。

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

Rust 的 default/extended 扫描先执行 `Prepare Rust native extraction prerequisites`，复用容量入口的 GTK 3、WebKitGTK 4.1、D-Bus 安装与 pkg-config 探针；准备共用最多 180 秒，计入对应 job 的总预算；默认分支 Rust default 为 90 分钟，其他 Rust 场景保持 45 分钟。失败时从独立 `oasis7-codeql-native-<run>-<attempt>-rust-repo-<profile>` artifact 查看安装 stdout/stderr、包和模块版本及 `native-terminal.json`；失败会阻止提取、查询和 SARIF 上传。native PASS 只表示该 runner 的依赖准备成功，覆盖和上传关联仍须分别验收。

分析 job 从官方 pinned upload-sarif action 的 `steps.upload.outputs.sarif-id` 生成闭合 schema `oasis7-codeql-upload-evidence/v1`，记录 repository、workflow path/ref/SHA、run/attempt、job key/name、checkout SHA/ref、unit/profile/category 与真实 execution/upload outcomes。上传未成功时 ID 为空。每个 job/attempt 使用独立 `oasis7-codeql-upload-<run_id>-<run_attempt>-<unit>-<profile>` artifact，唯一文件为 `evidence.json`，由完整 SHA 固定的 upload-artifact action 上传且不覆盖。取消、超时或 runner 丢失可能没有 artifact，不能补写或借用旧输出。

consumer 不信任 JSON 自述或 artifact 名称：先独立验证 live repository/default branch、可信 workflow、精确 run/latest attempt、唯一 job/check/provider/suite，并确认执行时 workflow 字节与当前默认分支 workflow 相同。无关 main 前进且 workflow 字节不变不使结果失效；workflow 改动、候选执行或读取失败保持 unknown。随后完整、有界读取该 run 的 artifact 清单，要求唯一、未过期、元数据匹配，并在下载后复验 run/attempt 与 artifact 身份。

archive 下载上限 1 MiB、超时 30 秒，并校验平台 `sha256` digest；ZIP 不解压到文件系统，只接受一个常规 `evidence.json`，JSON 上限 64 KiB，拒绝路径穿越、额外/重复项、符号链接、加密、错误编码、重复键及未知字段。官方输出 ID、全部 payload 身份与真实 extraction/upload 成功必须一致，才能与唯一精确 SHA/ref/category 的 CodeQL `analysis.sarif_id` 且无 analysis error 关联。分页溢出、大小超限、digest 不匹配、缺失/重复/过期 artifact 或失败的新 attempt 都保持失败或 unknown；该证据不证明零 findings 或 manifest coverage。

`previous_analysis_id` 与 `previous_analysis_age_hours` 只是精确 ref/SHA/category 下最近可见 analysis 的历史参考，不保证属于当前 attempt，也不能证明 latest upload 成功或覆盖新鲜。新 attempt 排队、取消、超时或上传失败时，保留该 attempt 的真实状态；旧 analysis 即使仍新鲜也不使报告 healthy。缺少有效上传关联证据时须保留 unknown 并交验收 owner 核查；artifact fixture 通过或 health 报告生成不能据此宣布 hosted 验收通过。

CodeQL 扫描保留实际发现和运行来源；required/advisory 由 GitHub CI 声明及生效保护决定。必需安全检查未成功时不得合入，不通过本地解释改写远端结果。

<a id="native-configuration-lifecycle"></a>
## 原生配置提示与生命周期

原生 `configurations not found` 表示原生比较尚未匹配到目标分支既有配置的相应分析。PR 固定 default，extended 只参加默认分支维护；按影响范围未选中 default 单元时也不会补传结果。分别核对事件、可信计划和实际分析，再解释覆盖提示与同时展示的告警；不能仅凭检查标题判断失败原因。

PR closed 事件会禁用扫描，plan/summary 成功且分析 job 跳过不代表合入后的 main 已重新扫描。Actions job、SARIF 接收、原生告警状态及原生覆盖提示分别记录，不能用其中一项成功替代其他项。

| 配置情况 | 核对依据与处置 |
| --- | --- |
| 活跃，但当前 PR 未运行 | 核对 profile、影响范围和事件适用性；保留原生配置，另核对该配置的维护扫描。main extended 与 PR default 的差异可以是预期行为。 |
| 暂停、HOLD 或待验收 | 核对已有暂停及恢复条件；保留原生配置、失败记录与覆盖缺口。局部研究成功或批次变量缺键均不表示退役。 |
| 已确认永久退役或完成迁移 | 核对精确配置身份、停止使用的决定、替代覆盖与历史告警影响，在已有授权覆盖删除时才清理该配置，并读回结果。 |
| 身份或用途不明 | 记录 unknown 和缺失信息，保留配置；本次未看到分析不能证明配置已退役。 |

原生配置身份结合 tool、category、workflow、适用 ref 及平台可见的 analysis key/environment 核对，不能只凭显示名或告警数量操作。删除批次变量键仅停止后续规划，不删除原生历史配置，也不代表该范围已验收。配置清理入口见 [GitHub Tool status](https://docs.github.com/en/code-security/how-tos/find-and-fix-code-vulnerabilities/manage-your-configuration/use-the-tools-status-page-for-code-scanning)。

`codeql-health.py` 继续按八个 unit/profile 诊断执行、上传与新鲜度，不读取批次变量来决定 HOLD、退役或缩小 healthy 的范围。活跃和 HOLD 配置完成梳理后的结果可以是全部保留；如需让 PR 提供完整对应分析，应另行评估真实扫描范围与耗时，不能重标 category、借用旧 SHA 或上传空 SARIF 消除提示。

误报按精确告警 ID 核对源码版本、实际输入和权限边界，在平台选择对应 dismissal 原因并记录证据，随后读回告警状态。历史探针注明实际受测版本，结合当前源码核对其适用性；判断条件改变时重审。处置某个 ID 不构成同类后续告警的自动豁免；操作参考 [GitHub 告警处置说明](https://docs.github.com/en/code-security/how-tos/manage-security-alerts/manage-code-scanning-alerts/triage-alerts-in-pull-requests)。

Actions 的预期候选代码执行与错误识别的可信 policy checkout 分别记录：前者在核对实际 runner、token/secrets、缓存写权限及仓内产物消费者后，可按精确 ID 使用 `won't fix`，后者有固定可信来源证据时使用 `false positive`。保留官方 checkout 查询，不以只读缓存、job 名或路径给同类告警统一豁免。已 dismissed 的同一 fingerprint 不保证在边界变化后自动重开；事件/来源选择、权限、runner、缓存模式、可复用调用关系或产物消费者变化时，评审须重新核对相应处置，必要时重开。记录应区分扫描自然关闭和人工 dismissal，不把减少的告警数称为修复漏洞数。

## Actions 缓存只读误报兼容

`scripts/security/codeql-actions` 在 default / extended 中只替换官方 `actions/cache-poisoning/poisonable-step`。仅当前 job 明确配置原始标量 `read` / `none`，或 job 未覆盖且 workflow 明确配置该值时消除该查询结果。表达式、重复 cache-mode、合并键、别名值及未知模式保持官方告警；显式可写 job 不继承 workflow 的只读结论。其他缓存规则、checkout 规则与源码范围保持原配置。

工作流同时直接表达真实信任边界：Command Center callee 自身声明只读缓存；四个受 main 来源与事件约束的 workspace/full writer 首次 checkout 使用 `github.sha`，候选 job 保留计划的 `tested_sha`。共享构建步骤按 job 复用，不能把只读候选上下文的结论应用到 writer；完整回归保留 npm、wasm-bindgen、Trunk 和 sentinel 的合法写入。该调整不为查询新增可信 job/output 名单。

兼容基线为 CodeQL CLI `2.27.4`、`actions-all 0.7.1`、`actions-queries 0.6.39`，上游源码 `1be292a1e54696ca543b4ad0f9b4537b9687caa7`。本地 bundle 的解析可能忽略 suite 版本约束，因此不以依赖声明或空 lock 作为实际版本证明。每个 Actions job 在 init 后校验实际 CLI、编译依赖、上游源码摘要及 init 最终查询集，再运行临时目录中的真实提取/查询反例；失败时后续 analyze/upload 跳过。两套查询总数为 18 / 24（各含一条提取诊断），目标 ID 均只出现一次。

本地复跑（`CODEQL_BIN` 指向相同官方 bundle 的可执行文件）：

```bash
python3 scripts/security/codeql-actions/model.test.py --codeql "$CODEQL_BIN"
```

测试只提取临时生成的 workflow 文本，不执行其中步骤，也不把测试 SARIF 上传。升级 Action / CLI / 查询包时，应先评审上游变化；已支持相同语义时移除此兼容层，否则更新版本和摘要并复跑两个 suite 的集合对等、实际 init 选择及模型正反例。不可通过跳过兼容检查或忽略整个规则恢复绿灯。因模型修正而消失的原生告警按误报修正记录，不表述为修复了相同数量的可利用漏洞。

## Tauri 上下文生成覆盖例外

Rust 的 CodeQL、容量及 CFG 诊断入口精确忽略 `tools/oasis7-command-center/src-tauri/src/generated_context.rs`。该文件只包装 `tauri::generate_context!()`，普通构建继续使用真实生成上下文；入口与命令实现仍参与分析。新增业务逻辑不得放入该文件。结果注明这个生成代码覆盖例外，不把排除后变快记为上游算法修复或无例外完整覆盖。旧诊断使用固定业务 SHA，其中旧宏位置不因新路径配置而消失；后续验证须明确采用含隔离文件的新源码身份。

验证使用 CFG 诊断固定的新源码与原完整官方 bundle，在同一新数据库中查询三个精确文件的 AST 和宏展开节点计数：隔离文件须为零，`main.rs` 与 `commands.rs` 必须存在正向 AST 见证。保留原始 BQRS、解码行、源码摘要和结果；未到、失败或结果不完整一律不记通过。见证只证明数据库中的 AST 覆盖边界，不证明 Rust Analyzer 没有执行工作区语义准备，也不单独证明性能改善。容量与诊断分别绑定下文记录的业务版本，旧版本结果不能代签新排除。

## 大型 JSON 测试夹具覆盖例外

经批准的例外是 `crates/oasis7_runtime/src/tests/agent_cognition_live_persistence_projection_fixture.rs` 中的完整测试数据构造函数 `cognition_projection`。持久化测试及其断言仍参与分析，普通构建使用原样实现；不扩大到其他 `json!` 或目录。结果须注明此测试夹具覆盖缺口。CFG 诊断固定包含拆分的源码，和 Tauri 例外一起验证五个文件：两个隔离文件 AST 为零，Tauri 入口、命令及原持久化测试文件均须有正向 AST。完整图摘要用于检查旧大型 scope 是否消失；它不能单独证明整个模型阶段的性能改善。容量入口的覆盖例外须在其独立固定版本上验收，不能用旧结果代签。

CFG 原始图诊断可显式使用 `--scope-identities`，仅为 `DomainEvent`、`Action`、`WorldState` 和 `LlmDecisionPayload` 所在四个文件补录 scope 的 AST 类别、标签、位置及宏展开标记。结果最多 10,000 行，每个字符串最多 4,096 字符；缺文件、重复 ID 或非法字段保持失败。标签用于区分同一源码范围的派生方法，不推断其性能成本；opaque scope ID 只能与同一次数据库的摘要、边相连，不能跨运行关联。生产类型在方法身份及兼容性边界核实前保持原实现和扫描范围。 拆分派生的 `Debug` / `PartialEq` 时，公开枚举、变体、字段与 Serde 属性保持原样；私有分组必须穷尽且互斥，并核对字段顺序、普通/pretty Debug 和相等性边界。旧版 oracle 可保存为纯数据期待值，不把原完整派生实现复制回扫描源码而重建同级闭包。序列化/反序列化的拆分另外核对 JSON、CBOR、默认/别名、未知/重复字段及支持的位置式序列输入，普通函数拆分或绿色 CI 不代签这些兼容性。

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

容量 helper 为 `scripts/security/codeql-rust-capacity.py`。当前 `mode=capacity` 固定业务源码为 2026-11-09 读回的 main 快照 `7c2f99a0ff2fbc3a3523dee7a1cc06fa5bf187ef`；checkout、布局检查和 prepare/capture/finish 显式绑定同一版本。该配置使用完整官方 2.27.4 bundle，与生产所固定 CodeQL Action 的默认版本一致；下载摘要、实际 CLI 版本、`rust-all` 0.2.25 及完整库依赖闭包分别核验，公共见证 pack 从该已验证闭包生成依赖版本。终态记录预期版本，实际身份以 CLI 输出及闭包证据为准。按 90 分钟完整 default 与公共见证合同评估，分析上限 75 分钟；默认分支 Rust default 的生产总预算也为 90 分钟。旧业务版本仅可显式选取，旧结果不能代签本配置。

共享 helper 按模式与业务 pin 绑定工具版本：仅当前 capacity 快照采用 2.27.4，其他历史快照及 `official-default-research` 继续使用 2.27.3。callee 定向诊断保留 `f9ab0ca14620f118c84130b3d558e48dc1e6314d`，用于复现该版本的空调用结果；e205 性能诊断虽复用 helper 的 capacity 模式，也保留原 2.27.3。它们不能据此声明当前源码验收通过。更新容量配置不修改公共见证文本或断言，也不证明已有调用解析缺口已修复。

其覆盖校验和公共见证使用同一真实数据库，包括 candidate、legacy、common 协议数据流及 Linux image 的完整规定正负结果；缺失、重复、额外标签或来源不符不算通过。默认查询执行成功与语义/覆盖通过分别记录，未到达的阶段保持 NOT_REACHED，不用旧局部 PASS 补签。

分析后的独立 `capture` 步骤先保存已有数据库日志，再分别读取 HEAD、工作区状态和 Git 根，不能由身份或资源错误跳过。`database-log-capture.json` 列出原始长度、已保存字节及摘要、变化/截断/未到达原因；部分证据可解释失败，但不能支持完整验收。步骤 outcome、最初失败与后续采证错误分别记录；失败后不继续 CodeQL 解码或公共见证。官方 Action 的进程清理保持 `UNKNOWN_RUNNER_OWNED`，需最终 runner 日志回读，不能套用 helper 自有进程组的清理结果。

当前容量配置的 90 分钟从实际 job 启动计时，涵盖 Action 准备、下载、提取、查询、解码和结果保存，分析分配为 `min(75, floor((剩余秒数−600)/60))` 分钟，为公共见证、结果校验、收尾及 artifact 上传保留 600 秒；准备较慢时实际分析分配仍会缩短。线程和内存不沿用旧诊断的 2/3072 参数，数据库也不沿用旧诊断的单文件或总数据上限；额外的证据收集上限及观测缺口必须单列，不把采集失败当生产计算失败。实际余量不足 270 秒时记 NEAR_LIMIT，不能直接进入 PR 扫描运营。

超限日志使用同一字节上限内的首尾片段；`source_ranges` / `omitted_ranges` 解释源字节位置，不能将拼接输出当作完整原始日志。进度摘要仅计保留片段内完整行，不代表完整查询结果。manifest 列表缩略、未闭合或采证部分完成时，`missing_found_manifests` 为 null，枚举完整性为 UNKNOWN；已观察到的 manifest 仍保留。Git raw status 保留，tracked 状态与未跟踪锁文件证据分列；未跟踪 Cargo.lock 不自动豁免 clean gate，其来源及依赖解析影响没有证据时保持 UNKNOWN。

预算记录同时保留固定 job 截止、准备耗时、实际分析分钟数及后续 600 秒预留。分析 step 触及其内部限时只能说明本次分配未完成；没有完整、隔离且可核验的执行结果时，生产容量保持未知。修复入口与离线故障回归不构成新一轮容量运行授权。

上游成本研究使用同一 workflow 的显式 `mode=official-default-research`，仍须 `execute=true` 且单独获准运行。research 模式固定原业务 pin `c036927d33b917a036a731f071209e16f6deb033`，独立于 capacity 模式的新 pin；官方 bundle/资源保持原配置，整个 job 360 分钟（含准备、提取、查询、官方结果解码、证据上传及 post）；不生成或执行公共自定义见证。分析分配为剩余时间减 600 秒收尾预留后的整分钟，不采用容量模式的内部分析上限；内部限时仍不等于整个六小时已耗尽。`RESEARCH_*` 与 Action outcome 分列，dirty/不完整证据/结果失败保持阻断。一次成功或失败后停止，不重试、不分享数据库；生产容量、180 秒诊断与 Rust HOLD 不因研究完成改判。当前 `mode=capacity` 使用 90 分钟评估预算与 75 分钟分析上限，完整公共见证和余量要求不变。历史 source 的 helper 仍保留原 60/45 分钟分配，CFG/profile 等入口的原实际截止不变。

按以下边界作决定：完整默认查询和规定公共见证均通过且预算有余量，只证明该冻结配置下计算与覆盖可行，下一步才是 hosted baseline；查询完成但覆盖缺口保留相应 HOLD；完整但接近上限先评估成本和频率；超时、OOM 或计算结果不完整说明该配置未通过容量评估，停止追加运行并保留生产规模证据。容量入口包含额外公共见证且没有正式上传；进入 hosted baseline 前，仍须核实正式入口在同一预算下的实际运行、上传及余量。无上传不证明平台关联、provider、运营队列或启用完成。原 180 秒优化问题继续独立跟踪，不因容量结果改判成功。

### 私有调用解析补丁对照

在已授权的单次 `mode=capacity` 运行中，可显式设置 `compare_qualifier_patch=true`。默认关闭，`official-default-research` 不接受该开关。官方容量结果和 artifact 先封存并上传；研究随后复用同一数据库，对固定公共 `CallsCandidate`/`CallsLegacy` 查询运行官方库与私有行为补丁。原官方失败不会被候选成功覆盖，研究结果不解除覆盖 HOLD。

研究最多 15 分钟，并必须在原 job 截止前 450 秒停止；余量不足 60 秒记 `NOT_REACHED`。研究使用单独编译缓存、BQRS 和 artifact，但同一数据库的 evaluator 缓存共享，不能将两侧耗时差解释为提速。核对实际闭包、补丁前后摘要、结果目标所属的真实 libp2p 路径，以及研究前后源码、官方库和官方 evidence 不变。最终报告分别列官方分析/见证、补丁对照、完整 job 与最终余量；不上传数据库或自动发表上游补丁。

### 独立真实调用对照研究

Trace refs: O7-REQ-351dbec8-3110-4ccd-aadc-49d0ba84bd89；承接[独立真实调用对照研究合同](codeql-integration.design.md#722-独立真实调用对照研究)的运行与结果解释边界。

需要单独验证 #5658 的真实调用补丁时，使用同一 workflow 的 `execute=true, mode=qualifier-research, compare_qualifier_patch=false`。该模式不等待容量七表完成：仍对完整固定 `7c2f99a…` 源码使用官方 2.27.4 提取，Action 设置 `skip-queries: true`，完成后核验 finalized 数据库、源码归档、锁/guard、资源、官方闭包及尚无查询结果，封存并上传提取 checkpoint。

随后以两个隔离 pack 顺序运行官方、私有补丁的原 `CallsCandidate`/`CallsLegacy` 查询。官方两项完成后先解码并保存，再运行候选；两侧采用独立编译/common cache、BQRS 路径和研究 artifact，同一数据库 evaluator 缓存仍共享。失败保留已有有界原始结果与收据，不追加解码或重跑。最终重新核验源码、官方库、数据库 metadata 和已封存 checkpoint；身份失败不得报告完整对照。

整个研究 job 最多 150 分钟，提取最多 25 分钟；比较最多 120 分钟，每侧批次最多 60 分钟，均受同一个实际 job 截止约束。比较在总截止前至少 450 秒停止，内部另留 60 秒审计；不把阶段上限相加来延长 job。一次运行后停止，并分别报告提取、四个观察、对照和完整 job 成本。它不运行或验收默认 37 项和完整七表，相关状态为 `NOT_REQUESTED`，生产容量仍为 `UNKNOWN`、Rust 覆盖仍为 `HOLD`。研究成功不自动启用 Rust，也不解除其他回归或运营验收。原容量与附加 15 分钟比较入口保持不变。

该模式的原生依赖准备最多 360 秒，prepare 步骤最多 11 分钟，均计入同一 150 分钟；其他模式仍为 180 秒原生准备。先读 `native-budget.json` 与 `native-update.stdout.result.json`、`native-install.stdout.result.json` 区分子阶段耗尽和总 job 超时：特权 `timeout` 的退出 124、安装尚未完成或探针未执行都不能计为依赖就绪。包/模块探针仍保留，失败停止 metadata 与提取。若源码提取和对照被 skipped，四个调用观察应写未运行；不能由失败采证步骤成功推断语义通过，也不能自动追加重跑。

### 候选库完整工作负载研究

Trace refs: O7-REQ-351dbec8-3110-4ccd-aadc-49d0ba84bd89；执行边界见[候选完整工作负载合同](codeql-integration.design.md#724-候选库完整工作负载研究)。

需要验证候选库的完整查询时，选择 `execute=true, mode=candidate-workload-research, compare_qualifier_patch=false`。沿用完整固定 `7c2f99a…` 业务源、官方 2.27.4、原生准备及真实 Action 资源。官方 Action 只提取；独立 checkpoint 通过后，私有源码布局和固定行为候选以同一批次执行原 37 默认查询及七项公共见证。入口不选定生产库或启用 Rust。

先核对分析窗口在提取前已固定：整个 job 90 分钟，提取、私有复制/解析/编译和 44 项求值共用至多 75 分钟的绝对截止，动态保留 600 秒结果验证与收尾；所有阶段仍保留总截止前 120 秒的证据边界。不能在 checkpoint 后重新分配时间，CLI 查询 timeout 不能替代外层墙钟。最终完整 job 时长及至少 270 秒余量须在 artifact 上传与 post 后独立读回。

同时读取官方 checkpoint 中的 `action-deadline-terminal.json` 与 owned-process 收据：真实官方 main 必须只运行一次，启动/退出/收尾均绑定同一分析截止，启动已过期时不得运行提取。监督器通过该 step 的显式 `NODE_OPTIONS` preload 接入，不通过 `GITHUB_ENV` 写该变量；stdout/stderr 保持 runner 处理，post 正常收尾。相对 step timeout 或预估启动余量不能代替绝对监督收据，监督/取消/采证失败均不得准入候选求值。

预算文件的大小检查、内容读取和摘要绑定必须一致：Node preload 以 nofollow/nonblock 打开 regular file，在同一文件描述符上最多读取 256 KiB + 1，并拒绝超限；supervisor 对同一有界读取的字节执行预算解析和摘要计算。路径替换不能改为读取另一对象，检查后的文件增长不能突破实际读取上限。这是 admission 的局部加固，不改变绝对截止、官方 post 或 Rust 启用状态。

从独立候选 artifact 检查 `qualifier-lines-constraint-matching-v6` 与 `image-v2` 合同、私有包装差异、37项原QL/七表文本身份及完整标签预期、44条实际 closure 和源/库/query pack 的前后身份。依次核对原 qualifier 补丁、`arity-patch-identity.json` 和 `lines-model-identity.json`：真实 v3 中间 `rust-all` 恰有 TypeInference/core.model 两文件变化。`trait-visibility-identity.json` 和 `shared-helper-identity.json` 保留 v5 版本，分别按 §7.2.4 核验 Rust 两处注解及 shared 原 helper 的完整中间快照、固定摘要与来源。

再核对 `constraint-matching-identity.json`：两份 v5 收据摘要必须匹配，shared 基线由当次官方快照重新验证，最终仅同一 TypeInference 文件从 `4a65…` 变为 `8b32…`。区分官方原 patch 与仓库规范应用 patch 的摘要；规范化只改变 diff 表示，实际应用结果必须符合设计中的固定完整文件摘要。最终两个私有包合计仍只允许三个 Rust 文件和一个 shared 文件变化，集合与 mode 不变，44条实际 closure 必须选择两份私有根。缺身份、伪造中间基线、旧字节、其他差异或官方 shared 回退均拒绝，即使重新封口也不放行。旧 v5 超时事实保留在 [#4989](https://github.com/nil-studio/oasis7/issues/4989)，旧收据不代签 v6；生产 v3、WASM helper 和旧 qualifier 对照保持原契约。原发布包的嵌入依赖及 `.qlx` 不能进入私有源码布局，命令还须显式禁用预编译回退。`suite-helpers` 是固定 query 角色包，不能按普通 library 强行放行。

Image v2 实际表应包含两处 format/精确 callee、digest 到各自 format、同一函数/循环/比较式的结构见证、expected→RHS localFlow、原 output→LHS Global taint，以及错函数、交叉操作数和 whole-bool 零负例。读取实际 QL、版本、标签和配对结果，不只看表的 PASS；派生 taint 不能称为字符串值相等。小 fixture、双 receiver 和同名非 stdlib 控制的通过范围另列，不能代替完整原仓库。历史 v1 的零值和 hosted failure 保留，不把本次修正后的判据追写为旧运行成功。

默认37项逐项核对完整 info/decode，七表逐项读取实际结果与各自冻结版本的 oracle 判定；v6 研究候选必须重新完整计算37+7，不能复用旧37结果。两处可见性计划的结构改进和官方小 fixture 结果不证明全仓耗时、内存或递归收敛改善。SARIF 只对37项在 runner 本地解释和校验，上传目录只含其摘要与验证收据；不上传 SARIF、数据库或源码 archive。失败保留已有有界原件和真实首失败阶段，64 MiB 私有证据超限或部分采证保持不完整，不追加查询或重跑。研究终态、执行/语义、身份、资源与最终余量分列；`production_capacity=UNKNOWN`、`full_rust_coverage=HOLD` 与原生产验收状态不变。

该批次的 `candidate-evaluator-v1` 使用固定 2.27.4 的 level 3 minified evaluator 日志并禁用 tuple counting。原始事件只在 owned FIFO 和最多 48 MiB sealed memfd 中存活；不上传或另存原文。64 MiB 总限同时包含临时内存、持久化证据和终态预留，memfd 存活期间预留完整 48 MiB；所有观测投影、样本与收据合计最多 8 MiB。先核对 batch 原退出、已有结果和身份，再读官方 predicates 摘要的独立状态；摘要最多一次、20 秒且含清理，仍受原证据截止约束，余量不足不启动。

原文保留达到 48 MiB 时，capture 保留 `MEMORY_LIMIT` 和 `INCOMPLETE`，继续排空、计数和哈希，不再保存新增原文。原 batch 及结果/oracle 校验可在原预算内收尾，官方 summary 跳过；总体研究仍不完整。分别读取 observed、retained、discarded 字节数及 transport EOF，不把保留前缀的 header/footer 或事件数当完整日志。写入、证据空间、进程身份和清理异常仍按原规则中止。

在同一 `candidate-observation/observation-terminal.json` 中，`capture.feed_cost` 按 `BEFORE_LIMIT`、`CAP_CROSSING`、`AFTER_LIMIT` 列出回调次数、输入字节和 wall/thread CPU；跨限整次调用只计入一个桶，恰好填满仍计入前者。`batch_parent_cpu` 单列原 `runtime.run` 区间的父进程全部线程 CPU，不含查询子进程。各时钟独立记录已测小计、未测次数和完整总量；未观察或测量不完整时总量为 `null`，不能读成零成本。父进程与回调计量范围重叠，不能相加或相减归因；计量包含观测扰动，不能据此改判旧运行、原查询失败或研究验收。

进度时刻是 stdout/stderr 接收时刻，不能视为内部精确编译/求值阶段。summary 首写前严格验证版本/schema，任意名称、路径、RA、源码位置和异常只保留受限 hash/类别；缺 footer、未知形状、截断和容量耗尽必须保留不完整及实际字节覆盖。literal 毫秒、已知迭代小计与未知 `-1` 分列，不重复归因共享工作；部分记录不是整个批次完整排名，采样资源不是硬峰值或独占 CPU。采集完整、日志完整、查询完成和 oracle 通过各自判定，观测错误不覆盖原首失败；开启观测带来的额外成本也不能用于证明 Image 或 lines 是唯一根因。

记录身份使用本次官方摘要 stdout 的 SHA256 与 header 后的原记录序号。`raHash` 可以复用，不是事件 ID；同 RA、同策略或不同名称的合法记录均保留各自计时，复用计数不改变完整性。不按 RA 去重、跨记录求和或向多个查询重复分配成本。

### 合法 WASM 单角色 metadata 诊断

在默认分支手动选择 `CodeQL Rust WASM Metadata` 并显式设 `execute=true`。本入口固定完整 7c2f99a 源码、Cargo 1.99.0 的完整提交、recovery-fence 和 wasm32-unknown-unknown，不提供任意命令/版本/源码输入，不运行 CodeQL、Cargo check/build 或部署。实现和验收边界见[metadata 诊断合同](codeql-integration.design.md#725-合法-wasm-单角色-metadata-诊断)。

先核对真实 job start 绑定的 1200 秒时钟：准备至多使用前 780 秒；剩余至少 420 秒才进入 300 秒 D1/D2 窗口，其中最后 60 秒用于清理、after/guard 审计和封存，另留 120 秒给上传/post。源码获取、工具安装和联网依赖预取都计入；clock 失败、精确版本不符或准备超时不得用新截止、另一 nightly 或在线 D2 回退。宿主 proxy 保持原位置，HOME/CODEX_HOME 不改，任务 Cargo/Rustup/cache 与业务源码和 evidence 分离。

先读 bootstrap/工具与源码身份、阶段收据及终态，再核对 D1 的父根和 D2 的完整 53 成员集合、custody 实际角色、六个固定本地 package、依赖种类和 target 表达式。元数据图与非图自由文本分开处理；stderr、安装/网络输出及异常采用首次写入前投影，不是原始日志。合法图若记 ROLE_CONTEXT_MISMATCH，按其实际 features、成员差异和固定失败项解释，终态仍为 INCOMPLETE；缺失或被拒绝的输出不构成阴性语义结果。sealed 清单须覆盖所有普通证据文件，包括 guard 的隐藏控制文件，合计不超过 64 MiB。

`ROOT_WORKSPACE_ROLE_METADATA_OBSERVED` 仅表示本次 Cargo 上下文观察完整；实际 upload/post 和整 job 成本需平台终态另行回读。metadata 不证明目标标准库已安装、WASM 编译、宏展开、每个编译单元 features 或 CodeQL 语义；Rust HOLD 和生产容量 UNKNOWN 保留。每次只运行一次 D1/D2，不自动重跑；父目标按后续真实结果决定下一项建模观察。

核对 guard 版本投影 adapter 的首次绑定回执：counter/failed 初值为 0、原/新 wrapper 与 helper SHA 均齐全；原 guard 未修改，真实 Cargo 的当次输出先投影再进入 audit。NOT_RETURNED 表示原版本调用未交付可验证完整输出，bytes/hash 保持未知；不能用已缓存版本或事后删日志补成通过。

### 合法 WASM 单角色 CodeQL 语义诊断

在取得本次真实补证运行授权后，从默认分支手动选择 `CodeQL Rust WASM Context` 并显式设置 `execute=true`。源码固定为完整 7c2f99a，工具固定官方 Rust-only CodeQL 2.27.4 和 Rust 1.99.0（含 rust-src、wasm32-unknown-unknown 标准库）；没有任意源码、角色、查询或版本输入。它使用原共享 slot 1，不修改生产批次或 observe 配置。入口合同见[WASM CodeQL 语义诊断](codeql-integration.design.md#727-合法-wasm-单角色-codeql-语义诊断)。

先核对实际 job 起点绑定的 1800 秒预算、前 780 秒准备及一次最多 900 秒提取/小查询窗口；其中最后 60 秒做采证封存，另留 120 秒用于 artifact/post。下载、依赖 fetch、版本与完整源身份都计时，未剩足 1020 秒不得新开观察窗口。失败只收已有证据，不重试、不换工具链、不借旧 D1/D2 的 PASS 代签本轮。

bootstrap 的时钟读取最多使用初始 20 秒，只有身份完整匹配的唯一 job 仍为 `queued` 或 running 的 `started_at` 明确为 null 时才有界重读；不重跑工作流或重新分配预算。先读 `bootstrap-terminal.json` 的固定错误类别和白名单时钟投影，再对照 `bootstrap-http.json` 的逐次 HTTP 状态、字节数、摘要及读取次数。缺失/非法时间、未来或陈旧开始时间、身份错和重复 job 仍硬拒绝；缺 `job-clock.json` 时不得称预算已绑定或后续提取已执行。历史证据仅记录 `Rejected` 时，其具体拒绝子因保持未知，不能追认为 API 发布延迟。

`wasm-context-<run>-<attempt>-<tool-sha>` artifact 只包含 evidence 目录。依次核对 `terminal.json`、`job-clock.json`、`source-before/after.json`、`official-library-closure.json`、`library-closure.json`、`query-library.json`、`metadata-graph.json`、`actual-index.json` 和 `guard-verdict.json`：工具根必须在业务根外，完整根 workspace 保留 53 成员，实际索引必须与六包下 `.rs` 集合精确相等，guard 必须有预检之后的根 metadata 和精确 recovery-fence WASM `cargo check --lib` 调用。请求配置文件只表示请求，不构成实际上下文通过。

本入口的 `library_variant` 固定为 `wasm-shared-helper-v1`。核对 `shared-helper-identity.json` 的固定补丁、上游提交及 shared 0.0.42 全部文件前后身份：只允许 §7.2.4 的 shared TypeInference 两个 helper hunk，stock rust-all 与其他官方库不变。实际 resolver 的 13 库集合必须只把 shared 根替换为本次私有副本；官方 shared 回退、额外修改、缺收据或未完成准备均拒绝。补丁与 helper 一同冻结，准备和应用使用原准备窗口；`helpers-after.json` 在成功或失败时重验官方 bundle、query pack 与私有 shared。该研究身份不代表已使用生产 v3，也不代签 §7.2.4 的完整候选合同；旧官方库运行的超时或未知结果保持原终态。

固定 CLI 的依赖接线回归为 `python3 scripts/security/codeql-rust-wasm-context.test.py RealSharedResolverTests`，显式设置 `OASIS7_CODEQL_WASM_SHARED_CLI` 为 2.27.4 可执行文件后，在一次 120 秒总界内实际应用固定补丁并解析私有及官方 shared 两种 closure，确认官方回退被拒绝、九项 QL 摘要及官方 bundle 前后不变。该回归使用独立缓存，不提取数据库、编译或求值；默认跳过，结果不代签 WASM 语义或性能。

`witness-observation.json` 的九行计数分别观察四个真实宏展开、两个普通项负例、两组静态调用正反例及数据库内 custody crate 的 WASM/角色 cfg。任何正例缺失、错误目标或非法角色出现、缺行、重复、未知 schema 都保持 INCOMPLETE；单有文件、metadata、编译成功或零负例不能通过。查询结果仍需结合 source/query/BQRS 摘要、完整封存清单及实际平台终态核对。私有 DB、原生日志、TRAP、源码归档和缓存留在 runner 临时工作目录，绝不打包或上传。

查询失败时先读 `witness-query.result.json` 的原退出码、`failure_kind`、清理和完整流摘要，再读 `query_progress` 的冻结查询身份、四类编译/求值事件计数及首末接收时间。未知行、非法编码和无换行尾部只保留计数与流身份；无事件不证明该阶段未发生，接收时间也不等于内部阶段耗时。投影错误不能覆盖原 timeout 或非零退出，亦不能用成功的进度事件替代九行结果验收。该投影随唯一原查询采集，预算、查询次数和上传边界保持原合同。

进度通道使用 `--loglevel=ERROR --verbosity=progress`，仅 `witness-query` 移除 `--log-to-stderr`，避免详细日志路由吞掉普通进度。固定 CLI 的可选小回归为 `python3 scripts/security/codeql-rust-wasm-context.test.py RealProgressTests`：显式提供 `OASIS7_CODEQL_WASM_PROGRESS_CLI`（2.27.4 可执行文件）与 `OASIS7_CODEQL_WASM_PROGRESS_RUST_BIN`（1.99.0 的 cargo/rustc 目录）后，在一次 120 秒总界内建立单文件临时 Rust DB、执行一次玩具查询并验证四事件与一行 BQRS；默认跳过。仅 schema 收据可由 `OASIS7_CODEQL_WASM_PROGRESS_EVIDENCE` 指向独立目录保留，临时 DB/原生日志/缓存会清理。该回归只验证固定 CLI 输出合同，不代签真实 WASM 九项见证或原运行预算。

只有本轮 metadata、真实提取、cfg、宏/调用、guard、前后身份与封存全部成立，才记 `WASM_CONTEXT_SEMANTICS_OBSERVED`。该状态限于固定旧源码上的小见证，不代表当前 main、完整 WASM 支持范围、全数据流、生产容量或运营启用；`full_rust_coverage=HOLD` 和 `production_capacity=UNKNOWN` 保留。本地 `query compile --check-only` 只验收公开 QL API，真实提取/查询未执行时如实记 NOT_RUN。

### 短原生准备诊断

安装阶段的无时间戳输出不足以解释耗时时，经一次短诊断授权，在默认分支手动运行 `CodeQL Rust Native Diagnostic` 并设置 `execute=true`。它最多 10 分钟，原生 update/install/probe 共用 360 秒；包、镜像顺序和获取参数保持原样，仅增加受限 Worker 观测。不会准备 Rust/CodeQL 或启动完整研究，运行一次后停止。

从 `native-diagnostic-<run>-<attempt>-<sha>` artifact 先读 `native-terminal.json`、`native-budget.json` 和各命令 `.result.json`，再按 stdout/stderr 中的 JSONL 投影接收时间及原流 offset 关联 URI Start/Done/Failure/Redirect。文件是白名单投影，不是原字节日志；未知、截断和被丢弃内容的摘要不能当作下载结果。601 配置记录只含允许的有效参数，`sources.json` 保存受限源/镜像观察，`archives.jsonl` 保存固定缓存目录的只读元数据采样。只有 Done 才是协议报告的下载完成，Get 或 partial 大小变化均不能独立证明包安装完成。

`NATIVE_READY` 仅表示本次四步原生准备通过；失败、不可读采样、未观察参数和 root 子树清理范围分别记录。用时间与失败事件缩小原因，不能仅凭一次通过/失败归因镜像或地区，也不能自动重跑长研究、启用批次或推定 Rust 覆盖完成。合同见[短原生准备诊断](codeql-integration.design.md#723-短原生准备诊断)。

## Hosted 验收与回退

授权运营者开启 baseline 前，读回默认/advanced setup 的有效状态并处理重复扫描，核对 required checks、rulesets/code-scanning protections、最小权限、action pins、fork/Dependabot 和缓存隔离。随后记录四语言 default/extended 的实际 execution、upload、native Security findings、manifest/extraction diagnostics、控制样本、覆盖缺口及 owner；验证共享槽位峰值、跨 PR 排队、同 PR 取消与实际来源。将样本耗时、runner 竞争和 required CI 延迟作为实测结果，不能用 fixture 或静态 YAML 代替。

observe 只能在 hosted 验收完成且明确授权后设置，并读回实际变量与非 draft PR 行为。当前文档不证明这些步骤已发生。平台 provenance 无法验证或覆盖缺口未解决时，保留能力不足事实并交验收 owner，不能宣布完整接入。

资源压力时可经授权先将 slots 降为 `1`；需停 PR 扫描时退至 `baseline`；需停全部重型执行时退至 `off`。变量设置影响后续计划，已有运行须另行核对或按授权取消。读回设置并确认新计划行为，保留既有 alerts、analysis 与失败证据；不要删除告警、改 category 或改 required policy 来制造回退成功。

该变更面向工程运营，没有玩家功能或世界规则变化。发布说明可描述“新增默认关闭的观察性扫描实现”；在取得实际验收前，不能声称仓库安全通过、漏洞为零、四语言覆盖完成或 observe 已上线。任何对外公告与玩家承诺需另行授权并由 LiveOps 与产品 owner 核对。


### CFG 关系规模诊断

`CodeQL Rust CFG Diagnostic` 是默认 `execute=false` 的独立手动入口，按诊断模式固定业务 pin，复用官方 2.27.3 完整 bundle、原生依赖和源码隔离检查；它不随容量入口升级 pin。它只 finalize 数据库并执行 CFG 定向测量，不调用 analyze、不运行默认安全查询、不启用批次、不上传 SARIF/数据库。一次完成或失败即停止，不自动重试。整个 job 45 分钟，包含准备、提取、查询、图分析和收尾；结果不替代生产容量或完整 Rust 覆盖。

先审阅私有 QLL 可见性补丁和查询实际解析路径，再执行 scope 汇总及有界 top scope 图检查。结果必须分别记录全图汇总、选中 scope 覆盖、跨 scope/未知归属、分量与可达对计算是否完整；只输出达到限制前的局部数据时保持 INCOMPLETE。保存工具/业务/官方源码身份、精确 QL、解码结果、原始日志、退出码和固定时钟；不保存或导出完整巨型闭包。新运行只能说明新冻结数据库，不能复原旧 run 的图。

使用 `codeql-rust-cfg-diagnostic.yml` 的 `execute=true, callee=true, profile=false` 可进行一次调用目标分阶段诊断，固定 f9ab 失败输入，只提取及执行定向查询。先按原顺序运行 Syntax、ResolutionCounts、TargetFunctions、OriginalCalls 四表，再追加 InternalPaths、InternalTypes，观察调用路径及目标impl self的解析与类型提及关系。新表只正常导入完整官方2.27.3导出的内部接口；不修改官方库，提取前check-only验证接口及解析闭包。完整关联函数路径的resolvePath空值、调用处省略泛型参数的TypeMention空值均需结合上下文解释；AST文本、内部关系不代替公共静态调用验收。原公共摘要在解码后保留，后表失败仍为整体INCOMPLETE。45分钟总预算、每命令900秒、180秒收尾预留及100行表上限共同约束全部查询，不通过重试或追加表续期。结果仅定位失败边界，完整覆盖和生产容量保持独立；声明存在不证明调用已解析，无SARIF/数据库上传。

官方默认 evaluator profile 使用默认关闭的 `codeql-rust-cfg-diagnostic.yml` 显式 `profile=true` 分支，仅手动一次运行。保持55分钟job、40分钟共享提取/查询窗口、固定e205源码与完整官方2.27.3默认suite，不上传SARIF/数据库、不启用批次。查询后先冻结不可覆盖的query终态及原始profile哈希，再保存身份/资源/锁与核心capture终态，最后才尝试数据库日志及官方字段投影摘要。

摘要仅单次运行官方 `--format=predicates --minify-output` 并流式投影，增量8MiB、命令20秒/阶段30秒且截止不晚于job截止前300秒；总写入248MiB/终态256MiB不变。不足空间或时间不spawn，已有摘要不覆盖，unsafe或核心证据不完整拒绝。摘要PARTIAL/NOT_RUN不得覆盖主查询及原始profile；保留旧run40898591506的各阶段失败。投影中的计时不是独占CPU，部分输出不得当完整成本排名；本诊断不代规定公共见证、结果解码、生产容量或完整Rust验收。

摘要字段投影只消费官方predicates摘要，不直接把原始事件当稳定摘要：每条记录至多32MiB、总输入256MiB、物理记录十万条，按 `codeql_summary_schema_2_27_3.json` 冻结的19种已观测字段/类型/策略签名识别记录；这只是旧partial输入的适配白名单，不是永久官方schema。识别raHash、predicateName、position、completion标记及两类计时；先识别唯一版本头；官方无timing字段的cache元记录单独计数而不按零耗时排名，所有可比较记录仍要求完整计时字段；仅允许同一raHash及同名predicate的一条SENTINEL_EMPTY元记录与一条IN_LAYER计时记录，并分别计数、不重复归因；拒绝同类重复、名称冲突、未知缺字段和截断，保留-1标记计数而不当零或据此推断是否完成。literal `millis` 与非负 `predicateIterationMillis` subtotal 分成两份排名各至多50条，绝不混排或相加；首次写stdout前核最终JSON编码（含转义）至多1MiB，不跨记录加总、不把共享alias重复归因、不称独占CPU。父执行器管理投影worker和官方CLI整个进程组，命令仍20秒且总产物增量8MiB（含receipt预留），语法/容量/时间失败均保留PARTIAL。旧text/dedup候选已在唯一离线测试中因RA逐条输出触cap淘汰，失败证据保留。
