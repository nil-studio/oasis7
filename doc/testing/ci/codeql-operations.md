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

## Hosted 验收与回退

授权运营者开启 baseline 前，读回默认/advanced setup 的有效状态并处理重复扫描，核对 required checks、rulesets/code-scanning protections、最小权限、action pins、fork/Dependabot 和缓存隔离。随后记录四语言 default/extended 的实际 execution、upload、native Security findings、manifest/extraction diagnostics、控制样本、覆盖缺口及 owner；验证共享槽位峰值、跨 PR 排队、同 PR 取消与实际来源。将样本耗时、runner 竞争和 required CI 延迟作为实测结果，不能用 fixture 或静态 YAML 代替。

observe 只能在 hosted 验收完成且明确授权后设置，并读回实际变量与非 draft PR 行为。当前文档不证明这些步骤已发生。平台 provenance 无法验证或覆盖缺口未解决时，保留能力不足事实并交验收 owner，不能宣布完整接入。

资源压力时可经授权先将 slots 降为 `1`；需停 PR 扫描时退至 `baseline`；需停全部重型执行时退至 `off`。变量设置影响后续计划，已有运行须另行核对或按授权取消。读回设置并确认新计划行为，保留既有 alerts、analysis 与失败证据；不要删除告警、改 category 或改 required policy 来制造回退成功。

该变更面向工程运营，没有玩家功能或世界规则变化。发布说明可描述“新增默认关闭的观察性扫描实现”；在取得实际验收前，不能声称仓库安全通过、漏洞为零、四语言覆盖完成或 observe 已上线。任何对外公告与玩家承诺需另行授权并由 LiveOps 与产品 owner 核对。
