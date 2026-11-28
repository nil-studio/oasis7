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

按[当前 Rust 日频目标](codeql-integration.design.md#rust-daily-baseline-goal)，Rust/default 验收后长期保持 `baseline`，每日北京时间 11:29 扫描主干；它是正式运行模式，不是等待升级 PR observe 的阶段。Rust PR 扫描不列入本目标后续必做项，extended 和更广 target/角色覆盖另按范围处理。后续关注自然调度、失败、新鲜度和告警处置。

Rust default 默认分支维护使用 90 分钟总预算。init 后读取当前 run/attempt 的唯一 Rust job，以 GitHub Date 和 started_at 计算剩余时间，再分配最多 75 分钟分析，预留 600 秒用于上传和采证；余量不足一分钟直接失败。在完整批次配置中仅设置 `rust-repo/default=baseline`，允许已授权的首次手动验收及验收后的每日持续扫描；保留其他批次原值和支持非 Rust PR 扫描的全局 observe。失败回退仅删除本次 Rust 键。日频共享 slot 1，排队影响仍需观察；24 小时健康判据保持原样，调度或执行波动可能造成短暂 stale。

该场景使用固定 CodeQL 2.27.4 Rust bundle 和独立固定的 `qualifier-lines-negative-projection-v8`，保持官方原始 37 项默认查询。核对生产 prepare/run 的最终 v8 variant、真实 v7 base_variant、原 v3/v5 中间快照和 v6/v7 全链收据，以及四棵私有库的前后完整身份；实际 37 条解析须在 14 项依赖中选择 `candidate-rust-all` 和 `candidate-negative-typeinference`，同时保留并核验真实 v6 `candidate-typeinference` 和 v7 `candidate-target-typeinference`，不能只看 search path 或版本标签。negative 收据重新关联真实 v7 全树、target 收据摘要与固定补丁三 SHA；保留原文件权限检查，并核对官方/v7/v8 完整目录集合和目录、文件权限。旧 v2/v3/v5/v6/v7 生产准备、研究或观测 preparation 不得改标签恢复为生产 v8；实际 shared 解析回退 v7、v6 或官方根均拒绝。官方 Action 负责初始化与提取，生产 helper 校验实际闭包后执行源码查询并生成 SARIF，仍由原上传步骤关联 Security；不加入研究七项见证或研究专用观察器。库准备、提取、查询与结果校验共用一次分析截止，资源沿官方 init 的原导出值；PR 和 extended 保持各自原路径。技术合同见[生产库接入与首次验收](codeql-rust-extraction-and-coverage.md#726-rust-default-生产库接入与首次验收)。

首次验收同时检查独立的 `oasis7-codeql-rust-production-*` artifact：工具和修补身份、当前 checkout 的 manifest/锁观察、实际 Cargo 解析、完整查询结果及覆盖诊断。逐项核对原 37 份 info/decoded 的全部结果集和行；解码显式保留实体 ID、文本及 BQRS 已提供的 URL，按实际位置核对未展开宏，不能用重复文本猜测位置。URL 缺失保持未知，细节见上述技术合同。`production-run.json` 的执行成功不代表覆盖通过；coverage 的 REVIEW/UNKNOWN、WASM 或源码归属缺口继续记录。该 artifact 与只包含 `evidence.json` 的上传关联 artifact 分开，后者仍用于唯一 SARIF/platform analysis 关联。只有覆盖及运营验收闭合后才将该批次声明为已验收日频基线。

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

Rust 的 default/extended 扫描先执行 `Prepare Rust native extraction prerequisites`，复用容量入口的 GTK 3、WebKitGTK 4.1、D-Bus 安装与 pkg-config 探针；准备共用最多 180 秒，计入对应 job 的总预算；默认分支 Rust default 为 90 分钟，其他 Rust 场景保持 45 分钟。失败时从独立 `oasis7-codeql-native-<run>-<attempt>-rust-repo-<profile>` artifact 查看安装 stdout/stderr、包和模块版本及 `native-terminal.json`；失败会阻止提取、查询和 SARIF 上传。native PASS 只表示该 runner 的依赖准备成功，覆盖和上传关联仍须分别验收。

仅 Rust default 的非 PR 维护调用采用 `ubuntu-archive-https`：使用已验证的私有源副本策略，源准备、四步命令、前后核验及清理共用原 180 秒；PR、extended 与研究入口保留 `runner-sources`。先核 native terminal/budget 的实际 profile，再核 `native-source-profile.json` 的唯一替换、原输入和有效副本前后摘要、未改系统源及清理成功，最后核四条命令的真实 exit、argv 与分配。正式 native 保留普通日志，不开启短诊断的 Worker 投影或采样；其收据在分析分配前产生，须与 production 查询 artifact 分开验收。失败终态保留异常类型、命令 exit/failure_kind 与源收据保存错误的固定字段，不保存异常原文；不由候选诊断的成功代签本次 native 或整体生产通过。

分析 job 从官方 pinned upload-sarif action 的 `steps.upload.outputs.sarif-id` 生成闭合 schema `oasis7-codeql-upload-evidence/v1`，记录 repository、workflow path/ref/SHA、run/attempt、job key/name、checkout SHA/ref、unit/profile/category 与真实 execution/upload outcomes。上传未成功时 ID 为空。每个 job/attempt 使用独立 `oasis7-codeql-upload-<run_id>-<run_attempt>-<unit>-<profile>` artifact，唯一文件为 `evidence.json`，由完整 SHA 固定的 upload-artifact action 上传且不覆盖。取消、超时或 runner 丢失可能没有 artifact，不能补写或借用旧输出。

consumer 不信任 JSON 自述或 artifact 名称：先独立验证 live repository/default branch、可信 workflow、精确 run/latest attempt、唯一 job/check/provider/suite，并确认执行时 workflow 字节与当前默认分支 workflow 相同。无关 main 前进且 workflow 字节不变不使结果失效；workflow 改动、候选执行或读取失败保持 unknown。随后完整、有界读取该 run 的 artifact 清单，要求唯一、未过期、元数据匹配，并在下载后复验 run/attempt 与 artifact 身份。

archive 下载上限 1 MiB、超时 30 秒，并校验平台 `sha256` digest；ZIP 不解压到文件系统，只接受一个常规 `evidence.json`，JSON 上限 64 KiB，拒绝路径穿越、额外/重复项、符号链接、加密、错误编码、重复键及未知字段。官方输出 ID、全部 payload 身份与真实 extraction/upload 成功必须一致，才能与唯一精确 SHA/ref/category 的 CodeQL `analysis.sarif_id` 且无 analysis error 关联。分页溢出、大小超限、digest 不匹配、缺失/重复/过期 artifact 或失败的新 attempt 都保持失败或 unknown；该证据不证明零 findings 或 manifest coverage。

`previous_analysis_id` 与 `previous_analysis_age_hours` 只是精确 ref/SHA/category 下最近可见 analysis 的历史参考，不保证属于当前 attempt，也不能证明 latest upload 成功或覆盖新鲜。新 attempt 排队、取消、超时或上传失败时，保留该 attempt 的真实状态；旧 analysis 即使仍新鲜也不使报告 healthy。缺少有效上传关联证据时须保留 unknown 并交验收 owner 核查；artifact fixture 通过或 health 报告生成不能据此宣布 hosted 验收通过。

CodeQL 扫描保留实际发现和运行来源；required/advisory 由 GitHub CI 声明及生效保护决定。必需安全检查未成功时不得合入，不通过本地解释改写远端结果。

<a id="native-configuration-lifecycle"></a>
## 原生配置提示与生命周期

原生 `configurations not found` 表示原生比较尚未匹配到目标分支既有配置的相应分析。PR 固定 default，extended 只参加默认分支维护；按影响范围未选中 default 单元时也不会补传结果。Rust/default 按日频目标保持 baseline，PR 缺少对应 Rust 分析可以是预期行为，应核对当次计划与主干维护扫描；不为消除提示而开启 Rust PR 扫描、删除有效配置或补传空结果。分别核对事件、可信计划和实际分析，再解释覆盖提示与同时展示的告警；不能仅凭检查标题判断失败原因。

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

本入口用于已授权的候选源码测量，遵循[有界公共模型性能诊断](codeql-rust-extraction-and-coverage.md#72-rust)，不执行安全查询集、不上传 SARIF、不修改批次设置或 required policy。源码与诊断程序分别固定 OID；每轮开始前记录探针要区分的假设、预期观察和停止条件，保留旧失败，不自动重试。

`scripts/security/codeql-rust-model-diagnostic.py --help` 列出实际参数。`--probe` 仅选择 `node-only`、`endpoints`、`peer-df`、`driver-non-target`、`driver-target`、`calls` 或 `full`。先用最小前置探针确定公共 Node、调用解析和局部流的成本边界；已有宏 AST 成功证据不要求每轮重做。`endpoints` 的存在性结果辅助解释 wrong-branch/wrong-state 等负例，不能取代原负例。`full` 仍须完整标签 union、全部正负期望和精确 Files 身份通过。

每个分区单独记录开始/结束、退出码、BQRS/解码表、指标判定和缓存状态，成功后立即解码；后续超时保留此前结果并标明完整批次未通过。一次模型阶段的所有求值、预热及解码共同受原 180 秒截止控制，全轮最多 1080 秒；多个独立轮次的局部结果不能拼成原完整批次的预算内成功。日志汇总优先使用官方 `generate log-summary`，共享阶段去重后的 wall 时间也不是 CPU 时间。

资源样本必须说明进程身份、采样间隔、观测峰值与真实峰值的区别，以及退出时的缺口；采样不到不能记作零。纯结构化 evaluator 日志、progress、tuple 信息、官方摘要、遥测和结果共受 64 MiB 限制。证据空间不足或身份/清理异常时终止后续子进程，保存具体失败；不删除旧证据、不循环探测空间、不通过丢弃失败原文伪装满足限制。

官方版本对照固定同一业务源码、Cargo 图、实际 Rust 工具链和提取配置，各用完整官方 bundle 与独立新数据库、缓存。先比较相同探针；新版能完成后，再做原完整公共见证验证。提取缺失而变快不算改善。若 Node-only 已呈现同一昂贵前置、新版受控对照仍未完成且成本仍集中在官方共享阶段，结束没有新机制的本地改写，准备上游材料：源码/manifest/lock/vendor 身份、bundle/pack 摘要、完整 QL 与期望、原始日志和官方摘要、资源样本、终态及可重建命令。上游公开提交另需授权。

性能问题解决与 Rust 启用是两项验收：本入口即使完整 candidate 通过，也不代签 legacy、Linux image、全仓覆盖及运营验收。具体运行身份、结果和未知项写对应 PR/Issue，不在本手册追加运行历史。

诊断由 Rust workflow 的 `workflow_dispatch` / `run_mode=codeql_rust_model_diagnostic` 显式触发，`model_probe` 选择上述探针，`model_bundle_version` 只接受固定 allowlist 中的 `2.27.2` 或 `2.27.3`，默认旧版；本地入口对应 `--bundle-version`。完整 bundle URL/摘要及解析包闭包随选择绑定，不能混用新版 CLI 和旧版库，也不接受任意下载 URL。派发前冻结诊断 source SHA 与业务 checkout SHA，读回唯一 run 后沿该身份观察，不重复派发。

`model_probe=trait-scope-plan` 仅在完整官方 2.27.3 上做隔离库编译计划实验：将 trait 可见性图的 ItemNode 域限制为目标点所属作用域及其完整反向外层闭包，保留原作用域跳转、图边和相关 trait/element 配对。有限图差分和路径等价证明不代替实际 Rust 语义验收。入口先核固定官方文件摘要，仅在私有副本中替换一处；两版分别编译公共 localFlow 与 getStaticTarget 查询，使用独立冷编译缓存、同一 360 秒共享编译预算及原 1080 秒/20 分钟总门。没有数据库、提取或求值，不读取业务源码，不上传 SARIF，不安装无需使用的 Rust 工具链。完整 RA 必须独立复核作用域约束是否进入图边构造，以及是否只是把工作移到 relevantItemNode/getADescendant；无有效减算证据时淘汰或记 INCONCLUSIVE，runtime_go 始终为 false，不能因编译成功自动启动后续求值。官方包原件及其他诊断模式保持不变。

## Hosted 验收与回退

授权运营者开启 baseline 前，读回默认/advanced setup 的有效状态并处理重复扫描，核对 required checks、rulesets/code-scanning protections、最小权限、action pins 和适用的缓存隔离。按本次目标批次记录实际 execution、upload、native Security findings、manifest/extraction diagnostics、控制样本、覆盖缺口及 owner，并验证共享槽位和实际来源。Rust/default 以日频 baseline 验收持续运行；采用 PR observe 的批次另核对 fork/Dependabot、PR 路由、跨 PR 排队与同 PR 取消。将样本耗时、runner 竞争和 required CI 延迟作为实测结果，不能用 fixture 或静态 YAML 代替。

只有目标包含 PR 扫描的批次，才在 hosted 验收完成且已有授权覆盖后设置 observe，并读回实际变量与非 draft PR 行为。Rust/default 按日频目标保持 baseline，不追加 PR 验收或以未开启 observe 判定未完成。当前文档不代替实际验收记录；平台 provenance 无法验证或适用覆盖缺口未解决时，保留能力不足事实并交验收 owner。

资源压力时可经授权先将 slots 降为 `1`；需停 PR 扫描时退至 `baseline`；需停全部重型执行时退至 `off`。变量设置影响后续计划，已有运行须另行核对或按授权取消。读回设置并确认新计划行为，保留既有 alerts、analysis 与失败证据；不要删除告警、改 category 或改 required policy 来制造回退成功。

该变更面向工程运营，没有玩家功能或世界规则变化。发布说明可描述“新增默认关闭的观察性扫描实现”；在取得实际验收前，不能声称仓库安全通过、漏洞为零、四语言覆盖完成或 observe 已上线。任何对外公告与玩家承诺需另行授权并由 LiveOps 与产品 owner 核对。
