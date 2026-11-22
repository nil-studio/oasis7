# CI 基础镜像

- 状态：设计已采用，分批实施与验收中
- 上游要求：[CI 分级执行 PRD](ci-tiered-execution.prd.md)
- 控制与验证：[CI 分级执行设计](ci-tiered-execution.design.md)

## 上游需求与问题

迁移前，`rust.yml` 的 `custody-node` 四个分片在临时 Ubuntu runner 分别安装 13 个相同系统包（含 gdb、gcc 和 pkg-config），然后下载上游预编译产物。PR #6023 的 [custody-node (2) 原始日志](https://github.com/nil-studio/oasis7/actions/runs/44297220098/job/134807710346) 显示微软 apt 源返回 HTTP 429，分片在测试启动前即失败。同一组开发包还出现在 Rust、编译指标、缓存预热、存储验收和发布打包流程。镜像将逐任务联网安装改为按镜像版本集中安装。测试内部的计划过期等时序问题沿既有缺陷独立处理。

主干[可信发布流水线](https://github.com/nil-studio/oasis7/actions/runs/44685362408)已发布镜像。迁移清单和剩余验收见 [#6031](https://github.com/nil-studio/oasis7/issues/6031)；单个作业是否已迁移或验收，以其 workflow 配置、固定 digest 和实际运行证据为准。

## 镜像结构与消费边界

Linux CI 共用一套 Ubuntu 24.04 底座、一份多阶段 Dockerfile 和一条可信发布流水线。按 job 实际依赖选择最小目标，避免浏览器和 Rust 工具链增加其他任务的镜像体积。

| 目标 | 增量依赖与消费者 | 验收边界 |
| --- | --- | --- |
| `runtime` | 原生运行库与通用执行工具。用于执行预编译产物和适合容器化的轻量检查 | 核对 ELF 动态依赖、实际子进程与测试结果 |
| `build` | 在 runtime 上增加 GCC、pkg-config 和系统开发包。用于需要本地 C 编译或链接的任务。custody 首批用 build，因为分片 0 的 workerd GDB 合同需编译 C fixture | 四个 custody 分片全部执行，保留 GDB、类型检查、产物身份和结果收集 |
| `rust-build` | 在 build 上预装仓库固定的 Rust stable、WASM nightly、组件和 targets。用于 Rust 编译、Clippy、编译指标和适用的缓存预热及打包任务 | 核对 rustc 版本、组件、targets、链接器、profile、缓存身份和实际产物消费 |
| `browser` | 在 runtime 上预装 Node、按锁文件安装的 Playwright、对应 Chromium 和字体。先用于 site-quality，再按实际需求评估其他 Web job | 核对锁文件、浏览器版本、非 root 启动和真实浏览器测试 |

Rust 或 Node 依赖仍沿用仓库锁文件和相应的缓存策略。预装 rustup 工具链减少每个 job 下载编译器的开销，Cargo registry 下载与缓存缺失仍按现有机制处理。预编译产物消费者按实际需要选择 runtime 或 build，包括两条 storage-recovery 路径。依赖宿主网络、挂载、内核权限、Docker、签名或目标环境的任务先核对平台合同。现有 WASM canonical builder、macOS 和 Windows 原生验证继续承担各自既定验收。

## 构建、发布与信任决策

独立工作流从受保护的 main 构建并发布到 GHCR。基础镜像以 digest 固定，记录 Dockerfile 与清单来源提交、目标架构、实际系统包及工具链版本、目标镜像 digest 和安全扫描结果。Rust 工具链合同或 Playwright 锁文件改变时，由发布流水线重建受影响目标，再以 PR 提升消费 digest。通过定期重建或安全更新触发候选构建，由更新 PR 审查并提升消费 digest。历史已采用 digest 至少保留到迁移验证结束并覆盖回退窗口。

发布 job 单独授予包写权限。普通 PR，包括 fork PR，只拉取已发布的可信 digest。接入前验证 fork pull_request 的无发布凭据拉取路径。若镜像可见性不满足此条件，先解决分发方案。镜像只含可公开分发的构建工具与系统包，发布工作流不使用普通 PR 产物或其缓存作为可信输入。镜像改动、digest 提升和 CI 门禁接线接受独立评审。

Actions 工具归档的兼容性还包含实际 archive path 和压缩方式；同一 textual key 不保证宿主与容器可互用。runtime 共用层预装 zstd，全部镜像 smoke test 验证 tar/zstd 往返；消费者使用可信 writer 的归档路径并保持原工具身份校验，正式日志仍须验证 exact hit。

镜像 digest 加入依赖系统库或工具链的 native 缓存身份，防止跨环境恢复不兼容产物。镜像拉取或依赖探测失败时保持 job 失败，按已有流程退回上一已验证 digest 或回退迁移提交。禁止以自动 apt 回退掩盖镜像缺件，让安装步骤重新进入常规路径。

## 迁移与验收验证

1. 盘点每个 CI job 的平台、安装入口、编译与运行依赖、宿主能力、权限和产物消费者，记录适用目标或保留理由。先验证 `custody-node` 四个分片，再分批验证两条 `storage-recovery`、`site-quality` 和一个实际 Rust 编译 job。每个批次独立记录已验证的环境与覆盖。在 GitHub Actions 的 `container:` 环境验证 `setup-linux-mold`、`setup-python` 和 `setup-node`，核对 `RUNNER_TOOL_CACHE` 挂载及写权限、实际运行用户和 sudo 假设。
2. 使用同一候选源码和测试选择，分别执行原 runner 路径与镜像路径。按 job 比较冷拉取和原安装耗时、总时长与失败类型，多次采样并报告样本数、中位数及尾部耗时。记录构建、扫描、发布频率与镜像大小，判断节省能否覆盖镜像维护成本。
3. 首批验收要求宿主平台合同、测试覆盖和真实结果等价，普通路径无系统包现场安装，且镜像引入的冷启动成本有可接受的实测结果。采用 rust-build 后，常规编译 job 无需现场执行 rustup 工具链或组件安装。候选镜像现场构建的结果可以证明测试兼容，需另用已发布 digest 验证冷拉取。对拉取拒绝、缺库与缓存不匹配验证可观测的失败及回退路径。
4. 普通 Linux 编译、缓存和指标采纳同一镜像后，再逐工作流迁移打包与验收。发布类任务逐一核对 SOURCEPLAN、BUILDINFO、签名边界和实际目标系统启动，保持 source HEAD 与被测产物绑定。任何依赖宿主能力的例外都记录在相应工作流设计。

### Storage Acceptance 冷构建拆分

独立 Storage Acceptance 的五个 Cargo 编译变体目前串行运行。预制镜像消除了临时系统库和工具链安装，仍有一次 90 分钟、一次 150 分钟冷构建耗尽预算，均未上传完整归档。150 分钟运行中，`formal-required` 耗时约 67 分钟，`runtime` 约 35 分钟，`endpoint` 约 9 分钟，`distfs-tests` 约 10 分钟。`storage-libs` 进行到 `oasis7_world_service` 后被取消。 [正式构建日志](https://github.com/nil-studio/oasis7/actions/runs/44650838134/job/136005023073)

首个并行候选拆成三个编译分区。分别运行 `formal-required`、`runtime` 加 `endpoint`、`distfs-tests` 加 `storage-libs`。分区内继续使用现有 Cargo 命令、features 及优化测试 profile，保留 opt-level 2、调试断言和溢出检查。不同分区可能重复编译依赖，验收同时记录单分区冷构建、关键路径和总 runner 时间。未取得实测前保留现有构建预算。

复用 Formal 归档汇总的身份和完整性校验。各分区保留相同的源与被测提交、运行 ID、镜像 digest、Rust 工具链、target、配置和编译环境。汇总验证预期变体、二进制、测试清单和文件摘要，拒绝缺失、重复、跨提交或编译环境不一致的产物。合成的完整 Storage 归档沿用现有消费者格式。网络注入和 ENOSPC 继续保留宿主内核准备：前者需要对实际流量所在网络命名空间配置 tc，后者需要真实小容量文件系统挂载；普通目录或进程内模拟不能替代。长期维护及 attempt-release 本身不要求 host PID：其原义务是同 UID 的真实父子进程、SIGKILL/回收与文件锁，以及同一真实文件系统中的硬链接、fsync 和物理计量。容器迁移必须逐消费者验证这些能力，不能把数据改放 overlay 或 tmpfs 来规避原测量。只有完整归档验证通过，才启动验收分片。

第一轮以同一提交和最小正式参数运行，核对全部分区、归档及现有分片的实际测试选择与结果。单分区失败必须使汇总失败；重跑仅复用身份匹配且已完整验证的分区。当前实施和验收证据跟踪于 [#6182](https://github.com/nil-studio/oasis7/issues/6182)。

### attempt-release 的镜像消费边界

仅该分片在预制镜像内运行完整 Python driver 与真实子进程，使用 Docker init、runner 的同 UID/GID 和默认 PID namespace，不增加 capabilities、privileged 或 host PID。一个 mode 0700 的 runner 文件系统目录单独 bind，TMPDIR 与采样 results 都在该目录内；预检核对宿主传入的 device/inode、容器目录身份和同 device，拒绝 overlay/tmpfs，避免资源采样只观察另一个盘。case 私有根仍由原 fixture 创建，P/R、attempt 和原字节之间的文件系统关系不变。上传只保留结果与诊断，显式排除该目录下 tmp/**（同时承载 HOME）的全部文件，避免失败时残留语料、native 原字节或进程私有文件进入结果归档。

启动原四阶段故障前，真实检查硬链接 inode/nlink、文件与目录 fsync、跨进程 flock、同 UID 的 proc marker/父子 PID namespace 和 SIGKILL/wait；再执行同一完整归档中既有 native original-loaded-bytes 测试，验证实际加载闭包及原字节。任何预检失败均拒绝进入验收，保存预检与失败 summary；不放宽 map_files/原路径的 device/inode 匹配，不通过增加 ptrace 权限绕过。原四阶段、P/R 与 pin 释放、writer kill 清理和默认 12 分片及汇总合同保持不变。

proc 预检必须先等待子进程完成 exec 后发出的显式 ready 字节（最多 10 秒），再读取同 UID、原 marker、PID namespace、PPid 和 RSS；Popen 返回本身不代替 exec 后就绪。ready 缺失、EOF、超时或任何身份检查失败仍拒绝验收，并真实 kill/wait 清理子进程。


dispatch 的 attempt_release_host_reference 默认 false；启用时新增同源、同 run、同完整 bundle 的宿主参考分片，汇总必须额外验证该结果与其 job 成功，未启用时必须 skipped，默认 12 组不变。只增加一次宿主 case，复用原三分区 producer，不另起生产或改写归档身份。镜像冷拉取、预检、原 case 和总耗时分别记录。同源 host/image 对照必须消费各自身份校验通过的完整归档；新 workflow 提交不得冒用旧 source/run 的 bundle。capacity、full-node 与内核故障消费者的迁移仍需各自真实验收，不由本分片通过代签。
