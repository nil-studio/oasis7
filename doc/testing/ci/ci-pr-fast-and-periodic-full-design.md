# CI PR 快速反馈与周期完整回归设计（分册）

- 主设计文档：[`ci-tiered-execution.design.md`](ci-tiered-execution.design.md)
- 本分册按 #6507 原样承接 `ci-tiered-execution.design.md` 中的“PR 快速反馈与周期完整回归设计”正文，不改变既有选测边界、Trace 归属或验收义务。

## PR 快速反馈与周期完整回归设计

Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29；承接[快速反馈与四小时完整回归要求](ci-tiered-execution.prd.md#ci-fast-pr-periodic-full)。本节描述当前候选实现及仍待 hosted 验收的目标。

### 执行入口与计划

保留 `ci-tests.sh required/full` 和最终 `required-gate`、`explicit-full-gate` 的职责；`plan-rust-required-scope.py --profile` 支持下列四种执行计划。不传 profile 时保留旧计划接口。`scope=full` 表示普通 required 内的保守扩张，完整 profile 使用 `scope=explicit-full` 并展开非空完整 groups。

| profile | 触发与对象 | 计划及汇总 |
| --- | --- | --- |
| `pr-fast` | PR 当前 source HEAD 及实际 tested merge | 快速基础集合、受影响消费者和命中风险的定向加测；由 `required-gate` 汇总 |
| `main-fast` | main push 的固定提交 | 快速组合验证及对应必要检查；不默认重复完整重回归 |
| `periodic-full` | 经周期观察准入判定到期的固定 main SHA（完整回归目标间隔四小时） | 全仓适用快速、重型与平台集合；由 `explicit-full-gate` 汇总 |
| `candidate-full` | 手动明确选择的候选 SHA | 对该版本执行完整适用集合；普通候选只读，发布消费者独立核对来源 |

计划沿用同源 scope 配置、Cargo 关系和 driver；当前字段如下。target/features 的真实命令由执行器生成，测试清单与环境身份再写入构建 manifest，不把计划中的组名当作真实执行结果。

| 字段 | 当前含义与消费者 |
| --- | --- |
| `execution_profile`、`fast_plan_version=1`、`check_inventory_version=1` | 区分快速/完整策略及接口、库存版本；旧 BASE 不含 capability 时不得套用候选的窄范围 |
| `source_sha`、`tested_sha`、`policy_sha`、`architecture` | 固定源码、实际 tested commit 和可信策略；PR 保留 source 与 merge 的区别，candidate-full 另记实际 tree，artifact 绑定 commit、工具链与构建输入 |
| `groups`、`oasis7_required_packages`、`oasis7_required_wasm_packages`、`workspace_support_profiles` | 组及实际 native/WASM 包范围；support 沿用版本化 profile，控制/不确定输入保留旧完整库存 |
| `ordinary_artifact_partition_version=1/2`、`ordinary_dynamic_partition_version=1`、`ordinary_partitions` | v1 保留完整目标分区；v2 对有 chain-runtime 的 targeted 或获可信 delivery v2 授权的 full 快速普通库存执行 controls 与 native case 分区（启用 `ordinary_dynamic_partition_version=1` 且 `scope=targeted`、未触达 `oasis7`/`oasis7_world_state` 重型套件且其余受影响包累计 `ordinary_harness_costs` 不超过 300s 时使用两个 native 分区，其余情况保留四个 native 分区）；保留唯一 producer 的完整 feature 组合，区别于 inline 分包构建 |
| `ordinary_artifact_delivery_version=1/2` | delivery v1 为 artifact v1 分发共享辅助产物与四个完整 harness 包；delivery v2 为 artifact v2 另共享唯一 required Runtime harness，其余 harness 保持唯一 payload owner；完整父清单、构建与测试选择不变，旧计划继续原路 |
| `workspace_support_partition_version=1`、`workspace_support_partitions` | 选中 custody worker 时单列 custody，其余 profile 归 other；两路并集保留全部选中 profile，旧 BASE 使用 all |
| `custody_execution_version=1`、`custody_node_shards_version=2`、`custody_node_shards` | 可信能力且无测试政策变化时允许 custody 三路构建与三个完整 Node 文件分片（固定 tested 源码或旧 BASE 缺少 `custody_node_shards_version=2` 时兼容回退四分片）；策略变更还须具备下述完整扩展能力，其他情况继续原 verifier，稳定 support 汇总核验全部实际结果 |
| `custody_policy_tests_version=1` | 可信能力及固定 tested 源码均支持时，快速 profile 的策略变更也可分阶段执行；依赖 custody 与 WASM 编译证明，原生合同 job 额外完整执行原 Worker expanded-feature `--all-targets` 尾部 |
| `contract_job_consolidation_version=1` | 可信配置启用后，保留全部逻辑组与细粒度选择，由 `group_jobs` 将 `codex_agent_config_validation`、`macos_package_contract` 并入 `baseline`，将 `compile_metrics` 并入 `doc-checker-contracts` 执行，并要求旧独立 job 保持 skipped |
| `fast_test_inventory_version=1`、`ordinary_fast_targets` | 可信路径合同允许时，固定普通快速检查的 Cargo target、features 和最低 case 身份；构建、Clippy、打包与执行使用同一清单 |
| `fast_execution_version=1`、`ordinary_execution=inline`、`ordinary_clippy_group`、`rust_baseline_checks` | 可信组件快速计划在单 runner 构建并执行测试，由明确的并行组执行相同 target 的 Clippy；显式选择 accounting/独立工具专项，缺少新能力时保留旧执行与 baseline 全集 |
| `architecture.build_policy_version=1`、`build_policy=ordinary-targets` | 全部直接架构输入由快速目标承接时，保留全部声明/依赖图，额外 profile 编译明确延期到 full；缺少能力或混合未覆盖输入保持原编译选择 |
| `full_fast_test_inventory_version=1`、`ordinary_full_fast_targets` | 完整 profile 在原全集上追加全部快速 target 的精确 feature 组合，不替代原 required 库存 |
| `python_control_contract_version=1`、`python_control_paths`、`python_control_tests` | 可信精确路径命中的 CI Python 控制输入及按组归并的非空最低合同，由可信基线执行入口针对候选实现验证 |
| `formal_suites`、`formal_shards`、`command_center_package` | 完整逻辑 case、实际物理分片及 App 打包选择；host Clippy 不随打包开关省略 |
| `formal_full_profiles_only_version=1` | 可信 BASE 启用重型 Formal 仅在完整 profile 执行；快速 profile 的组、逻辑 suite 和物理 shard 均不选择 Formal |
| `formal_deferred_ordinary_cases` | 同一可信计划中按 harness 与完整测试名登记、在普通批次中后移的重型 Formal case；完整 profile 为空，其他普通测试不受影响 |
| `test_policy_changed`、`test_policy_paths`、`fast_plan_exceptions`、`reasons` | 测试选择属性变化、必要扩测与运行前原因；不按实测超时事后改变普通/例外分类 |

快速规划接受 `--base-ref`、`--head-ref`、`--test-ref` 和 `--policy-ref`；完整规划只需固定 source/tested/policy，从可信完整库存展开，不依赖 diff。`--formal-config` 读取同一可信策略的 Formal 清单。ordinary producer 与 `ci-tests.sh required --group oasis7_required --plan <selection.json>` 消费同一文件；support 使用 `--support-plan-file <selection.json>`，Formal 的 build/pack/run 同时使用 `--plan` 与可信 `--formal-config`。适用环境相同才允许复用，构建成功与测试成功分别记录。

完整 `selection.json` 保留可信 planner 生成的全部字段、逐路径原因和检查库存，通过本轮既有 selection artifact 传递。执行器、架构检查、preview 与最终门禁读取同一完整文件；文件路径贯穿 shell 和 Python，不再将整份计划装入环境变量或操作系统进程参数。旧 JSON 参数接口继续兼容，文件接口共用原有选组、版本、profile、partition、feature 和来源校验；缺失或无效计划必须失败。批量删除或长路径令诊断数据超过单个进程参数限制时，仍执行原定全部检查，不截断完整证据或缩减覆盖。

workflow 的 plan 输出仅投影实际调度和身份消费者所需的字段，并完整保留对应值及必要扩测信息；新增表达式消费者须同步核对投影。各汇总和观察步骤仍取得真实 job result，完整执行与验证不得改用该投影。最终 required/full 门禁下载同轮完整 selection 与 authority artifact，先核对计划原字节摘要、投影字段一致性及已有身份与 select 独立输出的绑定，再运行事件可信 BASE 的验证实现；缺能力字段的旧计划保留原兼容语义。兼容旧 CLI 时在同一进程内传递已读取的完整 JSON，不要求旧 BASE 接受候选新增参数。完整结果回执保留完整计划，失败与来源约束照常传播。回归须跨真实 shell/Python 进程验证大于 128 KiB 的计划、多样引号和换行、旧 BASE 验证器兼容，以及缺组选项、错误 profile/partition 和来源身份仍被拒绝。

PR/push 继续从事件可信 BASE 取得选择策略、配置及汇总规则；schedule/manual 没有 PR base 时，固定已核准的 main/workflow 策略版本，再解析待测 source，不能以候选自带的策略缩小要求。删除和重命名两端、真实嵌入/生成输入和必要反向消费者取并集。未登记路径明确失败；Git/依赖解析不确定采用保守必要集合并记录超过快速预算的原因。共享控制规则保留，后续更细分类只能按现有独立评审流程进入可信基线。

workflow 先探测可信 planner 的 `--profile` capability；旧 BASE 的 PR/push 调用旧接口，driver 缺少新版本字段时保留原 native、WASM、Formal 和 support 范围。可信 main 尚未具备完整 profile 时，非 main 的手动 full 可用旧完整组库存做首次接线验证，计划标记 `bootstrap_full=true`。该只读运行不能缩减旧覆盖，也不能被发布消费者接受；main 的周期/完整入口不以此 bootstrap 替代可信策略升级。

风险映射由真实路径、消费者和行为合同决定，不依赖作者贴标签或模型猜测。没有已验证的窄用例映射时保留相应普通完整组，不能将缺映射解释为无风险。重型 Formal 由可信 `formal_full_profiles_only_version=1` 单独安排到完整 profile：`pr-fast/main-fast` 无论 targeted、保守 `scope=full`、风险映射或显式追加组，最终均移除 `formal_process`，输出空 `formal_suites/formal_shards` 并记录原因。`periodic-full/candidate-full` 保留全部组和五个逻辑 suite；`scope=full` 只表示普通检查扩测，不能据此恢复重型 Formal。旧 BASE 缺能力时沿用原风险选择，未知能力版本失败。普通 CAS/Formal Protocol 库、单元及控制合同测试继续执行。

Formal producer、合并及物理矩阵继续消费同一可信选择输出；快速层不产生 Formal 产物，`select` 成功且未选中 Formal 时 `formal-process` 直接在 job 层 skipped 并由 `required-gate` 核验未选中跳过（`select` 失败时仍运行以保持失败可见）。完整层仍要求恰好一个 producer 成功及矩阵成功，`explicit-full-gate` 核齐全部配置组。执行器仅在新能力与快速 profile 同时成立时接受空 Formal；Python 控制迁移也仅在可信配置已启用该能力时，从普通 full 的精确应选组集合移除 `formal_process`，其他组缺失及完整 profile 缺 Formal 仍失败。候选新增能力不能给旧 BASE 的迁移计划授权。

两个已登记的 BFT membership 长场景没有 ignored 或 Formal 环境门控，会在 ordinary `runtime-tests` 批次重复运行。可信 scope 配置以完整 case 名将其写入快速计划的 `formal_deferred_ordinary_cases`；执行器核对 Chain Runtime library 目标身份，以 libtest `--exact --skip` 精确后移并从实际应执行数中扣除，记录延期名称。近邻名称、其他 Formal 命名的单元/合同测试和失败计数保留。两 case 仍由完整 Formal 环境批次及物理分片各覆盖一次；完整 profile 清空延期表，旧 BASE 无能力时不排除任何普通 case。Formal 延期在普通目标、架构编译、Clippy 与 support 转交范围推导完成后应用，不借此进一步缩减其他检查。

### Python 与 CI 控制输入的职责选择

共享 `ci-tests.sh` 仅有一类可证明的范围例外：可信 BASE 支持纯追加诊断测试注册时，唯一 `run_workflow_governance_operational_contract_tests` 函数外字节不变，原命令、顺序、参数与次数完整保留，只增加已登记到 workflow_governance 的 literal Python 测试调用。新增文件必须是实际 Git regular blob；source 与实际 merge 比较均须成立。只有文档及同职责诊断混改可收窄到 baseline/workflow_governance，Rust、其他职责、共享 workflow、策略或解析不确定继续 full。preview 复用同一证明，旧 BASE 保持原范围。

普通 Python 路径沿用显式 `rules` 与 Cargo 输入图，按文档、治理、打包、架构或协议等真实职责选择已有组；直接调用、嵌入数据及反向消费者继续参与选择。无真实合同的工具不能为了缩短 CI 被任意登记为轻量治理组，未知路径仍由既有门禁阻断。

进程监督 helper `ci_process_runner.py` 的直接消费者包括 native/Formal 执行、Viewer 并行分支和 storage acceptance。其精确路径登记到可信 Python 合同，最低执行 `ci-formal-process.test.py`、`ci-support-profiles.test.py` 与 `ci-storage-acceptance.test.py`，使 helper 变更也受到固定 BASE 行为断言保护；只有治理组映射不能替代该验证。单独修改 helper 保持原有 baseline 与 workflow-governance 职责范围，混合共享控制变更仍执行该 profile 的完整必要组。

对原本命中共享控制的 Python 脚本，可信 `scoped_control_paths` 允许登记规范化的精确 `scripts/` 路径，不接受扩展名通配豁免。对应 `python_control_contracts` 必须指定已选中的执行组与非空精确测试清单；未登记或合同不完整时不能取得窄范围。登记的最低测试文件自身变化也通过精确反向关联触发同一合同，不能仅因它原属普通文档检查而绕过可信验证。命中后的计划保留实际源码或测试路径、按组去重的测试清单，并选择 `workflow_governance` 执行独立可信合同。真实 Rust 消费者与混合变更要求的组照常取并集；scope 配置、共享执行器、可信合同入口及 workflow 自身变化继续按共享控制处理。

workflow 从与选择器相同的 BASE 取得可信合同执行入口，在候选 driver 之外执行；测试源码也固定来自该 BASE，受测实现与路径绑定到候选 checkout。执行入口核对非空发现、真实执行、完成状态及失败传播，拒绝跳过与成功退出但未完成合同的情况。候选自身的完整回归仍由所属组运行，不能替代可信最低断言；最低用例身份的删除、空发现或意外跳过须在本轮被发现，不能随候选成为下一轮基线后才暴露。断言语义的保留仍由独立评审核对。可信入口及该固定 revision 配置登记的全部最低测试源码纳入完整 CI 结果的检查定义摘要，固定安全测试也必须属于这份登记；旧结果不能签认已改变的执行合同。

治理 Python 回归由同一个可信协调入口执行。配置中的 `workflow_governance_python_tests` 声明原 baseline 治理函数的完整 Python 清单；具有平台专用 CLI 选择语义的资源清理测试、运维诊断、shell、Node 和 telemetry 合同保留原入口。协调器合并每个 BASE 测试文件的固定子集与本轮最低完整合同；完整合同覆盖其子集，只执行一次。候选清单不得删除 BASE 已登记文件，可以追加；候选治理清单、最低合同与固定安全断言按文件合并后执行一次完整候选 suite。BASE 源码、候选源码和真实 Rust fixture 仍是不同验证责任，不互相代签。

所有受托管协调器执行的 suite 继续由 BASE worker 逐项见证，保持原非空、身份保留、零意外 skip、真实结束及失败传播检查。协调器成功完成后直接执行同一候选 driver 的剩余治理函数，不写可复用成功回执、不接受候选提供的跳过清单，也不跨 run 缓存测试结果。workflow 仅探测下载的 BASE 入口是否支持该完整模式；旧 BASE 仍执行原可信合同加候选完整组，新候选不能自行声明已获得去重复权限。本地完整治理入口沿用各测试文件的原 Python CLI 和平台 skip 语义，不能代签托管的严格完成见证。执行日志记录每文件来源、用例数和单调耗时，实际前后结果归 PR。Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29。

迁移期可通过独立只读 `pr-governance-preview` 比较完整治理入口。两边固定同一实际 tested merge、候选 self-policy 与 planner 生成的已登记 workflow-control 场景，保留完整治理清单；事件 BASE 只提供旧 runner 和 driver 的实现。旧方式重放最低合同、完整 driver 及 workflow 原有的两个 archive 与额外 Node 合同，新方式执行完整 `run --governance`。诊断不提供 BASE 授权，不被 required/full gate 消费；仅双方全部成功后记录耗时差，任一失败则保留不完整证据并失败。同 runner 顺序执行会受到后执行者的缓存预热影响，结果不是独立冷启动或 P95；BASE 已含新协调入口时明确记为不适用，不继续生成旧方式对比。Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29。

隔离执行 shell dispatcher 时，夹具加载受测 driver 的完整函数定义，再覆盖 `run`、Cargo 等明确的记录边界；不手工维护一份易随 helper 演进而遗漏的函数名单。driver 的 source 接口只提供定义，不解析 CLI、不选择解释器、不切换目录或执行检查，也不改变调用方的参数与 shell 选项；正常执行仍经原 CLI 初始化和分派。可信入口仅对明确登记的 support dispatcher 夹具提供这份候选定义闭包，使旧 BASE 测试仍能执行原断言；注入只影响该夹具启动的 shell，不传播到其真实子命令。没有该接口的历史 driver 保留既有夹具路径。缺失 helper、命令失败、空执行及提前成功退出仍判失败，不能将这类兼容问题处理为跳过 BASE 合同；新增 helper、跨版本组合和失败传播须分别回归。

可信合同本身的演进使用明确的完整验证迁移路径：可信入口核对共同祖先到 source 的分支差异与 BASE 到 tested 的实际合并差异，只有精确策略配置或可信入口变更且计划保留该 profile 的完整必要组时，才允许迁移旧的全量策略断言。可信 Formal 新能力仅允许快速 profile 从全组中排除 `formal_process`，其余缺项仍失败。main 单独前进不构成候选迁移。迁移继续执行固定 BASE 安全断言、真实 Rust fixture 及候选完整合同回归，并接受独立评审；固定最低安全身份不能借迁移删除。日志列明迁移原因与实际范围，不宣称完成已迁出的全部旧合同。普通 Cargo/共享输入与 Python 的混合变化不因 `scope=full` 自动取得迁移资格，也不接受候选自报的迁移开关。

planner 的可信最低合同固定验证必要的选择、未知输入和失败边界，另以离线微型 Cargo 项目实际编译并执行 cfg、features 和测试清单，覆盖词法归属及门控变化。该合同不冻结每个历史路径的全量策略断言，允许经过评审的职责细分进入新 BASE；候选完整 planner 回归仍验证新策略。它不构建产品 workspace，也不替代真正受影响的产品 Rust 验证。

完整 profile 保留原全集并执行完整控制合同库存；合同自身迁移仅适用上述明确条件。普通 required 的保守 `scope=full` 执行本次实际命中的控制合同，不混同于完整 profile 的全库存。旧 BASE 未提供此能力时，候选保持旧共享控制范围，不能以自身新规则收窄当前 PR；本轮改变可信配置、执行器和 workflow 的迁移同样接受旧必要检查。纯 Python 具名路径、真实嵌入消费者、混合 Rust 变化、删除/重命名、未知路径、空执行、取消与失败传播分别回归；十分钟效果仍由合入后的真实 hosted 样本验收。

<a id="ci-pr-fast-selection-design"></a>
### 准入规则到实际选择

承接 [快速检查准入规则](ci-tiered-execution.prd.md#ci-pr-fast-selection)。沿用实际 driver 的同源检查定义，关联每项检查的行为合同、适用输入/消费者、默认职责、风险触发、真实命令、构建与环境前置；时延和冷暖缓存样本保留在实际运行记录。无需另一份审批或性能登记账本，也不能把缺少历史样本设为不执行新增测试的条件。

选择按以下顺序进行：

1. **先确定必要覆盖。** 从可信基线规则和完整差异产生基线、受影响快速候选及风险定向集合。修改测试、fixture、生成输入或依赖与修改业务源码一样按真实消费者处理；新增/重命名 case 与实际可执行清单核对，旧枚举、遗漏标签不能让它消失。除明确安排到完整层的重型 Formal 外，关键风险所需的 full case 先加入普通必要集合，再处理其 feature/target 前置。
2. **再形成可运行计划。** 把必要检查映射到最小完整 package/target/features 和平台闭包。对缺少可信窄映射的范围保留相关旧必要组；不确定图及共享控制仍按既有规则保守扩大。只有符合可重复、隔离、有界要求且整体成本适合快速目标的检查才标为已验证快速；其他必要检查继续执行，普通范围的超预算如实暴露。
3. **最后核对实际结果。** 计划记录每项选择原因，执行器核对预期 case/命令/平台与真实结果，汇总拒绝缺项。估算关注共享准备与并行依赖形成的关键路径，不以各 case 耗时小于十分钟或相加各阶段 P95 证明达标；整条 workflow 的实际成功反馈 P95 单独验收。

对照可信 BASE 与候选的标签、cfg 和实际清单。planner 按完整词法 module/函数归属比较选择属性，required 改 full 时标记 `test_policy_changed`，在有实际扩展 driver 且 manifest 声明 feature 的 package 上显式启用 required、full、wasmtime/viewer 等适用组合。条件表达式按实际执行组合求值；例如 `any(test, feature = "test-support")` 在测试 harness 中为真，不能只因另一个分支的 feature 未启用就拒绝。引用的 feature 仍须由真实 owner 声明，声明不等于已经启用；普通组合无法证明时才检查实际扩展 driver。无法证明的嵌套选择门控、未映射的新 ignore 和跨 binary 同名短函数均不能让用例消失。

#### Authority 消费者与原场景分层

可信基线登记 authority verifier、BFT V3 worker/startup、Controlled writer 的精确输入及实际消费者。仅当整个变更的相关输入都有可信映射、源码在指定 Cargo target/features 内可达、没有测试政策迁移或未知输入时，ordinary 使用 Chain 的 native `node-libp2p + wasmtime + test_tier_required` 库，consensus 变化另选既有 `pr_fast_incremental` 物理合同，避免将全部反向依赖的世界字段、内建 WASM 和业务旅程一起带入。consensus/node 仍由原独立组执行完整库合同；这些组不能被普通消费者映射移除，未能证明其执行能力时回到原必要范围。所有策略来自 BASE，候选不能借本轮新合同减少自己的 required 验证。

Chain 定向执行按可信完整 module 前缀取并集：`formal_writer::tests`、`formal_node_hook::tests` 和 V3 `header_certificate_v3_tests`，保留族内所有实际非 ignored case，包括新增 case；另核对非空最低清单，特别保护 live worker、startup replay、真实冷重开、publication 失败和原 completion 恢复。执行器从真实 `--list` 清单计算精确差集，缺少目标/前缀/最低 case、重复身份、未跑完、取消和失败都必须判红。相应 trusted fast plan 使用差集；完整 profile 的原 `required` 库保留原全集与原 feature 组合，不能按前缀缩小；`full-regression` / `candidate-full-regression` 的既有 `full-extended` 入口另行执行 `test_tier_full` 全集。另行追加的 `required-full-fast-N` 原生 feature 变体只执行其登记的完整前缀合同，必须同时匹配该补充目标的身份与 variant 序号，避免在原完整库之外重复执行所有无关用例；既有无前缀的完整组件变体仍全量运行。复用原 library 意味着编译仍包含该库的测试源码，这项改造不以名字过滤证明独立编译或十分钟达标。

四 live-node bootstrap 轮换与两代 bootstrap 完整场景在既有 module 挂载处增加 `test_tier_full` 门控；十高度存储容量 smoke 对单个原 case 门控。原测试体与 helper 保留，不添加 ignore、不缩短过程、不改变节点、数据与断言。三个轻量存储 IO/progress 合同和原大规模手动入口保持其职责。 这三个长组合默认属于完整层，相关多代、网络活性或容量工作通过现有 `candidate-full` 提前验收；路径映射不宣称任意 worker 改动已经执行全部多节点组合。策略迁移 PR 继续由可信旧 driver 启用完整 features 检查；`periodic-full`、`candidate-full` 仍执行三个迁移后的场景。

普通包级计划中的 `pr_fast_incremental` 与三个原 integration 只有在可信能力、实际同一 Cargo 构建/依赖类型、feature 组合及完整共享源码合同都匹配时才可去重。优先保留三个原 harness；仅省去全部 case 已被原 harness 完整覆盖的组合 harness。独立 native feature 变体、完整 profile、来源或库存不匹配继续各自执行，不能凭短函数同名去重。可信窄计划直接选择组合 target 一次，仍保留 Raw/Zstd、冷重开、原 parent material 与继续执行断言。

首批五个增量规模用例保留独立的必要与大规模入口：原 integration target 的 required 使用 `2 * VALUE_CHUNK_BYTES + 17` 跨叶边界输入，专用 `incremental_full_scales` target 保留五个原名及 1/32/64 MiB、Raw/Zstd、成员数量和预算矩阵。Module State 的小输入保留现有 CBOR 32 位长度类别；Runtime Effect 的 full 场景继续证明未变数据大于 24 MiB 工作池也不被读取。两端复用不注册测试的 helper 和纯 fixture，其他身份、拒绝、取消、原子失败及冷恢复用例留在原 target。专用 target 以显式 `[[test]]` 及 `required-features=["test_tier_full"]` 声明，保留 crate 级门控；required 不再编译、链接它的空 harness。其他 integration targets 继续自动发现，共享 helper 仍由 required 编译。此步只裁去一个独立空产物，不据此推定共享依赖闭包或端到端耗时已经收缩。

行业增量状态的 `incremental_industry_jobs` 由 `oasis7_distfs` 的独立 integration target 承接，直接使用已有 `oasis7_world_state` 公开 API 和真实物理 PageStore，不再经顶层 `oasis7` 包引入 Runtime、World Service 与 Wasmtime。保留原八项测试、全部输入规模、Raw/Zstd、预算、错误拒绝、取消、完整 transfer 和冷恢复断言；本地 fixture 只提供临时目录与物理 I/O 预算，不复制 Inventory 业务夹具或引入新的暂存层。保持 integration harness，避免改为库单元测试后改变被测 DistFS 的 `cfg(test)` 环境。实际 Cargo 产物及直接依赖用于核对退出的编译单元，构建缓存状态与执行耗时分开报告。

接线先进入可信 BASE，再迁移测试。DistFS 的 required、expanded 和 full 共用 driver 保留原 library/endpoint 测试、Clippy 及转交命令。仅当 `crates/oasis7_distfs/tests/incremental_industry_jobs.rs` 存在时，另以默认 features 显式执行 `--test incremental_industry_jobs`，使用显式单线程测试参数和严格 Clippy。required 复用同一可信计划已验证的 `ordinary_fast_targets`：精确快速库存未选择行业 integration 时，行业测试和 Clippy 都不追加，以保持原快速路径范围；合法普通库存明确选择该具名目标、没有精确库存、策略变化、保守 full、完整 profile 或独立本地组时仍执行。行业文件与 support 的直接或混合变化不能借其他 fast target 的规则缩小覆盖。此选择不依赖 Clippy 转交 owner，非法计划与 helper 失败不能视为未选中；行业 Clippy 执行时仍归 DistFS。目标尚未迁入时保持原行为；文件存在但 Cargo 目标无效、feature 不满足、编译或测试失败均传播失败。planner 仅为该精确 target 名与 canonical 路径补充 native 证明，仍核对实际默认 feature、harness、静态挂载及全部 cfg；同路径别名、其他 integration、禁用 harness、未开启 feature 或不明路由不获此证明。

行业 target 的单线程是此次运行参数选择：迁移前的非 inline ordinary batch 不固定 libtest 线程数，继承默认或环境配置；inline 路径按 CPU 预算设置并发。八项行业测试在新 target 的四线程和单线程配置下均已完整运行，分别记录原四线程、候选四线程和候选单线程的实际耗时。串行可能增加命中该 target 时的测试墙钟时间；原 library 的 RPC deadline 约束不能用于证明行业必须串行，依赖闭包与选测范围缩减不等同执行阶段或端到端提速。

DistFS workflow 显式通过 `OASIS7_CI_DISTFS_PLAN_FILE` 传递完整 selection 文件，保留旧 source driver 的原 CLI 参数。新行业检查优先使用显式 `--plan`，没有该参数时才读取此环境变量，并执行相同 source/tested 身份及计划校验；不隐式搜索 runner 文件，不从环境输入变更 library Clippy 的 owner。旧 driver 可忽略该环境变量，手工调用没有两种输入时保持完整行业执行。

普通 artifact 的原 `oasis7` 库存移除旧目标，由 DistFS 组承担完整八项；Formal 的 `distfs-tests` library 产物保持原职责，不把 integration 混入同名 Formal 产物。目标或 support 修改按独立测试输入选择 DistFS，生产状态及物理存储修改继续沿既有实际消费者规则展开。迁移接受旧 BASE 的必要覆盖，不用候选配置授权自身减测；旧 owner 的历史耗时随旧目标移除，不作为新目标的实测成本。Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29。

选择器对 integration target 的精确解析同时保留显式声明和未被其名称、路径覆盖的自动目标，支持 `tests/name.rs`、`tests/name/main.rs` 与显式自定义路径。自动发现遵循 manifest 的 `autotests` 和实际 edition；继承 edition 从同一 Git revision 的 workspace 解析。重复名称、歧义路径及无法建模的声明不能证明执行。普通根仍拒绝 manifest feature 门控；具名 target 按实际默认 feature 设置和本包传递 feature 闭包同时核对 required-features、源文件及父模块 cfg。模块目录按当前所证 target 的入口解析；同一文件作为另一个 target 的根或被显式覆盖的自动路径，不能改变它在当前路由中作为外部模块的目录。所有潜在消费者继续保留禁用 harness、full 门控及无法读取的显式目标，不能把未执行当成不存在。

可信配置通过 `ordinary_scale_contract.version=1` 登记专用 full target 及五个原具名最低 case；计划以 `risk_test_inventory_version=1` 与 `ordinary_risk_targets` 下发。现有逐路径 Cargo 影响涉及 owner、且原来走整包测试时，普通 PR 在 required 上追加该完整 full 目标；测试或 fixture 正文变化、混合 core 输入、控制/不确定范围及生产 cfg 变化均保守加测。专用 target 只注册规模合同，避免再次构建执行原五个完整 integration target 中的其他必要合同；初始库存为 required 39 项加 full 5 项，不能因分层变成 39 加 44 项。已有精确快速 target 合同原本不执行这五个集成目标，继续原范围。风险选择不能借 `test_policy_changed` 代替长期加测。

首版只豁免已审计的 DistFS `feedback.rs`、`feedback/rate_limit.rs` 和 `feedback/tests.rs` 中可证明隔离的普通函数体变化。这些输入属于独立 FeedbackStore，五个合同使用 IncrementalPageStore 与 typed records；可信配置固定三个文件的词法结构指纹及 `lib.rs` 原始内容摘要，分别核对 source 差异与实际 tested merge 差异两端。签名、导入、模块挂载、属性、const/unsafe、嵌套 item 与宏输入均保留在指纹中，未知宏或同 crate 的相关宏覆盖/导入不能取得豁免。现有依赖图若发现这些源码还被 include 或作为数据读取，或 reader 关系未解析完整，也回到 full 补测。新增 helper、测试或相邻文件以及结构漂移均回到 full 补测，不能仅凭路径放行；也不能将当前新增 helper 的历史 PR 直接当成豁免收益样本。豁免不移除原 required 包、下游消费者或反馈行为测试。

构建、Clippy、实际库存和执行共同消费精确目标；最低 case 不能 ignored 或缺失，所选目标全部非 ignored 用例均执行。补测使用独立 `required-risk-*` 产物，不能顶替原 required 库存。迁移由旧策略显式展开 full feature 时复用原产物，仍核对真实默认/full features、请求 feature 和最低库存，避免重复构建执行。此能力仅由 ordinary bundle 承担；`scope=all` 构建或校验遇到风险合同明确拒绝，formal 子 bundle 保持自身职责。定期及候选完整回归继续原 `full-extended` 驱动，执行小合同及原 full 大规模入口。

新增且可证明可达的 item 不等于已有测试门控迁移。普通新增测试及其辅助模块，必须能从实际 Cargo test target 沿静态 `mod`、`#[path]` 或已解析的 `include!` 链确认执行归属，并按该 target 的普通 native Linux Cargo 组合求值，包括实际启用的默认 features。所有引用路由均须闭合到同一真实 package，条件均可判定，且至少一条完整路由及 item 条件成立；同一文件的互斥 production/test 挂载可分别为假/真，不能把两路条件相与，也不能用一条好路遮盖未知或未登记 feature。显式 `--no-default-features` 的 target 仍按其真实组合判断。不能仅凭文件名、函数名或某一层 `cfg(test)` 放行。满足条件时沿用受影响 package、必要反向消费者和风险规则，不追加无关 full features。Windows-only 条件、未知宏、无法解析的外部模块链、禁用 harness/测试 target 或未满足的 required-features 均不能取得普通新增豁免。

生产平台 cfg 的新增 item 只有在真实 ordinary driver 上可达、没有覆盖或替换既有门控时，才保留路径与依赖图确定的必要包及平台范围。既有 item 门控变化、测试删除/改名与新增混合、无法证明的条件或共享控制变化继续保守扩大覆盖；这些检查不把生产 cfg 当作测试退役，也不因新增项可达而减去同一变更中其他输入要求的验证。

`pixel_world_bridge` 的实际 ordinary 根为默认 features 的 library harness；同次选中的 pixel 组还执行 `wasm32-unknown-unknown` 生产检查。新增直接 free function 内的精确 wasm32 / 非 wasm32 语句门控，可按每个分支及全部祖先的同一真实编译组合证明。所有引用路径均须已知并闭合到该库根；生产检查不能证明 WASM-only 测试已执行，既有函数、新增未知谓词、宏生成归属或共享控制变化仍走保守路径。此能力由可信 BASE planner 登记，候选不能自行授权。

自动检查不冻结普通测试名称，也不证明任意 Rust 修改语义等价。普通 case 的删除/改名由既有独立评审核对原行为去留、替代检查或可信规则确认的不适用；未核清时保留相关旧必要组。Formal 的预期 case 另由可信最低库存强制核对，不能仅凭候选清单删除就视为无须执行。

首次切换时逐项比对旧必要集合、新 PR 集合及周期集合。差集中的每个检查均需说明验证对象、所属层和实际完整入口；相关普通行为或缺陷修复回归在本次 PR 执行，重型 Formal 按明确分层由定期或显式候选 full 承接。迁移核对完整库存、真实执行与失败传播，并接受 CI 控制独立评审；旧 BASE 仍保留其原有要求。

### 现有测试的首批归类依据

下表用于解释当前选择及后续细化依据，实际入口见 [CI driver](../../../scripts/ci-tests.sh)、[Formal 清单](../../../scripts/ci-formal-process-suites.json)及 [Command Center workflow](../../../.github/workflows/command-center.yml)。ordinary 对已登记输入按独立 target 合同收窄，其余按 package；support 按 profile，重型 Formal 在完整 profile 执行全部逻辑 suite；表中快速候选尚未取得逐项 hosted 预算验收，现有 `required`、组名或 feature 不能直接改称已达标 fast。

| 现有入口或用例 | PR 快速候选与触发范围 | 合入前必要加测 | 周期完整覆盖 |
| --- | --- | --- | --- |
| `run_required_gate_checks()`、文档/路径/可执行位治理 | 基线入口每次执行，正文、引用、索引和配置按真实输入选测；规划及汇总由既有独立入口保证 | 检查器或规则变化执行其合同与失败路径回归 | 全库一致性及历史报告，保持原阻断/报告级别 |
| `cargo test -p oasis7_consensus --lib`；`proposal_conflict_is_rejected`、`publish_happens_only_after_commit` 等 | 共识及必要消费者变化时保留正常提交、冲突拒绝、发布顺序和快照不变量；按实际 target 成本决定拆分 | 修改真实提交/最终性或身份接续时追加对应进程场景 | 适用共识组合及完整真实进程验收 |
| `run_oasis7_distfs_tests()`、CAS/Formal 协议库测试 | 存储/编码/协议变化时保留哈希、往返、拒绝与状态不变量；不预设整个 lib 都快 | 持久化时序、原子发布、锁、crash/restart、重试变化追加真实恢复 | 完整恢复矩阵、长记录与多副本场景 |
| signed-read codegen/Python/JS 合同；custody/signer 库测试 | API、签名、授权及消费者变化时保留编码、身份绑定、允许与拒绝路径 | Worker 平台、真实 HTTP/CLI/浏览器身份链路变化追加实际消费者 | 全部适用平台与完整身份链路 |
| Viewer `test:frontend-structure`、`test:feedback-contract`、`test:ui` | 前端、接口及消费者变化时执行相关逻辑、反馈合同与必要编译 | auth/会话/权限变化执行 `viewer-auth-browser-security-smoke.mjs`；WASM/runtime 变化验证真实消费者 | 完整浏览器、WASM 构建与跨模块组合 |
| `oasis7_simulator --test oasis7_init_demo --features test_tier_full`，现有过滤 `oasis7_init_demo_runs_` | 场景资产/初始化及必要消费者变化时保留实际场景验证；能否快速须量测 | 初始化、资源守恒和世界配置的相关完整场景属于必要集合，不因 full 标签推迟 | 全部场景及扩展组合 |
| Formal 五个逻辑 suite 的完整 case | 有界状态转换与关键不变量由适用库/合同用例验证 | 需要提前验证重型场景时显式运行 candidate-full，普通 PR 不启动重型 Formal | 定期与候选 full 均保留全部逻辑 suite 与原 70/40 次长场景，物理分片不决定覆盖范围 |
| `workflow_governance`、`packaging_contracts`、Command Center Core/UI/类型合同 | 对应控制、打包、Core/UI 输入变化时执行选测/失败传播/接口等适用合同与编译 | 来源/权限/缓存写边界接受可信旧验证；安装、路径、进程及平台变化验证实际 OS | 完整控制合同、原生 App/打包及运维平台矩阵 |

例如 [共识用例](../../../crates/oasis7_consensus/src/quorum/tests.rs)中的冲突拒绝和提交后发布可作为快速层的真实不变量检查；[成员恢复认证矩阵](../../../crates/oasis7_consensus/src/membership_restore_auth_tests.rs)使用真实签名并检查拒绝后原状态不变，不能用“只保留成功 smoke”替代。Formal 的 `protected_endpoint_children_world_long_history_prunes_reopens_and_retains_original` 和 `protected_endpoint_children_sigkill_runtime_durable_phases_resume_original_once` 均在定期或显式候选 full 执行，历史裁剪、重开、durable 阶段及 crash/retry 修改也不改变其所属层。完整选择依据逻辑合同和实际 case，不按物理 shard 名推断覆盖。

### 快速基础检查与编译范围

#### 独立快速 target 库存

可信 `ci-required-scope.json` 同时登记精确输入合同和组件源码合同，均绑定实际 Cargo 目标及非空最低 case 清单。仅 `pr-fast/main-fast` 的 targeted 计划、没有测试政策迁移且每个相关 native/Formal 输入均命中可信合同，才以 `ordinary_fast_targets` 替代包级 `--tests`。组件路径匹配只是候选条件，必须再证明改动源码沿真实 module/include 路由在所选 target/features 中可达；`src/bin`、未接线模块和所选 lib 之外的 integration 不能借目录匹配进入快速计划。旧 BASE 缺 capability、共享控制、未知或未映射输入、混合风险变化均保留原必要范围。

| 目标 | 完整原始用例 | 选择依据 |
| --- | --- | --- |
| Runtime `pr_fast_runtime` | `src/tests/basic.rs` 的 20 个用例 | 已登记的动作测试和独立 round-robin 调度，包含拒绝、ID 回绕、余额溢出、容量与 tick authority 不变量 |
| World State `pr_fast_reducers` | Runtime `src/tests/apply_domain_event_guards.rs` 的 11 个用例 | 已登记归约测试源，保留幂等、篡改重复、legacy 格式、错误 service 合约及缺失角色的拒绝与状态不变 |
| Runtime `pr_fast_incremental` | `oasis7/tests/incremental_runtime_commit.rs`、`incremental_runtime_allocators.rs`、`incremental_due_module_batch.rs` | Raw/Zstd 实际关闭重开、原始父材料、冷根续跑及有界调度批次；三个原 case 和全部断言不变 |
| Runtime `--lib` | 全部非 ignored 原生 unit tests | 已登记的增量 commit/allocator/clock/body/scheduler 实现或测试变化始终执行，同时以最低清单保留 parent/fence 拒绝、取消无成功前缀、stale clock、重复实例与 allocator 错误等负例 |
| 组件 `--lib` | World State、Simulator、World Service、World Projection、oasis7、Viewer Codec、Observability、Launcher Config 各自全部非 ignored unit tests | 可信源码合同和真实 target 可达性同时成立；World State 的生产及未证明私有测试输入同时执行 Runtime lib 与 `pr_fast_reducers`，保留跨 crate 的真实行为测试 owner；不以函数名过滤减少执行 |
| Viewer Runtime `--lib` | `runtime_live/agent_service_*.rs` 与 `stream_budget_diagnostics.rs` 的行为由全部非 ignored library tests 承接 | 无默认 feature、启用 `test_tier_required`；最低清单保留同阶段精确分派、身份变化拒绝、失败后原请求查询恢复和诊断有界性；其他 Viewer 源码继续使用原范围 |
| Game Client 本地服务 CLI | 完整 library，以及 `identity_cli`、`dev_test_catalog_cli`、`legacy_profile`、`dev_test_service_cli` 四个完整 integration targets | 仅登记私有 Unix CLI 适配文件及上述独立测试入口；真实二进制保护参数分派、私有配置、输入边界、socket 调用、凭据拒绝和子进程回收；共享 SDK、权限校验实现、身份、存储与恢复源码不在该合同内 |

目标使用配置中明确登记的默认 feature 设置与显式 features；声明 `test_tier_required` 的目标启用该 feature。Game Client 使用默认 features，与原 support library 入口相同；不能把该包没有 feature 声明当成永久不变量。增量生产修改同时选择 Runtime lib 和物理 integration，保留正常、拒绝和取消保护；不默认构建或执行 Chain Runtime 等上层完整测试包。精确输入规则优先，已有 module/导出声明限制失败时，组件规则不能再次放行。组件规则使用有界的 owner 源码范围及排除项，并在 base/source/tested 比较中核对真实可达性，不能把整个目录无条件视为已覆盖。

#### 私有 library 单元测试输入

可信 `ordinary_unit_test_contract_version=1` 及 `ordinary_unit_test_targets` 登记可独立承接私有测试源码的完整 library，首批为 Runtime、World State。该能力只适用于所有改动输入均为既有 Rust 文件、全部比较树都证明同一真实 owner 的普通计划。先闭合所有静态 `mod`、`#[path]`、`include!` 引用，再逐路确认唯一 Cargo 根为所选 library，且每路均有明确 `cfg(test)`；同时按该 target 的真实 features 证明执行可达。其他 library、binary、integration、example、bench 或禁用 harness 的根均不能被遗漏。文件名包含 test、只有一条成功路由、Unix 或非 WASM 门控均不足以取得此能力。

该登记合同限定这些私有源码不是其他消费者的运行时数据输入；新增数据消费者必须登记真实输入边，不能以动态路径读取绕过已有影响分析。静态检查不声称证明任意 Rust 程序的文件 I/O 或外部宏行为。它保守拒绝跨包/其他 target 挂载、未知或宏生成模块路径、include 宏别名与参数转发、Git 符号链接、非 NFC 标识符、已知改动源码的数据读者及未解析嵌入。正常嵌套的已解析 include 调用仍按其真实输入检查；读取未改祖先文件的字节不等于读取改动叶子。全部本地 host 生成入口须属于已有可信摘要登记的自包含 producer，且无未登记 build script、proc-macro 或传递本地 build dependency；不能以有限字符串扫描声称证明任意 host 文件访问。BASE、source、实际 target 与 tested merge 所涉及的树分别证明，候选不能改变能力、target 或生成器登记来授权自己。新增/删除文件、测试门控迁移、混合生产或控制输入、路径加测及显式请求均保留原范围。额外词法库存只在可信能力登记的候选源码变更中启用，未扫描不能当作空库存授权；文档和控制输入沿原路径处理。

通过证明时仅去掉这些输入没有实际消费关系的反向测试目标和组；保留完整 affected 集合作为证据，另列每个输入的 owner、比较树和排除的原组。完整 owner library 的全部非 ignored case、原 feature 组合、strict Clippy、基线、格式与完整架构声明图继续执行。World State 的生产及未证明输入仍绑定 Runtime lib、reducers 和适用的物理 incremental target；共享测试按全部真实 owner 执行。旧 BASE 缺少该能力时沿用原选择，周期 full 的完整库存不变。候选 preview 只测量新规则，不替代迁移 PR 的旧可信门禁；hosted 端到端和普通 PR 分布分别验收。

Game Client 的 `dev_test_service_cli.rs` 只承接原 `dev-test-serve`、`dev-test-call` 的参数分派与调用适配。Linux 本地身份代理的 `dev-test-local-serve`、`dev-test-local-call` 由可信公共分派入口按完整命令名提前交给未登记的 `dev_test_local_adapter`，平台条件、模型请求解码和本地身份校验均留在该模块；这些输入与公共分派入口变化继续扩测，不能进入 legacy 私有叶子的快速合同。私有配置的文件类型、权限、UID 校验和原请求解码、65536 字节上限由未登记的 service 实现拥有，语义保持不变。适配文件仅允许审定私有入口的函数体变化，新增 import、导出、impl、trait、模块、unsafe、属性或未知宏不能借此放行。每个比较树都核对私有模块引用库存及实际反向消费者的 commands 路线；新增 SDK 包装、重导出或别名导入均使合同失效。已审定 commands 分派与唯一二进制入口以可信摘要固定，变化后须重新评审登记，不能假定目标分支先合入的新调用边无影响。固定 `wasm32-unknown-unknown` 不执行稳定的 `cfg(unix)` 路由，但仍须核对所有挂载、嵌入与 host 生成路线；新增或改写 production cfg 不享受此豁免。

边界回归同时使用真实 legacy 源码的正例与独立的带平台属性源码负例；后者不能随生产文件拆分而失去保守保护。完整仓库中的普通函数体变化及预览的注释变化都必须得到原五个完整 targets、inline/support 转交和三包 all-targets Clippy；Linux 代理、公共分派、身份或混合输入仍保留原必要范围。本地代理的命令兼容与跨 UID 成功、拒绝、权限和恢复由真实二进制及既有 Linux 进程夹具验证，不以同 UID 模拟替代内核主体校验。

完整 Game Client support 在原 library 与 strict Clippy 之外执行 legacy 服务、本地传输和本地命令三个完整 integration targets；一次性 GitHub-hosted Linux runner 还构建真实 binary，由 root 所有的有限时进程组执行既有跨 UID 夹具。夹具仅复制本次 binary 和合成材料到 root 拥有的临时目录，私有材料另置 `0700` 子目录，保留获准／未获准主体、不可读私有文件、用途／版本、撤销／过期和滴流后重连断言；信号、断言或清理失败均不能完成 support。CI 缺失 Linux／hosted 身份或提权能力直接失败，不降为跳过；本地普通用户只运行可执行的 Cargo 合同，明确报告跨 UID 未执行，完整验收仍需隔离 Linux 证据。该入口仅随实际 Game Client support 执行，legacy 快速合同的五个目标保持不变。

调用库存覆盖全部 workspace 的 Rust 宏来源及实际 include 输入，不能因导出宏没有普通依赖边就忽略它。raw identifier 按真实标识符比较，未知宏参数和模板保持保守；消费者依赖闭包出现未经审定的本地 proc-macro 时回退，不尝试分析任意常量生成的 Rust 源码。独立本地 commands 模块及标准断言中的普通字段、索引和值不作为公共 CLI 路线；本地断言宏覆盖、别名导入和 macro_use 不能借这个表达式例外放行。

可信 `ordinary_fast_support_contracts` 明确登记该 owner 的精确输入和可延后到 full 的反向 support 消费者。只有每项相关输入都命中合同、真实 target 可达、native/test-only 证明成立且无风险扩测时，才把 support 自身的完整 library 测试交给 inline，把自身及被延期消费者原默认 feature 的全部 `--all-targets` strict Clippy 交给 rust-baseline。独立 integration 测试输入也必须经同一证明，不能仅因属于 support 包就跳过 ordinary。仅由同一私有适配输入传递触发、且已登记的消费者完整回归留在 full；计划保留完整 affected 集合和具体转交、延后清单。直接消费者修改、路径加测、显式请求、混合未知输入、测试政策变化和旧能力均保留原必要检查，不把任意 support 包或目录整体变成快速范围。

该合同的性能目标是 workflow 创建到最终 required gate 不超过 600 秒，包含排队、checkout、缓存、编译、Clippy、全部所选测试与汇总。冷、暖缓存均按真实行为改动或新增回归取样，并核对 source/tested/policy、实际目标和完整用例回执。首次 CI 控制变更仍使用可信 BASE 范围；合成差异的诊断只证明候选计划和当前构建成本，不能代签真实 PR、P95 或把超时样本改列例外。

完整反向消费者集合继续写入计划作为影响证据。已证明的组件合同承接普通行为，上层完整集成回归放入 periodic-full；直接规则、手动必要加测、未覆盖输入及平台风险仍取并集。新能力下重型 Formal 始终由完整 profile 承接；原 Formal 风险推导保留旧 BASE 及其他普通覆盖边界，最终延期不能在快速层留下 world-history 等 suite。Launcher release 构建只在其真实输入或必要风险要求时加入快速图。WASM 保留适用消费者编译；仅当源码路由与 cfg 可证明输入不进入 WASM 构建时，才排除纯传递依赖带来的无关 WASM 检查，解析不确定时保留。

不可达证明同时核对实际 library、binary 与 build script 入口、静态模块挂载和嵌入数据消费者。原生源码可能经直接或传递的 Cargo build-dependency、proc-macro 或 build script 在 host 上生成便携目标的输入；这种路线必须保留平台验证，不能仅凭 native cfg 排除。自定义或无法解析的入口同样保守处理；包里存在 build script 本身不等于所有 library 源码都作为 host 生成器执行。

已选完整 library 的实际入口可以按本次差异证明平台不可达：每个 base/source/tested 比较都只能新增顶层静态常规模块声明，逐条带精确的 `#[cfg(not(target_arch = "wasm32"))]`，既有 token 保持不变；被挂载模块必须在同一比较中新建、具有唯一常规路径，并在所选 native target/features 中真实可达。只有这些新增声明及模块的影响可排除，不能因入口既有代码可供 WASM 使用就误算整个文件，也不能凭 native 属性忽略其他改动。已有声明或属性改写、删除、重新挂载既有文件、混合便携改动、未知宏/动态路径，以及 host 生成或其他便携挂载/嵌入消费者均保留原验证。此证明沿用已有可信快速能力，不放宽下述无属性 `static_module_additions` 的独立准入规则。

构建前直接生成 `cargo test -p <package> --lib` 或 `--test <name>`；strict Clippy 使用同一命令列表。独立模式不隐式追加 Viewer/snapshot harness。artifact 核对 package/target、请求及实际 features、默认 feature、最低 case 存在且非 ignored、计划与 source/tested 身份；缺目标、空执行、错误变体或遗漏最低用例均失败。最低名单不限制执行，所选目标的全部非 ignored 用例包括后续新增回归都实际运行。

同一 package、相同默认 feature 设置和相同显式 features 的快速目标可合并到一次 Cargo 构建调用，例如同时指定 `--lib` 与具名 `--test`；不同 package 或 feature 组合不合并。构建清单继续为每个目标分别保存并校验真实 executable、features、摘要及完整用例，不以命令合并消除任何目标或验收项。

修改测试或生产源码时，即使已有 feature gate 没有变化，也核对快速组合可达性；无法执行则扩测。新增共享测试只有证明静态 `mod/include!` 路由实际到达被选 Cargo target 且全部门控成立，才继续精简，未知宏、动态路径和断开入口保守处理。`native.rs/world/mod.rs` 保留已登记声明/导出的精确新增规则；精确 native cfg 也是声明的一部分，删除或替换已有 API、其他代码仍回退。

可信规则可为完整 library 目标登记 `static_module_additions`。它只接纳入口最外层新增的无属性 `mod name;` 及其可见性形式，要求源码实际新增于同一比较范围、使用唯一常规模块路径，且所选 target/features 能执行入口和新模块。此能力用于 Runtime `world/mod.rs` 与 Viewer `runtime_live.rs`，不放宽 `native.rs` 的导出合同。base/source/tested 的每一项实际差异均须通过；已有代码变更、模块删除/替换、`cfg/path` 属性、inline module、宏挂载或歧义路径不能借此准入。新模块内的门控和嵌入输入继续接受原有风险及可达性检查。

periodic-full/candidate-full 保留原 26 组、全部包级测试、Formal、扩展 features 和平台组合，并通过独立 `ordinary_full_fast_targets` **追加全部快速目标的相同默认 feature 设置与显式 features 组合**。默认 feature 的 full 不能抵扣仅在无 Wasmtime 下存在的行为。补充构建、Clippy 和测试均执行，产物按 feature 变体区分；验证补充后仍独立验证原库存，不能用补充 harness 顶替原 required。旧 BASE 缺此能力时仍使用旧入口。

该机制和本地测试不证明 hosted 十分钟/P95 达标。仍按完整排队、准备、编译、Clippy、测试和汇总的真实时长验收；本 CI 控制 PR 自身接受旧可信规则，候选不能先减测使自己通过。

#### 快速执行链路与专项 baseline

可信计划为已选中的组件快速目标声明 inline 执行能力。`oasis7-required` 保持固定逻辑 job 身份，每个执行分区在同一 checkout/工具链中完成真实测试构建与执行，复用现有 build manifest、最低 case、features 和非空执行核验；不再为这些目标启动 ordinary artifact producer，再上传下载到第二台 runner。旧计划缺少 inline 能力字段、包级回退和完整 profile 继续使用原 producer/consumer 路径。汇总仍核对实际选中的执行模式，不能将 selected job 的 skipped、cancelled、missing 或失败视为成功。

可信 `ordinary_partition_version=1` 仅允许目标 owner 恰为 Runtime 与 World State 的有界 native inline 计划按这两个 package 分到并行 runner。父计划保留完整 `ordinary_fast_targets`，`ordinary_partitions` 必须恰好覆盖其全部 package 且无重复；每个目标仍只有一个执行方。单 package、其他 owner 组合、旧能力和非 inline 计划使用 `all`，保持原执行方式；不扩成任意多包 runner 扇出。分区不能修改父计划或其 source/tested/policy 身份，也不能单独宣称整个 ordinary 组已通过。构建与消费入口重算分区的精确目标子集，绑定完整父计划、分区名和目标变体；未知分区、漏项、重复、跨包或 features 不符均失败。矩阵由同一可信计划生成，`fail-fast: false` 保留全部分区结果；最终 required 汇总等待所有选中矩阵成员成功。Clippy、架构声明图和反向消费者继续读取完整父计划，不随测试分区缩减。

有界 inline 计划通过 `ordinary_clippy_group=rust_baseline` 将 strict Clippy 移到已有并行任务，缩短 native 构建和测试前的串行路径；不新增 job，也不改变 target、features 或 lint 参数。两个执行组都读取同一可信计划，核对 source/tested 身份后再判断唯一执行方；迁移要求计划同时选择 `rust_baseline` 与 `oasis7_required`，任一组失败仍由原 required gate 阻断。已知能力中的非法执行组、缺失所需组或不适用 profile 必须失败；缺少该字段时保留 ordinary 组内 Clippy，未知整数能力与旧计划继续使用原执行路径，不能让两个组都跳过。full 保留原 Clippy 全集及精确快速变体补充。

可信配置的独立 `distfs_clippy_contract_version=1` 进一步允许 Runtime 与 World State 的上述 native 双包计划将 DistFS 组原有两条 strict Clippy 调用交给 `rust_baseline`：CAS/Formal Protocol 的默认 feature library，以及 DistFS 的默认 feature library 与 durability endpoint binary，完整参数和三项拒绝 lint 保持不变。只有完整父计划同时选中 `distfs`、`rust_baseline` 和 ordinary，已证明 `ordinary-targets` 反向消费者边界，且没有例外、Formal 或 WASM 扩展时，才能声明 `distfs_clippy_group=rust_baseline`。baseline 将这两条独立 Cargo 调用追加到原 Clippy 清单，DistFS 核对同一可信 selection 的 source/tested 身份和能力后才转交；DistFS 的四个原测试 target 仍全部按原单线程参数执行。具名行业 target 按上述可信精确库存独立选测，不从 lint owner 推导覆盖。版本与 owner 必须成对且合法，缺失必需组或不兼容计划失败。旧 BASE 没有新能力、独立本地组和 full 均保留 DistFS 原位 Clippy；不能借既有 ordinary Clippy 字段取得新转交权限。required gate 继续等待两个组，任一失败、取消、遗漏或跳过均阻断。预览也显式传递同一完整计划，统计包含 baseline 新增工作在内的最后完成时间。

完整构建清单通过核验后，未分区 inline 的独立 harness 可最多两个并行执行，全部 libtest 线程的预算按可用 CPU 分配。已验证的 package 分区使用一个 worker 顺序执行该包全部 harness，将当前可用 CPU 预算分配给正在运行的 libtest；既有较低的 `RUST_TEST_THREADS` 上限仍生效，不提供任意扩线程开关。经完整身份与实际库存核验的 library harness 优先派发，避免短 integration 推迟长库的完成；两类各自保持原顺序，未知或旧 capability 保留原调度。每个 harness 仍执行全部非 ignored 用例并独立核对计数、退出、超时及后代清理；不拆分或截短单个场景。任一失败保留原失败，停止启动待执行项并取消、回收本轮已启动任务；后续成功不能覆盖失败或取消，未执行项不能算通过。进程监督器的可选取消信号只作用于本次拥有的进程树，默认串行调用和完整 profile 保留原行为。分区的额外冷依赖编译、runner 排队和资源竞争仍计入真实端到端预算，不能直接用历史单机耗时推定提速。

调试与测试构建中的密码学热点沿用根 manifest 的精确 package/version 优化：可执行文件校验同时覆盖实际使用的两个 SHA-256 版本，重复私钥封套派生覆盖锁定的 Argon2 实现。仅提高这些依赖的 `opt-level`，保留 debug assertions、overflow checks、固定 KDF 参数、封套格式与完整行为回归；不通过降低安全参数或复用解锁结果缩短测试。Cargo 的 test profile 继承 dev profile，包级配置与继承按 [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#overrides) 执行。比较相同产品源码、features、runner 和冷暖状态下的编译加测试关键路径后判断收益，不能只报告函数速度或忽略新增编译成本。

`rust-baseline` 的格式、架构、RustSec 与文件约束继续执行。NewAPI accounting 仅在其真实 package/依赖输入受影响时执行；独立工具锁文件检查在相应 manifest/lock、检查器或共享构建输入变化时执行。选择来自可信计划及完整差异，不靠候选环境变量自由关闭；没有新能力的旧计划和完整 profile 保留原检查。周期 full 仍执行这些专项全部原命令。

架构编译分层遵循[主责合同](../../engineering/rust-governance/rust-engineering-contracts.design.md#trusted-local-architecture)，Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84。可信配置的 `architecture_build_policy_version=1` 仅对完整符合普通快速合同的直接输入启用 `ordinary-targets`；消费者重核目标 owner、source/tested、必需组和干净 checkout。全部登记 profile 的声明/依赖图持续阻断违规，额外编译以 `deferred_to_full` 记录，快速目标的真实构建、Clippy 与测试继续由当轮两个必需组执行。混合非普通输入、必要加测、共享控制、旧能力或 full 保留相应原矩阵，不通过诊断 `--graphs-only` 放行。周期 full 承担额外 feature/平台及上层组合的实际兼容编译。

已覆盖直接输入的反向消费者可以继续选择 `consensus/distfs/net/node`，同时按主责合同提供 `architecture.consumer_groups` 的精确包归属与反向可达证明。组数本身不迫使这些普通输入重做全部架构编译；直接路径命中的额外职责、未覆盖输入和显式加测仍保留原矩阵。消费者检查及其失败汇总继续执行。

Viewer 的完整 UI 测试与 software-safe 构建在输入、输出互不依赖时可并行，前置的共享 fixture/产物清理合同先串行完成。构建内部的 Cargo 命令顺序、目标和 features 保持，不能通过另设 target 目录绕过构建锁。只有 UI 与真实构建都成功且各自进程完成清理后，才执行使用新产物的浏览器安全验证；任何失败、超时、取消或遗留后代均阻断，不使用旧 dist 抵扣构建。并发收益计入同一 runner 的实际资源竞争，不能直接以两个历史耗时相减宣称 hosted 达标。

Viewer 的 Vitest UI 测试保留单 worker、逐文件独立 fork 与 jsdom，只复用 Node 编译字节码，不复用测试文件的可变模块或全局状态。worker 默认使用本工作区 `node_modules/.cache/vitest-node-compile`；调用者显式的 `NODE_COMPILE_CACHE` 路径和 `NODE_DISABLE_COMPILE_CACHE=1` 仍有效。沿用 Node 对源码和运行时版本的缓存校验、Vitest 对 V8 coverage 的禁用保护；缓存不可用时仍执行原测试。该目录不上传为跨 job 缓存，完整文件集合、原断言、Solid 模块身份及失败/清理合同保持。

候选策略可通过独立、只读的性能预览取得合入前数据。为避免常规 PR 每次推送都额外启动规划 runner 并执行全量历史克隆，Rust workflow 与标签触发入口的 `pr-fast-preview-plan` 仅在显式带有 `ci-fast-preview` 标签的非草稿 PR 上运行，并在 `rust.yml`、`ci-required-scope.json`、`ci-tests.sh`、`cargo-dev-architecture.json` 或实际构建驱动 `ci-test-artifacts.py`、`cargo-dev-architecture.py` 变化时启用产品预览（同依赖的迁移对照 `pr-governance-preview` 随之按需启动）；单独修改 planner 或 preview 编排的 Python 实现及测试时，执行其所属合同与实际 required 消费者，不额外构建产品性能样本。混合修改仍由执行配置或构建驱动启用预览。

预览的触发差异复用 required 选择器的 Git 身份与比较范围：workflow 显式传入事件 BASE、实际 PR source 和 tested commit；在验证真实 target 与 source 父节点、BASE 祖先关系及唯一共同祖先后，取共同祖先到 source、实际 target 到 tested 的差异并集，包含删除和重命名两端。main 单独前进不触发预览，真实 source 改动与 merge resolution 改动仍触发；身份或差异无法确认时报告失败，不能安静跳过。这个范围只决定是否运行诊断，不改变可信 required 计划。

预览以实际 tested tree 为每个样本构造一份仅注释不同的 baseline，benchmark 的 source/tested checkout 保持该 tested commit 不变。`runtime-world` 保留两个既有 Rust 输入，必须包含 baseline、rust-baseline、ordinary 及 Runtime lib、World State lib、reducers、物理 incremental 四个完整 target；额外组仅接受上述 `architecture.consumer_groups` 明确证明的 native 反向消费者。`game-client-private-unix` 只使用已登记的私有 CLI 适配输入，必须包含三个核心组、完整 game-client lib 与四个 CLI integration targets，以及自身运行转交、两项 support 消费者延后和原 all-targets Clippy 保留清单。`executable-image` 使用独立库的私有摘要输入，必须包含三个核心组和关闭默认 features 的完整 library target，用于实测组件编译隔离。各样本期望目标与 feature 组合由预览独立固定，不能从候选选测结果反推应有覆盖。

`ci-fast-preview.py` 按样本分别记录实际 PR BASE/source/target、分支共同祖先和触发比较范围，以及 benchmark 的 policy/source/tested、合成 BASE/tree、固定输入、全部组与 ordinary 分区，下游 runner 按样本与确定性 Git 身份重建比较对象。独立样本最多并行规划两路，各自使用临时 Git index 和独立输出路径，不修改 checkout、真实 index 或 ref。`plan-matrix` 只有在所有样本完整校验成功后才一次发布执行矩阵；不发布半份成功。每行绑定 sample、group、partition 与该样本的合成 BASE，恢复时重核行归属；selection、metadata 和结果 artifact 分别命名，结果名称及时间记录包含 partition，禁止跨样本或分区复用。Runtime 的额外消费者组仍全部执行，样本完成时间取所有分区及其余组的最后完成时间。缺项、未解释的额外需求或错误 feature 策略均不能被静默省略。

预览矩阵直接消费已验证计划输出的完整组列表，所有选中组并行运行真实 driver，不恢复缓存，记录准备、编译、测试与结果；每个样本从 workflow 创建到其全部所选 job 完成的时长独立验收，包含排队、规划和产物上传，不能把单个 job 的 runner_seconds 当成端到端或混用另一份样本的完成时间。不能只运行三个核心组而漏掉计划中的消费者。该预览测量当前 Rust 测试与编译成本，不证明一般差异覆盖、回归质量或 P95；它不是本 CI 控制 PR 的 authoritative required 计划，不生成 release 可接受的完整结果。metadata、日志和状态使用独立名称，不把预览成功混记为当前 PR 已按新策略通过。

baseline 保留必要治理、格式与一致性检查。文档路径按完整差异检查变更正文及受影响的索引、入站引用、删除/重命名和 authority 关系；无相关输入的子检查及时返回。Markdown block/html/link 共享有界、按精确文本缓存的 CommonMark 解析结果，fragment/可见文本查询复用纯解析结果；保留全局 ID/索引等必要不变量，不能只读取修改行。baseline 不自动附加 `--full-corpus` 历史内容报告；需要存量分析时显式运行，不能把其历史告警升级为当前无关改动的阻断。改变规则、范围选择或解析器时仍运行全部适用回归并核对完整适用文档集合。

单次系统设计追踪检查可复用本轮 Git 工作树与暂存区的变更映射，避免每读取一个引用目标都重复扫描全仓状态。每次调用重新取得映射，仍按原规则区分 unstaged、staged-only、删除、重命名及未跟踪文件；工作树恢复为 HEAD 而暂存区仍有修改时，读取暂存内容。内容缓存限于本轮首次读取，不跨调用复用，也不把本地可变工作区宣称为原子不可变快照。

未命中独立快速 target 合同时，`oasis7_required` 从可信 26 包库存选择受影响 package，对所选包执行 strict Clippy 和全部非 ignored `--tests`；便携 WASM 编译从原四包库存取实际受影响闭包。Viewer 子进程与默认 feature snapshot 变体只在对应真实消费者被选中时构建。support 的普通 profile 可能只执行 library 或指定 binary，不能据 Cargo target 存在就认定普通入口会执行它；测试政策变化时保留原 profile，并追加所选包的 `--all-targets` 及适用 full features。扩展 feature 集合为空仍执行这些 target，省略空的 `--features` 参数；空 feature 不表示没有新增用例。未知映射或保守 scope 恢复旧库存，不以空 package 列表通过选中的 ordinary 组。

新能力下 Formal 仅在完整 profile 构建其既有共用依赖闭包，并执行全部五个逻辑 suite 的完整 case；旧 BASE 保留原风险选择。逻辑 suite 按可信 `case_shards` 映射物理 shard，跨 shard 的完整 case 必须保留。artifact 把可信 BASE 最低库存与候选新增项合并，既有 case 身份、ignored filters、环境 batch、shell tests 与依赖不能由候选删除；缺预期 case 或新增所需 shard 不在可信计划内均失败。build、pack、run 绑定同一 authority 摘要、同一计划和实际 tested source。

Cargo 的测试名过滤只限制执行，不能当作编译范围缩小的证据。快速集合仍包含实际行为测试，不以 `cargo check` 或空 harness 代替；单个局部编译示例也不能证明所有普通 PR 已达到预算。

兼容的同 run、同 tested commit、同编译输入产物可复用；消费者核对工具链、平台、profile/features、测试清单与摘要，执行被选中的真实用例。缺产物、输入不符、零匹配、错误 feature/target 和意外跳过均失败。长历史与恢复场景保持原断言和操作长度，在周期层或相应定向验证中执行。

保持可信 main producer、只读 PR 和已有仓库缓存预算；快速 profile 默认只读，可信计划明确启用 ordinary main-fast 写入能力时，仅当前 main push 的完整 ordinary artifact producer 可保存 workspace library 缓存。完整 main profile 保留原 workspace 与工具缓存生产，共享原生依赖由独立 main schedule/manual-full 预热任务生产。旧 BASE 回退保留原权限分支，候选始终不能借 full 名称取得写权限。缓存缺失走同一正确冷构建。记录实际 Cargo fresh/rebuilt、恢复与准备成本，不把 cache hit 当作复用或十分钟达标；独立预热的覆盖与写入边界见[缓存设计](ci-tiered-execution.design.md#ci-install-cache-design)。

### 完整集合与结果汇总

对相同 source 和环境，令 R 为适用快速及必要加测集合，M 为从旧 PR 后移的检查，F 为完整回归集合，验证 R 与 M 均包含于 F。periodic-full 从全仓适用库存展开，不只选择本轮差异；必需的平台组合由平台矩阵执行。清单包含稳定检查 ID、真实命令/参数、前置、target/features 与结果，同名命令的不同环境不能互相抵扣。

完整计划展开可信 scope 的全部 26 组，包含组级 Clippy、依赖本轮真实构建的 Viewer auth browser smoke、Command Center App、Windows rollout、macOS 包合同及 fleet-health 三平台。普通产物、Formal 产物/物理矩阵、平台组和 `full-extended` 可并行，`explicit-full-gate` 等待所有组、ordinary producer，以及恰好一个成功的可信或候选 full producer。

`full-extended` 只运行 core 扩展 features、support 的完整 target/feature 组合、fixture smoke 与生产 mock guard，不重复整个 required 清单；它单独成功不代表完整回归。support 按真实 manifest 生成 `--all-targets` 调用，启用适用 full features；没有扩展 feature 或该轮没有 `test_policy_changed` 时也执行，保留不在普通 profile 中的 integration 等 target。`full-standard` 保留本地普通加扩展入口，并补齐原遗漏的 Clippy/browser smoke；`full` 再加 Formal，但两者都不能代签 hosted 的多平台集合。

PR 汇总只等待本 PR 计划的必要检查，周期汇总等待本轮完整集合。保留可靠 always 收口：规划失败、选中任务失败/取消/超时/缺失、应执行却 skipped 均失败；未选中可以 skipped。预期矩阵单元须全部终态，不能用其它单元成功或总测试数掩盖漏项。新提交取消的旧运行不为新 source HEAD 放行。

保留稳定 required check 名称，避免对整个必需 workflow 加 paths 过滤造成 Pending；依赖失败也必须进入最终汇总。`pull_request` 订阅 `opened`、`synchronize`、`reopened`、`ready_for_review`：Draft PR 跳过 `select` 及全部下游重型组以释放共享 runner 容量，同时 `required-gate` 仍运行并快速失败阻断，避免在 `strict: false` 分支保护下产生 `skipped` 通过记录；PR 转为 Ready 时由 `ready_for_review` 自动触发完整规划与验证。手动候选验证不替代 PR 必需事件产生的检查。若发布所需平台不在本轮完整集合内，补跑准确候选的相应范围后再消费结果，不能把“不适用”用于隐藏缺环境。

### 调度、并发与主线健康

schedule 使用 `17,37,57 * * * *`（UTC）执行轻量观察。短串行 admission 从实际 Actions run、jobs 与版本化 admission 产物判断：无活动 full 且距最近真实 full 启动达到四小时，才启动本轮完整图；没有历史记录且查询完整时启动首轮。失败和取消仍计入真实启动间隔，不每二十分钟重跑测试；错过多个时隙只补当前一轮。每轮固定事件 main SHA，所有 checkout、产物和汇总引用该值，保留 manual full。

PR 并发组按 PR 编号取消过期运行；`main` 分支的 `push`（`main-fast`）按目标分支 ref 共享并发组并取消被新合入覆盖的过期运行。为避免连续合入取消中间 `push` 运行后仅按最近一次 `event.before` 选测而漏掉前序提交的受影响验证组及 `ordinary-main-fast-cache-writer` 缓存更新，`select` 在 `main-fast` 下保持 `policy_sha=event.before` 不变，同时通过有界只读 Actions 历史查询将比较基准 `range_base` 向前扩展至有界第一父提交窗口内最近一次成功完成的 `main` `push` 运行提交（绝不晚于 `event.before`；历史缺失时保守回退到窗口内最早未验证 `push` 运行的父提交或 `event.before`），`baseline` 与下游组同步消费该扩展范围，四小时 `periodic-full` 继续做全量兜底。schedule 观察使用每 run 独立并发组，admission job 使用固定短并发组且不取消正在执行的判断。判断与上传启动资格在同一短锁内；已发布资格但 select 尚未启动也阻止下一轮重复启动。活动 full 超期只报告，不自动取消或重叠。candidate-full 按触发 ref 与固定候选区分，避免取消主线回归或把不同候选串成一个成功结果。完整扩展 job 的执行上限为 210 分钟，主线健康检查另以四小时观测单轮预算；这不等于整条 workflow 已证明在四小时内完成。超过容量目标时优化分片或资源，不无限重叠。需要硬性隔离 PR 容量时使用独立 runner 池，普通 concurrency 不等于平台优先级。

`ci-full-result.py health` 只读观察最近两天 main schedule 的实际 attempt，区分观察与实际 full，报告最近完整运行、最近终态、`last_successful_sha` 和 `commit_range`，缺数据保留 unknown。为覆盖旧 run 最近重跑，metadata 按两天窗口回看三十七天，并单独查询全部活动轮；范围覆盖 GitHub 的[三十天重跑期限](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/re-run-workflows-and-jobs)及[三十五天 workflow 运行上限](https://docs.github.com/en/actions/reference/limits)加两天观察窗，不把创建时间当最新 attempt 时间。

读取共享 120 秒、800 次请求上限，最多四个并发只读审计，预算不足保留 unknown。GitHub API 的只读 GET 仅在打开响应时得到 HTTP 502/503/504 时重试，每个请求含首次最多三次，退避为 0.5 秒、1 秒；每次实际尝试计入同一带锁请求计数，等待及后续请求共用原截止时间，额度不足立即失败。4xx、重定向、其他传输错误、损坏响应、字段缺失及合同断言不进入该请求重试；artifact 的 302 仍由独立无 token 的 storage 下载承接，storage 不进入该重试。

周期准入与健康观察的客户端可显式启用有界的整集合恢复：仅当各页结构、非负整数计数及唯一数字身份合法，但分页间 `total_count` 改变，或没有下一页且累计条数与总数不符时，丢弃本次全部部分结果，从原集合第一页重新读取，含首次最多三次。所有页、请求重试和集合重读共享原时间与请求预算，不恢复已耗尽额度。字段缺失、重复或非法身份、非法分页链接、分页循环、权限错误及其他校验失败立即拒绝，不借恢复扩大接受范围；最终仍不完整则本轮观察失败。默认分页客户端与发布资格验证保留一次完整读取的合同，不自动启用集合恢复。

集合读取失败记录固定、脱敏的诊断字段：接口与集合类别、页码、集合尝试序号、期望与实际累计条数、是否存在下一页及失败类别；未知值明确表示未知。不输出完整 URL、查询参数、响应正文或凭据。诊断用于定位失败，不是可消费的历史或资格；只有同一次集合读取通过全部分页和身份校验后，才允许发布可用历史、准入输出或完整资格。后续恢复成功不改写既有失败运行或实际超期间隔。

合法空跑必须有唯一、同 run/attempt/SHA/workflow 的 admission=false 产物，admission 实际成功，select 与 full gate 实际 skipped，且无矛盾的 selection/full receipt；缺失或冲突不能算健康。旧版本没有 admission job 的 schedule 保持原 full 语义。四小时 cadence 另留一小时观测余量，默认最近启动窗口为五小时、未结束单轮预算为四小时；缺最近成功、漏跑/过期、失败或任一未完成轮预算超限返回非零。新 pending/running 不清除上一终态失败。该观测是调度与主线健康信号，不代替完整 receipt 的发布 qualification，也不会自动重跑、取消或回退。

选择性重跑可能让 Actions 为继承的成功任务返回新的 job ID 和当前 `run_attempt`，但保留原执行时间与 steps，产物仍属于实际 producer attempt。admission=true 与 selection 的复用须绑定最新存在的对应产物、原成功 producer 以及同 run/SHA/workflow/event/branch 的完整后继 attempt 链；继承任务的执行时间和 steps 必须与原 producer 一致，并在各次重跑开始前结束。后继 API 未列出该任务时仍核查每个中间 attempt；实际重新执行、失败、歧义、过期、身份或时序缺证均拒绝回退。周期起点保留原 select 时间，不用重跑时间刷新四小时窗口；admission=false 不继承为新空跑资格，完整结果仍只接受当前 attempt 的 full-result。回溯沿用同一 API 预算。

admission 成功且上述证据完整证明未执行 full，但 workflow 随后失败或取消时，记录为控制失败：健康和发布资格仍失败，不刷新 full 启动时间或占用活动名额，后续观察可按原四小时条件恢复准入。未知结论、未完成状态、缺失或矛盾证据仍拒绝分类，不能用恢复路径掩盖实际执行。

命令接入既有 [CI 耗时回顾任务](../../../.agents/dot/tasks/ci-duration-review.md)的只读检查说明；文档没有新建外部定时器，也不证明巡检已经运行。调用者独立读取 Actions 事实，不能只靠迟到的 schedule 报告自身漏跑。平台调度可能延迟或丢失，规则依据 [GitHub schedule 语义](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule)。

失败记录关联最后成功 SHA 到失败 SHA 的提交区间与失败检查。产品回归、测试不稳定和基础设施故障分别处置；无法判断时保留未知。有限重试保留原失败和 attempt，不自动把曾失败的运行解释为稳定。真实严重回归优先修复或按现有授权回退，限制受影响范围继续扩大；不增加“所有 PR 等最新 full”的全仓门禁。成功/失败区间来自 Actions 与 Git，沿用已有任务承接方式，不新增状态账本或自动回退授权。

### 发布消费与权限

Release/Testnet/Mainnet/ECS 发布或推广入口先固定 package source，再通过 `ci-full-result.py verify --source <sha>` 读取可信完整结果。`explicit-full-gate` 在成功验证完整 needs 后生成 `oasis7.ci-full-result/v1` receipt，绑定 source/tested/policy、检查库存版本、检查定义摘要和实际 run/attempt；产物名为 `ci-full-result-<run_id>-<run_attempt>`，保留 30 天。

消费者只接受 canonical Rust workflow 在 main 的 schedule/manual full，核对实际 API run/jobs、成功的 `explicit-full-gate`、相同 source/tested SHA 与 policy/check-definition 摘要。默认完整查询最近 30 天相关 runs 和必要分页，按最新实际 attempt 顺序判断；`--run-id` 可指定准确运行，但仍须满足其当前 attempt、来源和完整结果。较新的同源运行缺当前 attempt 成功 receipt（包括 pending/running）、失败重跑、过期/歧义产物、API 缺页或未知来源不能回退旧绿色放行。读取结果身份与用于查询的 selection artifact 均绑定实际成功的 producing attempt，失败重跑不能借旧 selection 假称验证了另一个候选。

API 已证明为专用非 full 诊断或经上述严格核验的 schedule 空跑，不会冒充完整结果，也不影响已有完整证据；单纯缺少 selection 不能据此排除未知运行。实际较新的同源 full 失败、取消、未完成或缺当前 attempt 成功 receipt，仍不得回退旧绿色。

普通候选分支自行修改 workflow 产生的绿色和 `bootstrap_full` 均不能取得发布资格。验证尚未覆盖的候选使用可信 main workflow 的 manual full 与 `source_ref`，select 一次解析不可变 source，所有执行 checkout 同一 SHA；变更中的策略/检查定义不等于受信任的新策略。验证代码不获得发布凭据，PR 产物不直接作为特权入口的可执行输入。当前 ECS 包/冷启动 smoke 与其他发布验证继续执行，完整结果不替代它们。

默认发布来源必须属于已 fetch 的受保护 main 历史，可信 policy 同样核对该历史。Testnet 的既有固定候选合同由 `--allow-candidate-source` 保留，该参数只接受 `GITHUB_WORKFLOW_REF=<repo>/.github/workflows/testnet-packages.yml@refs/heads/main` 的调用；它只放宽 source 的 main 祖先要求，仍强制该候选的可信 main 完整结果、相同检查定义和其余验证。其他 workflow 不能借此参数发布候选。校验不要求已验证 source 等于随时前进的 main tip；旧保留版本的显式回滚继续遵守既有恢复合同，不把回滚改称新版本验证成功。

### 接线状态与验收

检查映射、baseline/构建/执行范围、完整库存、周期观察与到期准入、汇总与发布消费已在同一候选接线。本实现 PR 自身仍接受可信旧基线的必要验证，不先降门禁使自己通过。新接口随实际合入后的可信 BASE 生效；回退使用正常修复 PR 恢复已知选择与汇总，不在失败时自动跳过检查。

验收至少核对：

- 文档/普通代码/关键协议或持久化/共享依赖四类真实计划，删除与重命名、未知输入、解析失败和控制自改均保持正确边界。
- 快速/完整清单的真实命令和平台包含关系；故意漏掉 Clippy、browser smoke、平台单元或迁出的 case 能被识别。
- 失败、取消、超时、产物缺失、零匹配和意外 skipped 传播到对应汇总；检查全部成功才通过。
- 周期与 main push 并发、PR 连续提交、漏跑/超时、main 前进及候选 SHA 变化；不会重复启动整套重回归或使用旧结果放行。
- 发布正例及错误 SHA、非可信结果、缺平台、缺页/未知状态负例；保留原包 smoke 与明确授权的回滚路径。
- 实际 hosted 数据按[反馈口径](ci-tiered-execution.prd.md#ci-fast-pr-periodic-full)报告完整观察窗口、样本数、普通成功 P95、失败/取消与例外、阶段耗时、冷暖缓存、完整回归时长及重任务占用。零匹配、跳过和早失败不算性能达标。

观察已有 full 失败的处置和修复版本的真实完整结果后再确认迁出的保护。文档、Python 合同及小型真实 Cargo fixture 只证明各自范围；普通 PR 十分钟/P95、四小时内完成的容量和真实平台完整覆盖仍待 hosted 数据，不在本文预写验收成功。

<a id="ci-coverage-design"></a>
