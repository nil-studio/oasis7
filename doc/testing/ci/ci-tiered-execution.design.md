# CI 分级执行

Testnet 在同一个不可变 source plan 后并行启动 Web 构建与原生编译，各用独立 runner。原生编译仍只有一个互斥的可信/只读入口；安装包组装等待 Web 成功、且两个原生入口恰有一个成功另一个 skipped，失败、取消或未知状态不能进入组装。原生中间归档只从同一 run 下载，组装前核对 source SHA、platform、target、profile、ops 范围及每个固定二进制的 SHA-256/权限，缺失、symlink 或篡改拒绝；组装不再调用 Cargo。最终 SOURCEPLAN/BUILDINFO、入口验证和校验和合同不变。并行收益以实际 hosted 耗时为准，原生 runner 的 Cargo 并发固定为 1，避免多个重型 Thin LTO 构建同时占满内存；这不影响 Web/native 跨 runner 并行。

- 状态：active
- 专业 owner：`repository_health_engineer`；CI 控制、来源与缓存边界由独立评审者审读
- 通用规则：[开发流程规范](../../engineering/workflow/source-of-truth.md)
- 上游要求：[CI 分级执行 PRD](ci-tiered-execution.prd.md)
- 可执行测试命令和平台限制：[testing-manual.md](../../../testing-manual.md)

## 测试范围

普通 CI 是默认验证方式。成功规划后轻量 baseline 始终执行；选择器使用完整 Git name-status diff、base/head 两侧 Cargo 关系及必要目录映射选择 Rust、Web、场景、平台、打包和运维合同测试。

PR 测试合并提交时，选择范围合并两组差异：实际 target/source 的唯一共同祖先到 source HEAD，以及实际 target 第一父提交到 tested merge。main 在事件 base 之后已存在的改动不重复计入；PR 自身变化和合并结果额外变化仍覆盖。校验 tested merge 恰有两个父提交、第二父提交等于 source HEAD、事件 base 是实际 target 的祖先；父提交身份不符、共同祖先不唯一或对象不可读时扩大为普通全量 required。依赖及嵌入输入分析保留事件 base、共同祖先、source、实际 target 与 tested merge 的图。push/source-only 检查继续使用事件 base 到 source 的完整差异。

Rust 变化覆盖自身及需要验证的反向消费者，删除和重命名覆盖两端；保留有效 features、WASM、生成输入和平台组合。workspace、features、锁文件、共享生成器及 CI 控制输入变化采用保守普通 required，图解析失败仍扩大覆盖；未登记路径按下述门禁阻断。

`./scripts/ci-tests.sh required` 是普通必要测试的本地入口；`full` 保留广覆盖回归，nightly 执行适用 full。真实产品测试、WASM 确定性、共识、持久化、安全、升级恢复和跨平台打包均继续保留，不因流程精简删除。

Command Center 的独立 Cargo workspace 由 `command_center` 组承接，工具目录内的代码、配置、资源和测试变化选择 baseline 和该组；共享 context、local signer、secure envelope crate 变化也选择该组。已核实仅为使用说明的 `tools/oasis7-command-center/README.md` 从该目录规则中排除，保留 baseline 文档治理；排除只作用于这条显式规则，真实嵌入或测试消费者仍可选择相应组，删除和重命名仍检查两端。Rust workflow 调用可复用的 `command-center.yml`，保留 Core、生成类型、UI、Clippy 和 macOS App 打包，并将调用结果纳入 `required-gate`。专用 workflow 不再独立触发 PR/push，避免重复执行；Cargo manifest、锁文件和 CI 控制变化仍采用保守全量。该平台组的本地验证使用专用 workflow 中的 canonical 命令，不由通用 shell driver 执行。

文档 corpus 对象、索引及 evidence inventory 数据更新由始终执行的 baseline 文档治理检查校验一致性；三平台 corpus/evidence 工具回归只在检查器实现、fixture、Git 路径配置、scope 配置或平台 workflow 变化时触发，数据更新不单独触发三平台回归。

<a id="ci-generated-inputs-design"></a>
### 生成文件与纯文档选择

可信 scope 配置可登记已审读的 `OUT_DIR` 生成文件，逐项绑定 Cargo package、实际 build producer 的仓库路径、producer Git blob 内容的 SHA-256 和固定输出文件名。选择器在每个参与分析的 Git revision 上独立验证 package、manifest 的 build 入口及 producer 摘要，只将完整匹配的 `include_str!` / `include_bytes!` 生成输出计为已解析；不执行候选构建脚本，也不按 `OUT_DIR` 名称统一豁免。

登记适用于已确认输入全部受既有共享构建/CI 控制规则覆盖的独立生成脚本。`oasis7_chain_runtime` 的三份 native 构建元数据分别来自构建环境、编译器版本和根目录 `Cargo.lock`，其 producer、manifest、锁文件、工具链及 CI 配置仍按原规则选择保守全量。producer 内容或 build 入口变化、输出未登记、路径逃逸及其他动态表达式继续保留未解析回退；更新生成器时须重新核对实际输入再更新登记，不能沿用失效摘要。真正由 Rust 嵌入的文档仍按 reader 及反向消费者选择测试；无关说明文档和 corpus 数据仅保留各自已登记检查。

回归覆盖纯说明文档、corpus 数据的逐路径及组合变化，并同时包含已登记生成输出、producer 在 base/source/tested revision 间漂移、未知动态嵌入、真实文档消费者和共享控制输入。还需用当前主仓依赖图重放文档变更，避免仅有微型仓库 fixture 或代码与 README 混合变更掩盖误选。

<a id="ci-unknown-inputs-design"></a>
### 未登记路径的失败链路

Trace refs: O7-REQ-4e7f2e0d-9147-43d9-a593-00a8db9b21df；承接[未登记路径门禁](ci-tiered-execution.prd.md#ci-unknown-inputs)。

选择器完成 Git/Cargo 与显式规则匹配后收集全部未登记路径，抛出独立的 `UnknownInputsError`，不被现有“输入不确定则全量”的捕获分支吞掉。CLI 返回非零并以转义 JSON 路径列表报告补登记提示，不写成功 plan 或 GitHub 组输出；共享控制输入视为已登记的全量规则，不能因没有 Cargo owner 误判为未知。混合变更中的已知全量输入及 `--add-group` 均不能覆盖未登记路径失败。

所有重任务沿既有 `select` 成功条件启动；轻量 formal 汇总和最终 `required-gate` 保留 `always`。最终 gate 在下载规划产物前先确认 `select` 成功，失败时立即给出范围规划诊断，因此不依赖失败路径上未上传的 artifact。trusted baseline 来源保持不变；新独立目录先通过规则/验证入口登记 PR，再提交使用该目录的功能 PR。新增测试使用真实 Git diff/CLI 验证未知新增、删除、重命名两端、混合全量输入、无成功输出及失败汇总；已登记路径、共享输入和图解析全量回退保留正例。

## 调度和结果

select 先产生明确的非空逻辑组列表，按资源组并行执行。普通组使用原生 job，fleet-health 使用固定 Linux/Windows/macOS 矩阵。最终唯一 `required-gate` 使用可靠的 always 汇总，并显式依赖 select 和所有组。选中组必须 success；未选中组可以 skipped 或 success；失败、取消、缺失、未知及意外 skipped 都必须阻断。fleet 矩阵不吞退出码，采用 fail-fast=false。

记录 source HEAD、base 和实际测试对象及运行链接。普通 PR 权限只读，从同一事件 BASE 提取可信选择器、scope 配置与结果汇总器，候选 checkout 的 driver 执行真实测试；候选控制变化不能缩小基线覆盖。CI 控制、安全或兼容边界变化接受对应能力的独立评审。

## 专项条件

缺测试先补普通 CI。只有普通 runner 难以提供的真实长期状态、专用硬件或受控环境才使用专项验证，说明具体不足、环境、版本、通过标准和实际结果。阻塞专项纳入同一 PR 最终汇总；调试 dispatch 不能替代 PR 检查。

CI 结果对应实际 source HEAD。main 前进或分支显示 `BEHIND` 本身不使该结果失效；只有合并冲突或 main 的相关变化带来明确集成风险时，才按需更新分支并验证实际组合。source HEAD 变化后重新运行必要普通 CI，复审实际变化与受影响部分；具体合入规则遵循开发流程规范。产品层的签名、checkpoint 和状态一致性证明仍按专业合同验证。

CodeQL 技术扫描见 [CodeQL 设计](codeql-integration.design.md)，实际 required/advisory 按 GitHub CI 与生效保护执行。

<a id="ci-coverage-design"></a>
## 测试覆盖专题的共享设计

本节承接 [coverage PRD](ci-test-coverage.prd.md#coverage-requirements)，不替代它的独立需求，
也不新增 CI 配置。由本设计的选测、调度和结果链承载离线回放、wasmtime 及 full 深度回归。

- **输入到执行**：从同一候选的实际 driver、features/target 与选中 job 核对覆盖清单；
  离线回放保持 snapshot/journal 输入与 HelloAck/Snapshot/Event 消息断言，端口隔离和超时不能省略。
  full 保留适用 required 和原有重型消费者，不以总用例数代替具体功能被执行。
- **失败与发布影响**：区分清单漏项、消费者未调用、命令非零、超时/端口冲突、环境缺失与有效不适用。
  必要链路缺失或失败沿原 suite/job/gate 传播；一次后续成功不能覆盖原失败。
  发布影响根据实际触发的适用门禁说明，不把文档存在当成 coverage 已交付。
- **历史目标**：PRD 中无 wgpu 的 egui_kittest snapshot 降级仍是历史/目标；未有可运行实现及匹配 gate
  证据前不得计入当前覆盖。未来实现须同时验证可诊断降级与有图形能力环境的真实快照，不能把所有 skip 算 PASS。
- **验证**：按 [测试手册](../../../testing-manual.md) 对同候选 required/full 的实际命令、参数、
  选中 case 与结果逐项核对；选择器/结果函数回归仅证明其自身边界，真实离线回放与 wasmtime 执行结果另记。
  清单故意漏项、失败命令和缺环境的负例应能阻断适用结论；本次文档合并没有运行这些场景。

<a id="ci-case-tier-design"></a>
## Test case 分级专题的共享设计

本节承接 [testcase-tiering PRD](ci-testcase-tiering.prd.md#case-tier-requirements)。该 PRD 继续
拥有 `test_tier_required` / `test_tier_full` 的标签语义；命令级触发、影响选择和汇总由本设计及通用规范拥有。

- **分配与选择**：新增或迁移用例时核对 cfg/feature 标签、Cargo feature 闭包、driver 参数与实际选中列表。
  标签存在但 runner 未执行不是覆盖；同名 case 在不同 target/features 下不能互相代签。
  required 选择足以检出受影响关键缺陷的用例，full 保留适用 required 及重型回归。
- **退化和冲突**：错标、漏标、full 回归被移除、预期用例未执行、旧硬编码与 feature 过滤重复都应可定位。
  先澄清对应需求与消费者再修正分配，不能为时延数字删断言、吞失败或把零匹配当通过。
  影响不明时沿现有选测规则扩大普通 required，不新设任务身份或通用审批。
- **性能与回归**：保持缺陷对应的 case、失败原因、实际版本、参数与结果可追溯；用可比较环境的实际运行
  评估反馈时间。文档压缩、feature 名称存在或历史绿色结果都不证明提速或 full 覆盖保持。
- **验证**：使用 [测试手册](../../../testing-manual.md) 和实际 CI driver 核对标签→参数→执行结果；
  漏标、错误 features/target、未选中、重复与失败传播分别构造最小适用检查。
  [选择器回归](../../../scripts/plan-rust-required-scope.test.py)与
  [结果函数回归](../../../scripts/ci-required-result.test.py)不代替真实 case 执行。

两专题共用以上实现与验证分配，不重构整个平台、不修改业务断言、不引入自动 case 学习。
旧同名设计路径暂留兼容跳转；删除必须先核清动态/外部分发、索引、配对与 corpus 消费者。

## 2. 上游与本设计

### 2.1 需求承接与分配表

| 上游要求 | 分配内容 | 本设计 | 负责范围 | 限制 |
| --- | --- | --- | --- | --- |
| [Coverage requirements](ci-test-coverage.prd.md#coverage-requirements) | 离线协议、full/wasmtime、降级目标及失败范围 | [覆盖专题](#ci-coverage-design) | CI 维护者与 QA | 不将未接线 snapshot 或文档合并计为覆盖 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [Case-tier requirements](ci-testcase-tiering.prd.md#case-tier-requirements) | 独立标签语义到实际选中 case 与结果 | [Case 分级](#ci-case-tier-design) | 测试维护者与 CI driver | 不修改断言或扩展任务准入 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [测试范围](ci-tiered-execution.prd.md#测试范围) | 保留产品测试并按真实影响选择 | [测试范围](#测试范围) | 选择器与执行器 | 解析不确定扩大普通 required，未登记路径另按门禁阻断 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| Trace refs: O7-REQ-4e7f2e0d-9147-43d9-a593-00a8db9b21df；[未登记路径门禁](ci-tiered-execution.prd.md#ci-unknown-inputs) | 所有未登记路径阻断规划，失败到最终 required 汇总，保持可信规则来源 | [失败链路](#ci-unknown-inputs-design) | 选择器、Rust workflow 与结果回归 | 产品适用性：N/A；reason=CI 范围登记治理，不改变游戏行为；scope=未登记路径的规划失败、可信基线与 required 结果传播；本地失败回归不代签 hosted 运行。 |
| [调度和结果](ci-tiered-execution.prd.md#调度和结果) | 最终汇总真实结果 | [调度和结果](#调度和结果) | workflow 与结果函数 | 本地回归不证明 hosted 运行成功 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [专项条件](ci-tiered-execution.prd.md#专项条件) | 普通 CI 优先，专项需具体环境不足 | [专项条件](#专项条件) | 对应产品运行场景 | 不新增通用 PM 集成链 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [Release Trunk](ci-tiered-execution.prd.md#ci-install-trunk) | Linux x86_64 冷热路径共用摘要与版本验证的预编译安装器 | [安装流程](#ci-install-trunk-design) | Release workflow 与共享安装器 | 工具成功不等于 Web 构建或发布成功 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [缓存复用](ci-tiered-execution.prd.md#ci-install-cache-evidence) | 对齐归档身份、可信 main writer 与只读消费者，验证真实 scope | [缓存边界](#ci-install-cache-design) | GitHub cache 与 main full-regression | 静态 key 相同不证明跨 tag 恢复 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [apt 预算](ci-tiered-execution.prd.md#ci-install-apt-budget) | 先量测；需要治理时约束总预算、有限重试及受控子进程清理 | [apt 评估](#ci-install-apt-design) | 临时 CI runner 的依赖准备 | 本次不实现未经数据确认的 apt 策略 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [量测与判定](ci-tiered-execution.prd.md#ci-install-measurement) | 分阶段、分冷暖记录可比较的准备成本与失败样本 | [耗时证据](#ci-install-measurement-design) | 实际 Actions 运行与维护者分析 | 不由 Cargo 指标或本地 fixture 推断发布提速 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [真实覆盖](ci-tiered-execution.prd.md#ci-real-coverage) | full 包含适用 required；真实消费者、wrapper 结果和扫描证据可核对 | [执行清单与结果](#ci-real-coverage-design) | CI driver、构建 wrapper、QA；CodeQL 保持独立主责 | 不把 fake 工具、旧运行或文档当成实际执行 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [RustSec 基线](ci-tiered-execution.prd.md#ci-advisory-baseline) | 完整 TOML 集合、真实日期、范围棘轮和逐项有界续验 | [例外检查](#ci-advisory-baseline-design) | repository_health_engineer 与安全/兼容审读 | 不修改当前有效期、批准列表或依赖范围 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [共享产物](ci-tiered-execution.prd.md#ci-formal-process) | 普通/重型分离、一次构建、五分片及失败汇总 | [进程调度](#ci-formal-process-design) | CI 构建与原生进程运行器 | 产品 N/A；理由：CI 专业调度优化；范围：普通检查与真实进程验收，不改变玩家行为。 |
| Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7；[共享产物中的缓存容量](ci-tiered-execution.prd.md#ci-formal-process) | 单份 raw 准备预算与远端压缩总量分离，可信写后按族回收 | [容量治理](#ci-cache-budget-design) | 正式 producer、缓存管理 helper 与 CI 维护者 | 产品 N/A；理由：已有共享产物要求的缓存资源约束；范围：可重建缓存，不改变必要测试、发布身份或持久世界。 |

## 3. 依赖准备的基线与目标

2026-10-12 审读基线是 `fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e`。本次采用 Release 复用现有安装器的最小改造；真实 hosted 构建、main 缓存写入、跨 tag 恢复及节省时间分别以实际运行记录确认。apt 保留观察与条件实施要求。

| 对象 | 审读基线事实 | 本设计的变化与限制 |
| --- | --- | --- |
| Release Web 工具准备 | [原工作流](https://github.com/eng-cc/oasis7/blob/fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e/.github/workflows/release-packages.yml)恢复 `~/.cargo/bin/trunk`，未命中时 `cargo install trunk`。 | 恢复预编译归档并无条件调用共享验证安装器，去掉重复源码编译。 |
| 普通 CI 与 main writer | [Rust 工作流](https://github.com/eng-cc/oasis7/blob/fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e/.github/workflows/rust.yml)已使用归档；main schedule/manual-full 的 `full-regression` 可写入。 | 复用已有 writer、路径与 key，不新增预热工作流。 |
| 安装器 | [原安装器](https://github.com/eng-cc/oasis7/blob/fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e/scripts/install-ci-trunk.sh)固定 Trunk 0.21.14、官方归档摘要及 Linux x86_64。 | 继续由该脚本集中拥有版本、来源、摘要、平台与失败规则。 |
| apt | 原工作流直接执行 `apt-get update/install`，本轮没有当前卡死或遗留进程的运行证据。 | 先核实阶段耗时与失败，再决定预算和重试参数。 |

<a id="ci-install-trunk-design"></a>
## 4. Release / Testnet Trunk 安装流程

消费者是 [Release `build-web-dist`](../../../.github/workflows/release-packages.yml) 和 [Testnet `build-web-dist`](../../../.github/workflows/testnet-packages.yml)，支持范围是现有 Linux x86_64 runner。源码身份继续由 `package-source-plan.py` 一次解析，job checkout 固定的计划提交并核对 HEAD；工具优化不改变原有源码、BUILDINFO、构建验证或发布时 tag 复核规则，完整边界见[包来源与缓存安全规则](../../engineering/workflow/package-source-security.md)。

1. 在计划源码校验后确认 `scripts/install-ci-trunk.sh` 是非空普通文件且不是符号链接；缺失、为空或为符号链接时明确失败，不能用空安装器 hash 恢复缓存，也不从其他 ref 临时借用脚本。
2. 使用固定提交的 `actions/cache/restore` 恢复同平台归档。该步骤只恢复，不为 Release / Testnet 增加 Trunk 缓存保存；不把缓存命中作为跳过安装器的条件。
3. 调用 `bash scripts/install-ci-trunk.sh "$RUNNER_TEMP/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz" "$RUNNER_TEMP/ci-tools/bin"`。归档不存在时，安装器从固定官方来源下载并校验后落盘；归档已存在时仍校验固定 SHA-256，校验通过后才解压及执行。
4. 安装器检查提取的 binary、精确版本和支持平台，成功后写入 `GITHUB_PATH`，后续原有 Web 构建使用该工具。任何必要安装失败沿 job 失败传播。

缓存损坏或下载摘要不符时在解压、执行前失败；版本不符、网络失败、提取失败和不支持平台均不得继续构建。保留现有安装器的有界下载重试，不引入任意版本、未审核镜像或 `cargo install` 回退。损坏缓存不静默改写成一次成功的冷启动。

<a id="ci-install-cache-design"></a>
## 5. 缓存身份、作用域与恢复

归档路径与普通 CI/main writer 保持一致：`${{ runner.temp }}/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz`。key 复用现有表达式：

```yaml
trunk-archive-${{ runner.os }}-${{ runner.arch }}-${{ env.TRUNK_VERSION }}-${{ hashFiles('scripts/install-ci-trunk.sh') }}
```

安装器来自已核验的计划 source；key 中的版本、平台、架构和安装器内容指纹共同描述本次工具输入。升级脚本或 pin 会产生正常冷启动。缓存路径、归档格式及 cache action 的兼容实现也必须一致；仅对齐 key 字符串不充分。GitHub 的 cache version 包含路径与压缩方式，tag 间不能直接读取彼此缓存，可恢复默认分支范围内的缓存，见 [GitHub 缓存引用](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)。

现有 main `full-regression` 的 schedule/manual-full 是共享归档 writer；Release / Testnet 使用只恢复 action，普通 PR 继续受只读 cache token 约束。Rust 依赖按下述生产者与消费者关系复用，不增加宽泛 `restore-keys`、扩大缓存写权限，或把缓存内容当作可执行来源的认证。

普通 Rust CI 与 Release / Mainnet 的 runtime core、support、sync、soak 读取 `ci-shared-native-trusted-v3`。唯一 producer 是 main schedule/manual-full 的 `full-regression`；这些 Release / Mainnet gate 仅有只读 cache token，所有消费者显式禁止保存。GitHub cache 是不可变归档，同 key 的并行保存不会合并各自 target，因此由 producer 在同一根 target 内形成依赖并集，完整成功后保存一次，禁止失败保存。新命名空间使旧的不完整快照不能成为精确命中；后续改变生产覆盖时必须更新族版本，不使用每个 source SHA 或 run ID 给大依赖包换代。

producer 的 `full-standard` 已实际构建 full-core 和大部分 support 依赖。额外的 build-only 准备补齐 ordinary producer 的真实构建变体（下载的测试 binary 不算 target 预热）、support 的纯 `wasmtime --lib --bins --no-run`、默认 features 的 chain runtime，以及 sync 使用的 `hydrate_builtin_wasm` 和带 `sync-builtin-wasm-identity` 的 host helper。复用现有构建参数定义，任何补齐失败均使 job 失败且不保存。各 reader 继续原测试、sync 和 soak；没有以预热代替验收。builtin 模块的独立 workspace / Docker target 不属于根 target 缓存，仍走 canonical 构建，不为扩大归档改写其目标目录。

生产者与消费者在缓存步骤前对齐实际已安装的 Rust/WASM 工具链、Cargo 配置及参与 key 的环境值；`shared-key` 相同本身不足以消除工具链或环境 hash 的差异。共享 native 族固定 stable 与 builtin nightly，保留显式 `CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM` 指纹。准备 helper 只在 GitHub-hosted 临时 Linux runner 上工作，验证所需的精确工具链与当前 active 工具链后，移除无关预装工具链；缺失、非固定版本或卸载失败均失败，不关闭环境 hash。Release / Mainnet native gate 在 job 范围对齐 Cargo 重试、registry 协议、debug profile 和四项 WASM 环境。NewAPI 两个入口仍保持各自对齐的身份。

registry 下载内容与 target 编译产物分别核对复用价值。当前固定的 `rust-cache` 将两者一并归档；合并 native 族减少重复库存，但各小 gate 会恢复更大的归档，恢复/解压成本与编译收益必须由实际 hosted 数据分别评估。缓存可能被驱逐，保存后的 Cargo 清理也可能丢弃不活跃依赖；缺少产物仍由原命令补齐，不能承诺全部 fresh。不同 target/profile 可以共存；此版保留 Web 发布、ECS `ecs` profile 与 Formal workspace 的专用生产路径，以免这些构建的发布频率或源码恢复合同绑到主缓存。

Testnet / Mainnet / ECS Web 与 Release Web gate 只读取 `release-packages-web-wasm-trusted-v3`；唯一 writer 是 Release `build-web-dist` 的成功 main 构建，计划源码和 workflow SHA 必须同时等于 event SHA。Web cache 前仅保留固定 stable，ECS Web 在 job 范围补齐相同 Cargo 环境。ECS 原生独立迁移到 `oasis7-ecs-native-linux-ecs-v2`，仍只允许既有 main 可信 lane 成功保存。计划源码缺少新工具链 helper 的历史包跳过这一 Rust 缓存路径，继续原冷构建；不能把缺失 helper 的空指纹作为新族写入。

Linux 原生发行包仍按工具链、target 和 profile 共享 `oasis7-package-native-…-trusted-v2`；Testnet 的 Linux-only packaging-profile main workflow dispatch 仅在计划源码和 workflow revision 均等于 event SHA 时选择独立的 write-scoped 原生作业。其他分支、tag、旧 SHA 或 workflow/source 不同版本均只读，两条路径互斥，不重复编译。非 Linux 原生包仅在 OS、target 和 profile 匹配时读取对应 Release 原生包缓存；Storage / Compile 的独立边界不扩展。core/support 的文档 gate 使用计划提交父子范围和完整历史；必要命令通过 `tee` 输出日志时启用 Bash `pipefail`，上游失败必须使 gate 失败。

缓存是可回收的加速数据，没有发布状态、事务或持久化权威。main 尚未保存该身份、缓存被回收或 reader 不可达时，正确的冷下载仍可完成工具准备。恢复服务异常须保留实际 action 结果；若 action 允许继续且归档不存在，安装器仍执行同一校验冷路径，不用忽略必要安装失败来维持 job 绿色。

跨 tag 复用的验证必须关联一个可确认的 main 保存记录及两个不同 tag 的实际 reader，记录 run/job、event/ref、请求和实际匹配 key、平台与 cache version 兼容条件、恢复及摘要验证结果。仅同 tag rerun 只能证明同 tag 恢复；未取得上述记录时保留“配置已接通、真实跨 tag 恢复待观察”。沿用自然发生且已授权的 main/full 与 Release 执行积累数据，不为本项新建发布事件或 required check。

<a id="ci-cache-budget-design"></a>
### 5.1 Workspace 缓存容量、滚动保留与已迁移命名空间的退役

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7；承接[共享产物要求](ci-tiered-execution.prd.md#ci-formal-process)中的[正式缓存容量与保留](ci-tiered-execution.prd.md#ci-formal-cache-budget)。

单份 staging 上限只约束准备上传的 workspace 文件原始逻辑字节；ordinary 为 2 GiB，Formal 为 8 GiB，ECS 为 6 GiB。三个 workspace writer 共用完整库存、实时额度与已核验配置预算的容量准入，以及同一预压缩归档和外包装上传上界计算；它们分别受 1 GiB、2 GiB、6 GiB 远端族预算约束，均以两代为维护目标。族预算不是相互独立的仓库额度，所有 ref 和其他族的库存也计入同一个仓库容量检查。[缓存维护 helper](../../../scripts/ci-cache-maintenance.py) 使用 GitHub cache API 的 `size_in_bytes`，按缓存 ID 校验唯一性，以缓存族和 ref 汇总条目数与压缩字节；相同 key 的不同 cache version 或 ref 仍是独立条目。兼容组由去除 source SHA 的完整 key 前缀与 cache version 共同确定，用于代数及恢复点判断。GitHub 的默认仓库额度与驱逐规则见[官方缓存引用](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy)，它不替代本仓库对受管族的预算，也不保证旧缓存能保留到下一次复用。

workspace 受管范围限于 `refs/heads/main`：当前格式为 `{ordinary-workspace-v1,formal-workspace-v3,ecs-workspace-v2}-{64hex compatibility}-{40hex source SHA}`。Formal 历史 `formal-workspace-v2-ubuntu24-{arch}-{rusttoolchain}-{wasmtoolchain}-{64hex compatibility}-{40hex source SHA}` 与 ECS v1 的完整格式仍计入各自族预算，只有该族当前条目确认后才能按旧兼容组回收。Formal v3 改用单文件压缩归档，绝不把旧 v2 目录当作新 payload；identity 包含平台和完整工具链配置。族预算覆盖所有格式、兼容指纹与 source SHA。source SHA 继续进入不可变精确 key；兼容前缀保持平台、工具链、Cargo 输入、features/profile 与构建脚本边界，不扩大恢复范围。helper 集中维护族级压缩字节预算、当前兼容组的保留代数及 API 总预算；调用方显式提供当前 key，不能传入任意前缀扩大清理范围。

额外的退役白名单只接受完整 `v0-rust-{family}-Linux-x64-{8hex environment}-{8hex dependencies}` 与 main ref：旧 `ci-full-regression-trusted-v2` 及四个 `release-packages-release-gate-{runtime-core,runtime-support,runtime-sync,soak}-trusted-v2` 由 `ci-shared-native-trusted-v3` 替代；旧 Web `release-packages-web-wasm-trusted-v2` 由同名 v3 替代；ECS `oasis7-ecs-native-linux-{packaging,ecs}-v1` 由 `oasis7-ecs-native-linux-ecs-v2` 替代。只有对应替代族存在非零、合法 main 条目且两次完整清点中身份不变，才允许退役该旧命名空间。替代族所有条目均保留，不按最新时间推断其中的旧环境 hash 可删；近似命名、其他 ref 和其他族只统计。

容量允许时，writer 可先保存临时第三代，再由维护回到两代；禁止先删恢复点来腾出保存空间。清理在可信 main producer 的必要构建、产物上传及保存或精确命中核验成功后进入独立维护 job。Rust 维护等待普通、Formal 和本轮 full producer 全部结束，包括依赖缓存 post-job 保存；各族只使用成功 producer 输出并已确认的精确 current key。ECS 维护等待可信 native producer 成功，不依赖 Web 结果。两条 workflow 共用固定 `ci-cache-maintenance-main` concurrency 组、`cancel-in-progress: false` 与 `cache-mode: none`，不恢复或执行缓存 payload。先通过 REST 查询当前 `GITHUB_RUN_ID` 并核对仓库、source 与可信运行身份，以该 run 的 `created_at` 作为并发保护起点；不能使用客户端时间或当前缓存落盘时间。

保存 action 成功或构建后的精确命中只决定是否进入核验，精确 key 在同 ref 的完整库存中须对应唯一、合法且非零字节的条目，才视为保存已确认。预留竞争可能只产生 warning；若并发 writer 已保存同一精确条目且 API 核验通过，可以维护。保存未确认、当前 key 缺失、同 key 跨 version 多条或 API 结果不完整时跳过删除。

保护集包括当前精确条目、同兼容组中创建时间早于当前条目的最新一个完整恢复点、零字节条目，以及保护起点之后新建或被访问的全部受管条目。起点取当前可信 run 与清点中相关未完成 workflow runs 的最早 `created_at`；活跃运行也须完整、有界查询，查询不完整时不删除。这样较旧 SHA 的慢构建不能删除清点中可见的较新 SHA 已提前保存的结果，已观察到的运行期间访问和未完成上传进入保护集。

先保留保护集并计入族总字节；每个具有可信 current key 的族中，非当前兼容组的未保护条目直接进入删除计划，当前兼容组的其他条目再按访问/创建时间降序受代数和字节目标约束。明确退役的旧 Rust 命名空间在替代核验后按同一活跃/零字节保护筛选，不占用 workspace 各族预算。保护集超额时报告实际超额，不能扩大删除范围；这些数值是回收目标，不是并发条件下的硬上限。

先完整读取分页并核对缓存 ID、key、version、ref、字节及时间字段，再生成删除计划；执行前再次完整清点缓存和活跃 runs，重算保护集和计划。只有两次计划都选中、且 ID、key、ref、version、创建时间和字节数保持相同的条目才能删除，第二次计划只能缩小首次删除集合。当前精确条目的不可变身份变化使整轮跳过；替代项变化或消失使对应退役规则停止。后页失败、重复或冲突数据、分页未结束、字段不合法均不得把部分结果用于删除。

删除按准确 cache ID 从创建时间较旧的条目开始，保留计划、实际结果与未执行部分；404 视为该条目已不存在，其他删除错误停止本轮回收并报告。清理和查询使用有限的请求与总时长预算，不进行无限重试；API 故障不使必要构建失败，也不能把原构建失败改成成功。维护入口仅在可信 producer 成功路径取得最小的 Actions 删除权限，PR 与其他只读消费者不继承。

清理后的容量须由再次查询得到；无法回读时仅报告已确认删除的条目和未知剩余量。两次清点不构成事务锁或缓存租约，之后的访问和平台驱逐仍可能发生；消费者继续依赖正确冷路径恢复。存储报告与本地 raw/去重体积、下载传输量、实际 Cargo fresh/重编数分列，不从命中次数反推省下的存储或运行时间。旧的未受管缓存仍由平台驱逐或单独核实后处理，本策略不承诺全仓即刻低于默认额度。

<a id="ci-cache-budget-validation"></a>
[容量治理回归](../../../scripts/ci-cache-maintenance.test.py)使用多页 API fixture 验证族/ref/version 归属、两类预算、旧 SHA 迟于新 SHA 完成时按 run 起点保护、近期访问与零字节保护、两次计划交集和身份变化、未知项保留、后页失败零删除、部分删除失败及重复执行；[workflow 接线回归](../../../scripts/ci-workflow.test.py)核对保存前不能清理、保存未确认不能清理、候选没有删除权限、固定维护并发及治理错误不覆盖必要 CI 结果。[发布缓存回归](../../../scripts/package-workflow-cache-reuse-contract.test.sh)核对实际工具链/环境 hash 输入与 writer/reader 成对关系，禁止仅验证 key 文本相同。真实 hosted 保存、删除后库存与冷热构建收益仍分别在实际 PR/运行记录中验收。

<a id="ci-install-apt-design"></a>
## 6. apt 的有界安装评估

先从现有 job 日志区分索引更新、下载、安装与锁等待，记录 runner image、阶段耗时、失败类别及实际重试；日志粒度不足时由维护者在对应依赖步骤补量测。当前没有依据直接规定统一超时秒数、尝试次数或重试所有错误。

若观察到足以实施的长尾或取消清理问题，采用一个可单测的 helper 管理整个依赖准备预算：每次执行、终止宽限、退避与后续尝试都计入有限总预算。仅对可确认的瞬态失败重试；缺包、错误参数、权限/配置、签名或完整性失败不靠重复调用掩盖。

超时与取消针对本次启动的受控进程组，先请求终止，必要时在有界宽限后强制终止；确认该组及其受控子进程退出后才可结束或开始下一次允许的尝试。不得删除锁文件、终止无关进程，或把父 shell 退出当作清理完成。若 dpkg 已开始修改且被强制中断，不能盲目重复 install；无法证明状态可继续时让 job 失败，由新 runner 重试。

该后续设计仅面向临时 CI runner，不直接推广到开发机器或部署主机。实现时用假命令与进程 fixture 验证挂起、忽略终止信号、父进程先退出、瞬态/永久错误、预算耗尽及取消，不对真实 apt/dpkg 故障注入。未实施或未执行的场景不得写为已验证。

<a id="ci-install-measurement-design"></a>
## 7. 耗时与运行证据

维护者从实际 Actions 日志分别记录缓存恢复、官方下载、摘要校验/提取/安装/版本校验及总准备耗时；writer 的保存成本另列。当前日志可提供 action/step 级时间，缺少子阶段时间时标明粒度，不把合并步骤的时间伪拆成精确阶段。需要更细比较时再在同一安装路径补计时。

| 样本类型 | 判定依据 | 记录边界 |
| --- | --- | --- |
| 冷启动 | 没有恢复归档且本地归档不存在，实际执行下载 | 包括恢复尝试、下载与验证安装 |
| 暖缓存 | 从 GitHub 恢复对应归档并通过校验，未调用下载 | 包括恢复及验证安装，不跳过校验成本 |
| 本地已有 | 同 job 已存在归档且验证通过 | 单独列出，不充当跨 run 命中 |
| 恢复异常后下载 | action 异常后仍能按冷路径取得正确归档 | 单列异常与额外成本，不混入正常冷启动 |

每组记录 runner OS/image、架构、Trunk 版本及归档摘要、安装器指纹、source、event/ref、run/job 链接、key、样本窗口与数量。失败和取消保留原因及耗时，不能仅挑成功样本。总发布耗时、排队时间、Rust 编译与 Trunk 准备时间分别报告；Cargo cold/no-op 量测不覆盖本节的网络安装与 cache scope。

样本不足时报告单次观测和环境差异；至少三次可比观察后可以报告样本数、中位数及范围，仍不外推稳定性能承诺。当前不设仓库统一 SLO、预写提速比例或新增性能门禁。实现接线正确、本地合同通过、hosted 构建成功与真实缓存收益是分别验证的结论。

## 8. 兼容、启用与回退

实现随普通 PR 合入生效，实际 Release 使用哪个 workflow revision 和 source commit 由原有计划合同确定。计划源必须包含受支持的共享安装器；早于该安装器的历史 tag 在新 workflow 下会明确失败。需要重发历史版本时，先审读相应源码和工具兼容性，不能用移动 tag、任意 ref 脚本或源码安装回退绕过这一边界。

缓存内容无需迁移；旧 Release binary 缓存可以自然回收，首次使用新身份的冷成本单独记录。回退通过正常 PR 恢复已知工作流基线及配套合同测试，仍须独立评审和适用 CI；不在运行中自动选择另一种安装策略。apt 的条件设计不改变当前运行参数。此改造不涉及玩家接口、世界状态或游戏 Runtime，无需 Agent 游戏实玩验证。

<a id="ci-real-coverage-design"></a>
## 9. 执行清单、真实消费者与结果

本节承接[真实覆盖合同](ci-tiered-execution.prd.md#ci-real-coverage)，是后续实现要求；不将清单设计、已有工具接线或历史绿色结果视为全部覆盖已交付。

### 9.1 required 到 full 的可校验包含关系

对固定 source 和环境/平台/features/target/影响范围，生成适用 required 清单 R 与 full 清单 F，验证 R 是 F 的子集。清单来自同一检查注册表或实际执行入口的可核验导出，不能维护一份与执行代码无关的手写“已覆盖”清单。每项至少能核对检查身份、命令与参数、前置依赖、平台/feature 适用条件、退出结果；同名但运行不同 feature/target 的命令不能互相替代。

full 可以按依赖图合并重复构建、并行独立测试，但不删除 Clippy 或独立 Viewer browser smoke 的实际调用。browser smoke 之前必须完成其所需依赖和真实构建，失败不执行使用陈旧产物的验证。单个命令失败沿原 suite/job/required-gate 传播；总测试数、成功构建或后续命令的零退出码不能覆盖它。可信 base 选择器与候选控制输入不缩小基线覆盖的规则保持不变。

对不支持的环境明确标记不适用并指出实际受支持入口；缺工具、缺 target、未执行或无法判定列为阻塞/失败/未知，不能装成不适用或通过。回归故意删去 full 中一项 required、修改其 feature/target 和令各命令返回非零，确认包含关系和失败传播均生效。这些回归进入既有检查，不新增身份门禁或 PM 复验链。

### 9.2 WASM 与 wrapper 分层证明

维护实际 native/WASM 消费者到 stable smoke、确定性构建入口、输入/产物及 hash 核验的映射。fake Docker 只检验 build wrapper 传参和错误传播；真实 builtin 容器构建与 hash 比较独立执行。`build-wasm-module.test.sh` 有测试文件不等于被 CI 调用，应能从实际选中 job 追到该命令和结果。缺必要 target 或把所有 lane 改为 `BUILD_STD=0` 不能压掉真实消费者；明确已退役且无消费者的旧 lane 不重新激活。

构建 wrapper 在启动子命令前冻结一个有效 target-dir，并区分 wrapper/Cargo 参数与 `--` 后的应用参数。保留含空格参数及 sccache 开/关、环境与显式参数的既有边界；冲突输入统一解析或明确拒绝。退出清理使用保存的 child exit status，释放自己持有的 lease；仅更新仍存在且归属有效的元数据，不能在 target 被合法删除后重新 mkdir 或用记账错误遮蔽原退出结果。不得以保退出码为由遗留占用/GC 锁。

原生 Windows 用两个真实进程验证超过平台默认短重试窗口的锁等待、释放后只执行一次、等待可取消、非阻塞模式及非争用 I/O 错误立即失败。WSL、fake lock 或短等待仅证明各自条件；这些合同不授权清理其他 worktree 或未知缓存。

### 9.3 CodeQL 结果是独立证据链

沿用 [CodeQL 专项设计](codeql-integration.design.md) 的语言、查询、预算与启用决定。每个适用语言/查询组合分别核对执行终态、上传/analysis 绑定及 finding 处置，使用准确 source、ref、run、attempt、job、category 和 App。API 分页在总预算内完整读取；后页失败、重复游标或预算耗尽不发布完整结论。缓存结果与不匹配运行不能补齐缺失证据。

修复结果发现/绑定不意味着 CodeQL observe 已启用；扫描超时也不直接授权延长预算、删除查询或新增 required gate。专项执行、依赖兼容复核和 RustSec 到期处置独立推进，不让一个广泛 umbrella 的等待阻止有明确期限的安全维护。

<a id="ci-advisory-baseline-design"></a>
## 10. RustSec 语义解析与例外生命周期

本节承接[RustSec 基线要求](ci-tiered-execution.prd.md#ci-advisory-baseline)。当前值由 `deny.toml` 和 `scripts/rustsec-ignore-direct-dependency-baseline.json` 拥有；本设计不更新日期、advisory、依赖或批准范围。

检查流程为：读取配置并以标准 TOML 解析器取得完整 `advisories.ignore` -> 验证集合类型、每个 ID 和重复项 -> 从受支持的注释布局提取元数据并建立唯一对应 -> 对照批准集合和 direct manifest 基线 -> 日历日期及有效期校验 -> 执行原依赖路径验证 -> 通过或带定位信息失败。不能先从正则抓到的 ID 子集推断真实 ignore 集合。

TOML 中单引号字符串、引号表名和同行元素均以解析后语义为准；注释里的 ID 不进入启用集合。元数据仍可限制为现有清晰布局，但存在多个候选对应、同一元数据覆盖多个条目、缺/重复元数据或不支持布局时明确拒绝。解析成功与治理批准是两步：合法 TOML 中新增未批准 advisory 仍拒绝。集合相等检查不能先去重而丢掉重复条目诊断。

使用日期类型解析所有参与比较的日期，包括测试用 today；拒绝不存在的月/日及不合法闰日，保留现有到期日比较包含关系。`reviewed_at` 与期限先验证合法，再作原有策略检查，不以格式正则或字典序替代日历合法性。测试分别覆盖到期当天与次日，不把修复校验器当作续期。

每项例外到期前的处理记录须明确：当前依赖路径及批准 manifest 是否变化、是否有可行移除/替代、保留理由、负责 owner、实际验证和有界下次期限。仅更改日期、复制旧 cargo tree 或扩大 direct manifest 列表不构成续验；无法取得所需证据时保持未完成并按原检查结果处理。已删除例外不因模板、解析重写或历史测试样本重新进入生产 allowlist。

## 11. 验证

### 11.1 验证映射表

| 上游要求 | 本设计 | 验收内容 | 验证来源 | 实际记录 | 限制 |
| --- | --- | --- | --- | --- | --- |
| [Coverage requirements](ci-test-coverage.prd.md#coverage-requirements) | [覆盖专题](#ci-coverage-design) | 同候选/环境/features 下离线消息链、full 包含关系、失败及未接线目标 | [测试手册](../../../testing-manual.md)、实际 driver/选中 job；按本节选择正反例 | 实际命令、参数、case、退出与缺口；本次未执行 | 不由 selector fixture、skip 或旧 PASS 代签真实覆盖 |
| [Case-tier requirements](ci-testcase-tiering.prd.md#case-tier-requirements) | [Case 分级](#ci-case-tier-design) | 同候选标签到实际执行；漏标、错 feature、零匹配和失败传播 | [测试手册](../../../testing-manual.md)、实际 CI driver；结果函数回归 `scripts/ci-required-result.test.py` 仅作补充 | case/feature/target 与运行结果；本次未执行 | 无 hosted 或性能结论，不改 required/full 保护 |
| [测试范围](ci-tiered-execution.prd.md#测试范围) | [测试范围](#测试范围) | 删除、重命名、依赖和未知输入覆盖 | [选择器回归](../../../scripts/plan-rust-required-scope.test.py) | 命令退出码及输出 | 不替代真实产品组运行 |
| [未登记路径门禁](ci-tiered-execution.prd.md#ci-unknown-inputs) | [失败链路](#ci-unknown-inputs-design) | 未登记新增/删除/重命名、混合全量控制、候选豁免不能放行；无成功输出且 gate 明确失败 | [选择器回归](../../../scripts/plan-rust-required-scope.test.py) | 当前 PR 中保留选择器、工作流和结果回归的命令、source HEAD、退出码和 Actions 结果 | 本地 fixture 不证明 hosted 运行；Git/依赖解析不确定仍保留全量 |
| [调度和结果](ci-tiered-execution.prd.md#调度和结果) | [调度和结果](#调度和结果) | 失败、取消、缺失和跳过均正确传播 | [结果函数回归](../../../scripts/ci-required-result.test.py) | 回归输出与 Actions 链接 | hosted 图仍需独立核对 |
| [专项条件](ci-tiered-execution.prd.md#专项条件) | [专项条件](#专项条件) | 适用环境和真实场景执行 | [测试手册](../../../testing-manual.md) | 所测版本和实际环境结果 | 未执行的环境保持未证明 |
| [Release Trunk](ci-tiered-execution.prd.md#ci-install-trunk) | [安装流程](#ci-install-trunk-design) | planned source、只恢复归档、冷热均调用验证安装器、必要失败传播 | [Release 接线回归](../../../scripts/release-packages-trunk-cache-contract.test.sh) | 候选版本、命令退出码及回归输出 | 不替代真实下载、Web 构建或发布 |
| [缓存复用](ci-tiered-execution.prd.md#ci-install-cache-evidence) | [缓存边界](#ci-install-cache-design) | main writer 与不同 tag readers 的真实身份、恢复及验证 | [依赖安装核验](../../../testing-manual.md#ci-dependency-installation-validation) | writer/reader run/job 链接与实际 key | 未 seed 或未执行 reader 时仍未证明跨 tag 命中 |
| [apt 预算](ci-tiered-execution.prd.md#ci-install-apt-budget) | [apt 评估](#ci-install-apt-design) | 量测后选定参数；实施时验证总预算、分类重试、超时/取消无受控遗留进程 | [依赖安装核验](../../../testing-manual.md#ci-dependency-installation-validation) | 实际 apt 观察及后续 helper 的 fixture 输出 | 当前未实现，不推断宿主或部署机适用性 |
| [量测与判定](ci-tiered-execution.prd.md#ci-install-measurement) | [耗时证据](#ci-install-measurement-design) | 相同工具/平台下分冷暖计入准备与缓存成本，保留失败样本 | [依赖安装核验](../../../testing-manual.md#ci-dependency-installation-validation) | 原始耗时、样本窗口/数量和条件差异 | 不以本地测试宣称 hosted 性能收益 |
| [真实覆盖](ci-tiered-execution.prd.md#ci-real-coverage) | [执行清单与结果](#ci-real-coverage-design) | 漏项/错误传播、真实 WASM 消费者、wrapper 退出及 Windows 锁边界 | [测试手册](../../../testing-manual.md)；实际 CI driver、构建/wrapper 回归和原生平台运行 | source、计划清单、命令与真实结果 | fake 工具和计划存在不代签执行；CodeQL 启用另验 |
| [RustSec 基线](ci-tiered-execution.prd.md#ci-advisory-baseline) | [例外检查](#ci-advisory-baseline-design) | 完整 ignore、元数据对应、日历/期限与 direct manifest 漂移 | [基线回归](../../../scripts/check-rustsec-ignore-baseline.test.sh)及真实依赖查询 | 正反例、当前依赖路径与逐条例外处理 | 文档不续期；fixture 不证明实际依赖已移除 |
| [共享产物](ci-tiered-execution.prd.md#ci-formal-process) | [进程调度](#ci-formal-process-design) | 独立分片、精确清单、同源产物、旧基线扩大、错误与超时传播 | [分片核验](../../../testing-manual.md#ci-formal-process-validation) | 当前 PR 的真实 runner、测试与评审结果 | fixture 不代签完整产物消费；未完成 hosted 验证保持未证明 |
| [共享产物中的缓存容量](ci-tiered-execution.prd.md#ci-formal-process) | [容量治理](#ci-cache-budget-design) | 保存确认后回收、族/ref/version 与压缩预算、分页及并发/访问保护、失败不影响必要 CI | [容量治理回归](../../../scripts/ci-cache-maintenance.test.py) | 当前 PR 的回归输出、准确 source、hosted 保存/回收和库存记录 | 本地 fixture 不证明实际释放空间或缓存收益 |

## 12. 决策与残余风险

历史 [Issue #1808](https://github.com/eng-cc/oasis7/issues/1808) 中的旧 PM 工具问题随 [#4354](https://github.com/eng-cc/oasis7/pull/4354) 退出当前实现；本设计保留可适用于现行 CI 的依赖准备与缓存问题，不恢复旧任务或验收状态机。选择共享预编译安装器，是因为现有实现已集中拥有来源、摘要和版本检查，可以直接去掉 Release 的重复源码安装路径。

`repository_health_engineer` 维护安装器、缓存接线及 apt 观察；`qa_engineer` 协作核对真实运行证据。主要残余风险是外部下载服务可用性、main seed 尚未产生、缓存被回收以及旧 tag 缺少安装器；这些情况分别按既有失败语义或正确冷路径处理。版本、平台、来源策略或 cache action 更新时重新审读受影响合同；没有实际 hosted 数据时保留未知项，不从文档状态推导发布成功。

<a id="ci-formal-process-design"></a>
## 真实进程验收的边界与结构

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7

承接[共享产物要求](ci-tiered-execution.prd.md#ci-formal-process)。`ci-test-artifacts.py` 从 Cargo JSON 的 executable 字段收集可执行文件，并区分 formal、ordinary 与兼容本地入口的 all 构建范围。正式最小 producer 仅构建 required-feature 的 oasis7 library / chain-runtime binary harness、production runtime、release endpoint 及 default-feature distfs library harness；`oasis7-test-build` 保留普通 required 全包、Viewer 和 default-feature snapshot 的原变体。两个构建范围独立依赖 select，正式矩阵不等待普通 producer 完成。

正式 harness 的最小构建显式启用 `oasis7/ci_formal_required_features`，通过 native target 与 host build 的 optional dependency aliases 保留完整 required 根包共同构建时的依赖 feature 集合。该标记不参与业务 `cfg`，不自动启用，也不加入 production runtime、ordinary 或 default-feature snapshot 的原命令；依赖版本复用现有锁文件。验证比较完整构建与正式最小构建保留的完整依赖子树，并分别核对 host/build 与 target context 的实际 feature 组合，不能只比较根包上的 `test_tier_required` 字符串或把同包不同 context 合并后判断等价。

正式 producer 分为互斥的候选只读入口 `formal-test-build` 和可信 main 写缓存入口 `formal-test-build-trusted`，共享相同构建步骤，每轮恰好运行一个。可信入口仅用于默认 main 上的 push、schedule 或 manual-full；其他候选不能取得缓存写权限。矩阵和固定汇总显式依赖两个入口，选中时要求恰有一项 success 且另一项 skipped；任何失败、取消或未知状态均阻断，不能让 skipped 的隐式 needs 传播掩盖应运行的矩阵。

producer 核对完整清单后，为普通组及各物理分片输出独立 tar.gz，gzip level 1；保留 executable 权限，不传输整个 target。每包只带该消费者的 harness、生产子进程和动态库闭包，并报告原始字节、压缩字节和逐 binary 体积分布。consumer 核对原 run/source/tested identity、全局清单及所选闭包的 hash、profile 和实际 `--list`，不能因缺少不属于自己的二进制而补编。producer attempt 记录供审计；同 run、相同源码和构建身份的 consumer 重跑可复用先前 attempt 的产物。保持相同 checkout 布局，恢复编译时嵌入的 Cargo 程序路径，设置包工作目录、动态库路径及原二进制环境。普通集成测试需要的 Cargo binary 和 default-feature snapshot 不得被正式分包移除。

分片清单由 `scripts/ci-formal-process-suites.json` 拥有，实际名称以下载 harness 的 `--list`/`--ignored --list` 核对。清单中的旧过滤器用于检出重命名、遗漏和新增未分配验收；辅助入口单独登记，不运行整个 ignored 集合。

| 旧入口覆盖 | 新归属 |
| --- | --- |
| protected endpoint 协议、崩溃、effect、handoff、availability、survivor | runtime-protocol |
| protected endpoint 长历史 | world-history |
| formal healthy durable set、owner 故障 | healthy-set |
| distfs endpoint qualification、recovered children、C10 长记录 | storage-recovery |
| world initialization、full state sync、local-node-lifecycle、legacy migration | lifecycle-cli |
| oasis7 lib / runtime bin required-feature 的非 ignored formal | runtime-protocol，保留原二进制环境 |
| distfs default-feature lib 的非 ignored formal | storage-recovery，保留原二进制环境 |

表中归属保持为逻辑 suite。物理分片可将 healthy-set 或 runtime-protocol 的完整慢用例放到较轻的分片；执行摘要同时记录逻辑 suite 与物理 shard，并对全集检查恰好一次归属。world-history 的单个 70 次连续操作场景保持完整，不能以切断历史链消除增长成本。上述非 ignored formal 命令未证明与普通环境等价，继续执行；snapshot_progress 与 legacy fields 回填仍使用原 default features。独立 ignored 验收按 `--exact --ignored --test-threads=1 --nocapture` 串行运行，矩阵 `fail-fast: false`、`max-parallel: 5`。world-history job 上限 45 分钟，其他分片初始保护上限 90 分钟；每次子命令绝对上限 40 分钟，均为防挂保护而非性能目标，后续按真实基线调整。

正式 workspace 缓存使用官方 restore/save 的独立步骤。先恢复既有可信依赖缓存，再恢复本次正式构建实际使用的仓库内 path package 编译单位；不重复保存整个第三方依赖 target。构建从 Cargo 的 compiler-artifact 与 build-script-executed 消息确定具体产物 hash，保存对应 deps、完整 .fingerprint、build-script 编译与执行输出目录以及公共可执行文件。内容和纳秒时间戳原样保留，不改写 Cargo fingerprint 来强制命中。缓存目录位于 RUNNER_TEMP，恢复前校验绝对 checkout/target 身份、路径、文件类型、长度、权限和摘要，完整校验通过才叠加到依赖 target。

新 checkout 的源码时间戳会使相同内容的 workspace 产物失效。producer 因此记录构建前后完整源码文件清单，包含未跟踪和 ignored 的普通文件，排除 .git 与实际 Cargo target；只有始末路径、内容、权限和时间戳均一致才允许保存。恢复时逐路径核对实际长度、权限和 SHA-256，内容相同才还原旧 mtime；内容或权限改变的文件设置严格晚于缓存 Cargo 参考的时间戳，交由 Cargo 正常重编。新增、删除、重命名输入、symlink、越界路径、不完整或未来时间戳的缓存均保守视为 miss，不回退目录时间戳以隐藏 build.rs 的目录观察。缺失缓存仍完成冷构建，缓存不是产物身份或测试通过的依据。

key 隔离平台/架构、Rust 与 WASM 工具链、Cargo lock/manifests/config、构建与缓存脚本、实际 build variants 和 features/profile，source SHA 用于精确条目，同一兼容前缀才可回退。候选只读，可信 main producer 才可保存。8 GiB 上限约束所选 workspace 文件的原始逻辑字节；所有 producer 仅用 stat 报告完整 target、所选单位、硬链接去重字节、目录归因和预算 eligible，不为测量计算 payload 摘要、复制文件或改写源码 mtime。只有允许写入且未精确命中的 producer 才校验并准备缓存，超限正常跳过 save。远端代数与实际压缩总字节按[容量治理](#ci-cache-budget-design)独立约束，不能把单份 staging 预算当成仓库总量保护。复用收益必须包含依赖与 workspace 两次恢复、源码验证、保存与清理成本和 Cargo 实际 fresh/重编数量，不能仅凭 cache action 命中宣称免编译。

运行时优化遵循[单次 open 预热复用](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#formal-open-prewarm-reuse)和[健康 Replica 归档安装](../../p2p/design.md#formal-replica-staging-content-reuse)。新请求等待只读取精确请求的 hot result，使用轻量发布切面观察避免每 25ms 克隆完整 World；历史原请求仍保留认证冷查断言。正常成功路径达到目标 publication 后可先停止同一 runtime owner，再做不依赖活服务的材料统计与历史断言，减少测试自身继续产生的空 Tick。生产 Node 的 in-flight stop/join 语义不变，专门的 pending/in-flight 停止场景继续覆盖。阶段诊断记录 open/prewarm/restore、70/40 次操作、冷查及 stop 的单调耗时；不改变原请求绝对期限、签名、fsync 或原验收断言。

链节点入口由独立包 `oasis7_chain_runtime` 拥有，保留 `oasis7_chain_runtime` 可执行文件名、CLI、正式协议与持久化格式。`oasis7` 保留世界执行、状态和读投影；链节点以关闭默认 features 的显式依赖接入，并按需转发 Wasmtime、网络和测试 features。投影承诺树、规范编码和包含证明算法下沉到 `oasis7_client_api::formal_projection`，仅接收序列化投影和显式 audience；世界侧只枚举已授权 audience 并生成投影。通用端点协议、持久化和恢复仍由 `oasis7_distfs` / `oasis7_node` 拥有，链节点只负责世界适配与进程组合，不把这类代码重新放回游戏包。

CI 对依赖影响区分 library 输入与独立目标：包内 `src/bin/`、`tests/`、`examples/`、`benches/` 的变化选择该包自身的检查，不自动沿 library 反向依赖传播；若这些文件被其他目标以静态路径引用，仍按引用者传播。无法确认输入关系时保留保守覆盖。世界 library、状态序列化、正式读投影与共享构建输入变化继续选择 formal 验证，不把玩法代码整体视为与正式运行无关。新增、删除、重命名及测试 fixture 都使用 base/source/tested 的关系并集。

`formal-process` 使用 always 汇总：选中时构建和矩阵均须 success；未选中时矩阵须 skipped，明确输出 not_selected。required-gate 只新增此固定依赖。可信 base 尚无 formal_process 时，仅保守扩大：full 或相关 Rust 组选择全部分片；不修改旧计划使旧结果校验器接受未知组。新 base 通过原 Cargo 反向图映射选择 formal_process。schedule/manual-full 明确选全部重型分片，普通 full-standard 不重复执行重型链路；专用 package dispatch 保持原行为。

进程运行器使用单调绝对期限、20 秒存活心跳及独立进程所有权标识。Linux 扫描同 UID 环境标识并核对进程 starttime，清理包括脱离原进程组的自有后代；macOS 本地兼容使用独立进程组。退出码、超时、实际测试数、清理结果分别保存。摘要产物只含测试名和运行元数据；原输出通过 nocapture 进入 job 日志，不打包临时世界目录。

验证方法见[测试手册](../../../testing-manual.md#ci-formal-process-validation)。具体候选、失败、独立评审及 hosted 结果归当前 PR，未完成真实 runner 验证不得宣称产物复用已验收。

### Ordinary workspace library cache

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7；沿用[容量要求](ci-tiered-execution.prd.md#ci-formal-cache-budget)。

普通构建使用 `ordinary-workspace-v1`，先恢复共享依赖 target，再按上述内容、完整路径集合和纳秒 mtime 校验叠加本地单位。四个 ordinary build variants 和完整测试 artifact 保持原语义；只选择非测试 library（包括 proc-macro）及完整 custom-build 编译/执行单位，不把 test harness 或普通 binary 放进这份缓存。缓存记录的默认选集仍是完整单位，Formal 与 ECS 不受 library 过滤影响。每个 variant 分别记录本地与第三方 compiler-artifact 的总数、fresh 和 rebuilt；这些是单位观测数，不是唯一 crate 数。

候选与可信 main producer 共用构建步骤，分别使用只读和可写 cache token；只有可信 main 的 push、schedule 或 manual-full 可以保存。`oasis7-test-build` 汇总两入口：选中时必须恰好一个成功且另一个 skipped，失败、取消或未知结果不能通过；原 ordinary、full 与最终汇总继续依赖此结果；三个构建消费者使用显式状态条件和汇总 success，避免互斥入口的 skipped 祖先传播，取消后不启动消费者。保存前测量所选库的 2 GiB raw 上限，再按实际压缩归档和外包装上界检查 1 GiB 族预算及全仓预算；无法形成有界完整缓存时跳过保存并完成原构建和测试。所有状态、归档和报告位于 runner 临时目录，禁止 Python bytecode 污染源码扫描。

普通和 Formal 使用共同 workspace 入口及与 ECS 相同的归档、容量策略。准备成本、archive 恢复校验、实际 Cargo fresh/rebuilt 与远端保存确认分开记录。候选可以做只读容量预检和 stat-only 测量，不获得写入或删除权限。首次可信保存、兼容 reader 的实际复用及第三代回收仍须真实 hosted 运行证明；本地 fixture 仅验证机制。

### ECS workspace artifact cache

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7; [cache capacity requirement](ci-tiered-execution.prd.md#ci-formal-cache-budget). Product applicability: N/A; this is build-cache capacity policy, not a player-facing requirement.

The fixed Linux native `ecs` profile uses a separate `ecs-workspace-v2` family. Reuse the complete Cargo unit/fingerprint and source content/mtime checks in `ci-test-artifacts.py`, without its Formal package list. Identity covers toolchain, target, profile, features, flags, Cargo configuration and manifest/lock/helper inputs. Immutable source keys have a separate compatible restore prefix. Only the current main workflow native writer saves after native success; consumers are read-only and saving does not depend on Web success. The v2 archive has its own restore prefix; a v1 directory is never interpreted as a v2 payload.

The staging logical-byte limit is 6 GiB, the remote main family compressed-byte budget is 6 GiB, and each compatible identity retains at most two generations. Inventory accounts for every ECS format generation under the same family budget. This entry point never deletes caches. Unavailable full inventory or a conservative projected family or repository size over budget skips saving. Two generations are the retention target: a temporary third generation may be saved within both capacity budgets, then a separate trusted maintenance job verifies the exact complete current entry and removes only unprotected older generations. Insufficient headroom preserves existing recovery points and skips the new save; cleanup never creates room by deleting before confirmation.

Before installing the toolchain or building, `preflight` reads the complete repository inventory and attempts `GET /repos/{repository}/actions/cache/storage-limit`. `stage` repeats both reads immediately before authorizing an upload; the early report is not a capacity reservation. A valid positive integer `max_cache_size_gb` is conservatively converted using `1,000,000,000` bytes per GB. An explicitly configured `ECS_CACHE_REPOSITORY_BUDGET_BYTES` is a policy ceiling; when the live limit is available the effective budget is the smaller value. Only a recognized access-denied response from the quota endpoint, excluding rate limits, may use this verified configured budget while leaving the live limit unknown. Invalid responses, incomplete inventory, unrelated API failures and an absent configured budget fail closed. Diagnostics retain only bounded, allowlisted status/request/permission information; response bodies and credentials are not logged.

The repository's [Actions cache page](https://github.com/nil-studio/oasis7/actions/caches) showed a 10 GB eviction limit when the configured budget was verified on 2026-10-21. The trusted workflow therefore supplies a conservative 10,000,000,000-byte budget. This is a recorded configuration source, not a claim that every run read the live quota. Maintainers must reverify this setting when the repository cache limit changes. Reports separately identify the live limit, configured and effective budgets, capacity basis, observed inventory and projected usage. The entry point never changes repository limits or grants administration permissions.

Stage the validated units outside the checkout, preserving internal hard links, then produce the single `workspace-units.tar.gz` cache payload with flat regular-file entries. Keep the logical and unique sizes for staging diagnostics. Capacity checks use the measured compressed payload plus a conservative bound for the actual outer tar/compression wrapper used by `actions/cache`, not raw logical bytes or an assumed compression ratio. The complete all-ref/all-family inventory plus this upload bound must fit the effective repository budget. Separately, the main ECS family inventory plus the bound must fit the ECS family budget. Restore rejects unsafe paths, links, duplicate entries and excessive expanded data before invoking the existing manifest/hash/source-mtime validation. A rejected archive falls back to the normal full build. Record pack/unpack time independently from Cargo time. Capacity remains a point-in-time check: concurrent writers and later dependency-cache post-job saves may still affect eviction; this guard does not reserve capacity.

During source scanning all native staging, state, monitoring and reports live in runner temporary storage. Disable Python bytecode generation before the package helper imports any repository module, so a producer cannot add `__pycache__` inputs absent from a clean consumer. Do not broadly exclude untracked or ignored inputs. Misses, corruption and unavailable cache services fall back to the original full build. Candidate native jobs run `stage --preview` to measure the same archive and capacity policy while always emitting `save=false`; the actual upload remains restricted to the trusted main writer. Record the requested/matched key, cache-hit, validated restoration, Cargo unit Fresh/rebuilt, restore/save duration, bytes and final package identity independently. After the save action, `observe-save` confirms a unique, positive-size exact main entry through a new complete inventory; a successful action alone is not a confirmed save. Upload diagnostics after this observation. Successful trusted main producer and compatible consumer samples remain required remote evidence, obtained only after actual runs. Candidate workflows retain read-only cache access throughout validation.

### Controlled graceful-stop deadline regression

The runtime-protocol suite registers one real-owner controlled regression for admission and Node-worker budget consumption. With a short test-only timeout, a held admission gate must return retained Pending before release without creating a late worker; a later retry keeps the same owner and exactly one worker until real completion. Join-only Pending, unstarted/already-stopped stop(0), poison and cleanup failure remain distinct. This regression does not increase any existing caller or production stop timeout. Artificial blocking proves this mechanism rather than any historical CI root cause; measured return overruns include OS scheduling. No diagnostic workflow, observer or production timeout is enabled by this registration.

Controlled crash-publication shutdown must distinguish a zero-budget Pending before worker creation from a Pending while a worker owns the Node. It advances that same owner with a positive interval bounded by the existing scenario/10-second observation deadline before asserting worker ownership; zero-budget Pending alone never authorizes release or successful completion. No retry creates a replacement worker or renews the scenario budget.
