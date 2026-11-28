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
| `runtime` | 原生运行库、通用执行工具和 Node 24 可执行文件。用于预编译产物、轻量检查和需要 PATH 上 Node 的分析器 | 核对 ELF 动态依赖、实际子进程与测试结果，并以 UID 1001 离线执行 PATH 中的 Node |
| `build` | 在 runtime 上增加 GCC、pkg-config 和系统开发包。用于需要本地 C 编译或链接的任务。custody 首批用 build，因为分片 0 的 workerd GDB 合同需编译 C fixture | 四个 custody 分片全部执行，保留 GDB、类型检查、产物身份和结果收集 |
| `rust-build` | 在 build 上预装仓库固定的 Rust stable、WASM nightly、组件和 targets。用于 Rust 编译、Clippy、编译指标和适用的缓存预热及打包任务 | 核对 rustc 版本、组件、targets、链接器、profile、缓存身份和实际产物消费 |
| `codeql-rust` | 复用 build 底座并预装正式 CodeQL Rust 分析固定的工具链，以及 GTK、WebKitGTK 和 D-Bus 编译依赖 | 核对工具链来源、非 root 离线编译、已安装包及分析器使用的全部 pkg-config 模块。CodeQL bundle 随原分析作业选择 |
| `api-runtime` | 复用 runtime 底座，为只读 GitHub API 证据作业提供固定 GitHub CLI | 核对官方 Linux 发行资产 SHA256、非 root 离线 CLI 版本与 API 命令。调用者保留各自的认证、权限和证据校验 |
| `rust-ordinary-builder` | 复用 rust-build，为普通产物构建预装固定 Docker CLI 与 Buildx（可信 main 读写与 eligible 候选只读共享摘要） | 候选离线校验普通 UID、客户端路径和版本。可信消费者验证 runner 已有 daemon、原 canonical WASM recipe 与产物身份；候选只读消费者强验 UID 1001 与 Docker socket 拒绝 |
| `browser` | 复用 runtime 的 Node，在其上提供 npm、按锁文件安装的 Playwright、对应 Chromium 和字体。先用于 site-quality，再按实际需求评估其他 Web job | 核对锁文件、浏览器版本、非 root 启动和真实浏览器测试 |
| `rust-custody-wasm` | 复用 rust-build，在专用路径提供固定 Node 22 及 npm。容器内部仅向 UID 1001 开放 `/.cache/oasis7`，供现有只读工具归档按原相对路径恢复 | 非 root 离线验证 Node/npm、Rust WASM targets、实际 tar 解包与路径权限。原 Worker 和 bindgen 身份由消费作业核对 |
| `rust-viewer` | 复用 Rust 层和 Node 24/npm，按现 Viewer 合同固定完整 Chrome、agent-browser、Playwright 模块与字体 | 非 root 离线运行原 Chrome 探针，并用 Playwright 连接同一 Chrome 验证 site quality 入口 |

五个目标从同一 Node 24 官方镜像来源构建，构建时固定来源 digest 并记录在镜像标签。通用层提供 Node 可执行文件，browser 层提供 npm 及其锁文件依赖。镜像 smoke 验证离线工具能力，CodeQL 的 JavaScript/TypeScript 提取由实际分析 job 验证所用的正式摘要。

Rust 或 Node 依赖仍沿用仓库锁文件和相应的缓存策略。预装 rustup 工具链减少每个 job 下载编译器的开销，Cargo registry 下载与缓存缺失仍按现有机制处理。预编译产物消费者按实际需要选择 runtime 或 build，包括两条 storage-recovery 路径。依赖宿主网络、挂载、内核权限、Docker、签名或目标环境的任务先核对平台合同。现有 WASM canonical builder、macOS 和 Windows 原生验证继续承担各自既定验收。

Viewer 必需检查和历史性能报告共用 `rust-viewer`。可信选择作业用已选测 Git 对象核对镜像发布源码的浏览器包锁、浏览器准备代码、Rust 工具链与 wasm-bindgen 准备入口。兼容源码走固定摘要及独立只读 Cargo 缓存，确切版本的工具归档继续按原可信键读取。输入缺失或变动的历史源码沿用原宿主准备路径。业务源码继续按原选测版本，功能、安全、性能与站点质量测试保留各自原入口。

## 构建、发布与信任决策

独立工作流从受保护的 main 构建并发布到 GHCR。基础镜像以 digest 固定，记录 Dockerfile 与清单来源提交、目标架构、实际系统包及工具链版本、目标镜像 digest 和安全扫描结果。Rust 工具链合同或 Playwright 锁文件改变时，由发布流水线重建受影响目标，再以 PR 提升消费 digest。通过定期重建或安全更新触发候选构建，由更新 PR 审查并提升消费 digest。历史已采用 digest 至少保留到迁移验证结束并覆盖回退窗口。

发布分为通用、专用和应用 WASM 三组并行作业，每组在原 10 分钟时限内逐个下载、加载、推送并清理所属镜像。各组消费同一可信构建作业的扫描通过产物。原九个目标串行时在第八个推送期间耗尽预算。十个目标分两组时，专用组在下载 Custody WASM 时超时，Viewer 尚未发布，因此将这两个大构件移入应用 WASM 组。 [串行发布记录](https://github.com/nil-studio/oasis7/actions/runs/45599148910) · [两组发布记录](https://github.com/nil-studio/oasis7/actions/runs/45623494794)

发布 job 单独授予包写权限。普通 PR，包括 fork PR，只拉取已发布的可信 digest。接入前验证 fork pull_request 的无发布凭据拉取路径。若镜像可见性不满足此条件，先解决分发方案。镜像只含可公开分发的构建工具与系统包，发布工作流不使用普通 PR 产物或其缓存作为可信输入。镜像改动、digest 提升和 CI 门禁接线接受独立评审。

Actions 工具归档的兼容性还包含实际 archive path 和压缩方式；同一 textual key 不保证宿主与容器可互用。runtime 共用层预装 zstd，全部镜像 smoke test 验证 tar/zstd 往返；消费者使用可信 writer 的归档路径并保持原工具身份校验，正式日志仍须验证 exact hit。

归档运输库前置固定 `actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9` 所用 SDK 6.1.0 和 npm integrity 锁。可审源补丁仅增加 V2 的显式 archiveWorkingDirectory 与下载后、首次 tar 解析前的 beforeExtract 回调；默认不传选项时保持上游行为，V1 显式选用则拒绝，回调失败传播 ValidationError。原 raw paths、压缩方法、版本 salt 与 getCacheVersion 不变，不通过修改 GITHUB_* 环境伪造工作根。

第三方 SDK 生成库和许可证置于 `vendor/actions-cache-explicit-context/`，自有构建入口、源补丁及 provenance 保留在 `.github/actions/cache-sdk/`。npm 元数据没有 toolkit Git commit，provenance 如实记录为空；不能以 npm 字段缺失冒称源码证明。该构建包复用现有 vendored-source 的 Dependabot 精确例外，版本升级须与上游快照、源补丁、lock、provenance 及生成库同批审阅；不独立自动更新这份固定依赖闭包。构建使用固定 ncc 的 minify 输出，避免第三方生成格式的行尾空白，不手工改写字符串或放宽全局格式检查。清洁复现入口是在该目录执行 `npm ci --ignore-scripts --no-audit --no-fund && npm run build`，回执绑定输入与生成文件摘要，真实 SDK tar 四向及本地 V2 HTTP 回归只证明运输行为，不代表共享缓存上传。固定库通过有限动作入口服务两个原工具缓存producer；原权限、准入、6/12分钟和20 GB预算保持，生产验收仍以该源码的正式job收据为准。


镜像 digest 加入依赖系统库或工具链的 native 缓存身份，防止跨环境恢复不兼容产物。镜像拉取或依赖探测失败时保持 job 失败，按已有流程退回上一已验证 digest 或回退迁移提交。禁止以自动 apt 回退掩盖镜像缺件，让安装步骤重新进入常规路径。

## 迁移与验收验证

1. 盘点每个 CI job 的平台、安装入口、编译与运行依赖、宿主能力、权限和产物消费者，记录适用目标或保留理由。先验证 `custody-node` 四个分片，再分批验证两条 `storage-recovery`、`site-quality` 和一个实际 Rust 编译 job。每个批次独立记录已验证的环境与覆盖。在 GitHub Actions 的 `container:` 环境验证 `setup-linux-mold`、`setup-python` 和 `setup-node`，核对 `RUNNER_TOOL_CACHE` 挂载及写权限、实际运行用户和 sudo 假设。
2. 使用同一候选源码和测试选择，分别执行原 runner 路径与镜像路径。按 job 比较冷拉取和原安装耗时、总时长与失败类型，多次采样并报告样本数、中位数及尾部耗时。记录构建、扫描、发布频率与镜像大小，判断节省能否覆盖镜像维护成本。
3. 首批验收要求宿主平台合同、测试覆盖和真实结果等价，普通路径无系统包现场安装，且镜像引入的冷启动成本有可接受的实测结果。采用 rust-build 后，常规编译 job 无需现场执行 rustup 工具链或组件安装。候选镜像现场构建的结果可以证明测试兼容，需另用已发布 digest 验证冷拉取。对拉取拒绝、缺库与缓存不匹配验证可观测的失败及回退路径。
4. 普通 Linux 编译、缓存和指标采纳同一镜像后，再逐工作流迁移打包与验收。发布类任务逐一核对 SOURCEPLAN、BUILDINFO、签名边界和实际目标系统启动，保持 source HEAD 与被测产物绑定。任何依赖宿主能力的例外都记录在相应工作流设计。

### ordinary producer 的镜像与缓存配对

可信 main 的 `oasis7-test-build-trusted` 以可写与宿主 daemon 访问模式运行固定完整摘要的 `rust-ordinary-builder`（仅该可信路径持有缓存写准入与 Docker daemon 访问），保留原 selector 准入、45 分钟 job 边界、实际 Cargo 变体、features、分包格式及下游 required 执行。该叶子沿用 `rust-build` 的编译器、系统及 mold，仅补预装的 Docker CLI 和 Buildx 客户端。预检核对 Python、stable/nightly 工具链、组件和 WASM targets、native pkg-config 依赖，mold 沿原固定路径实际验证。该路径不执行现场 apt/rustup；候选宿主回退路径的真实宿主 SDK 回收保持独立，不借本迁移改选测或权限。

完整 ordinary 产物仍包含原 Docker canonical WASM 测试材料：`local_test_provider` 和 sensor、mobility、memory、storage 四个 m1 模块。GitHub runner 已提供默认 Docker socket；本消费者不新增 daemon、socket 挂载或 privileged。耗时 native 编译前，使用有界客户端调用验证实际 daemon 可达、版本可读及默认 context/builder 的 `docker` driver；预检不能只看客户端文件存在，也不启动或替换 builder。后续仍由原 canonical wrapper 选择实际执行镜像，生成 WASM、metadata、receipt，并由当前源码编译的 checker 严格验证源、模块集合与各文件摘要。不能设置 `IN_CONTAINER` 伪装 canonical 环境、改用 host-native fallback，或把 tiny Rust/ABI 探针算作这五个模块的验证。

同一 producer 作为镜像 dependency cache 的 writer 和后续 reader，使用单独包含完整 digest 的命名空间、原固定 Swatinem 动作及相同环境 hash；只在构建成功后，经真实归档 post 容量准入保存。新增 family 绑定原 Rust workflow 和 trusted job，只允许当前 main/source/实际 Git HEAD、一致 workflow SHA、适用 push/schedule 或 `run_mode=full` 手动事件，校验固定 image 与 GitHub provider。普通 PR、错误 job/source/digest/mode 均拒绝。命名空间进入现仓库容量/保留规划，不增加配额或绕过上传时点的检查。

普通 artifact 的既有 runner_image 身份同样不能因容器化改成 `sys.platform` 的通用 `linux`。trusted job 固定 Ubuntu 24.04，容器显式声明既有 `ImageOS=ubuntu24`，并在路径预检中用实际 os-release 核对 Ubuntu 24.04 后才进入构建；不修改 host consumer 期望或放宽 loader。镜像完整摘要继续单独绑定实际工具及 native 库版本。

普通 artifact 继续严格要求生产与消费的绝对 checkout 相同，且 Cargo 会将原二进制和 manifest 路径嵌入测试产物。可信 producer 固定使用现 Ubuntu hosted consumer 的 `/home/runner/work/oasis7/oasis7` 作为实际编译目录；容器将 runner 的同一 workspace 另行 bind 到该原路径，首个预检在 shell 内声明固定原路径并显式 cd，随后通过仅含相同路径声明与 cd 的 mode 0600 Bash 启动文件，让全部后续 run 步骤也在该路径执行。不能只依赖 defaults.run.working-directory 或 job env 的 host 路径值：[runner 的容器路径映射](https://github.com/actions/runner/blob/v2.338.2/src/Runner.Worker/Handlers/StepHost.cs)会把 host workspace 前缀转换回 `/__w`。启动前核对声明路径、脚本解析 ROOT、实际 cwd、默认 workspace 与 alias 的 directory device/inode，以及原 Git HEAD；拒绝 symlink、不同目录或任何身份不一致。不能只改 manifest、放宽 loader 的 checkout 比较或假造原二进制路径。该 bind 只提供已挂载 workspace 的另一入口，不增加宿主目录范围、capability 或 host PID 权限；真实容器路径及宿主消费仍须实测。

ordinary workspace 保留当前 v1 格式与宿主兼容输入的计算方式，脚本内容变更仍按原规则改变兼容 hash；镜像路径在兼容 hash 中额外加入完整 image，并在产物 manifest 记录 image，拒绝不完整摘要。宿主与不同 digest 不共享该 workspace 兼容组。原 `shared-native-cache` 仍服务尚未迁移的 full/support/production 与宿主消费者，不为预热 ordinary 另起广泛 full 编译。真实验收分别核可信写入与后续同环境恢复、实际 ELF 与所选测试消费、mold/profile/环境 hash、冷拉取和总成本；本地合同或一次 producer 绿色不能代签跨 run 精确命中。

当受信任选择计划判定本轮普通构建无需运行宿主 Docker canonical WASM 材料构建（`fast_plan_version=1`、`execution_profile` 属于 `pr-fast` 或 `main-fast`、`test_policy_changed=false`、不含 `ordinary_full_fast_targets`，且 `ordinary_risk_targets` 仅含无需 `local_test_provider` 的集成测试目标）时，`oasis7-test-build-candidate` 以只读模式（`cache-mode: read`、`save-if: false`）进入同一固定完整摘要的 `rust-ordinary-builder` 容器，配对恢复主干写入的镜像依赖缓存与 `ordinary-workspace-v1`。候选容器强制声明 `--user 1001:1001 --init` 与 `--tmpfs /opt/oasis7-mold:uid=1001,gid=1001,mode=0755,exec`，既使 `setup-linux-mold` 在无 root/sudo 条件下保持与可信 writer 完全一致的 `/opt/oasis7-mold` 链接器路径及缓存身份，又利用普通 UID 无 `docker` 附加组的内核权限边界阻断默认挂载 `/var/run/docker.sock` 的读写与连接；启动预检除核对 canonical 工作区路径、同目录 inode、Ubuntu 24.04、`tested_sha` 与 `selection.json` SHA-256 外，还显式校验 `1001:1001` 身份及对 `/var/run/docker.sock` 的 `AF_UNIX` 连接被拒绝（`PermissionError`）。凡 `test_policy_changed=true`、`candidate-full` 或其他需构建 Docker canonical WASM 材料的运行，`container.image` 保持为空并继续回退宿主 `ubuntu-24.04`（保留宿主 SDK 磁盘回收与 `ci-shared-native-trusted-v4` 只读依赖缓存）。

### Ordinary 原生测试构件消费

`oasis7-required` 的 v2 归档只将 `native-0` 至 `native-3` 迁到预制 Rust 镜像。保留原分区矩阵、每个分区的超时和 priority runner 路由。inline、controls 和旧版归档继续使用各自原执行环境。

构件中的 `CARGO_BIN_EXE_*` 和编译清单包含生产者的绝对路径。镜像只增加当前 checkout 到该原路径的同目录绑定，现有临时目录继续接收构件。预检确认两个 checkout 路径的设备及 inode 一致、Ubuntu 24.04 身份、UID/GID 1001 和 Docker init。仅解包和执行步骤使用该 checkout 路径及相同 `ImageOS`，原加载器继续验证源码、运行、路径和构件摘要。Bash 初始化只作用于这两个步骤，子进程可以按测试选择的目录运行。

原 mold 链接器 使用固定摘要校验后安装到本 job 可写临时位置。镜像为此链路提供库和 Rust 工具链，现有只读缓存权限、宿主依赖缓存和 inline 工作区缓存路径保持。需记录真实原生分片的完整执行结果、绝对路径加载、进程清理和镜像冷拉取；环境预检失败使原分片失败。

### Storage Acceptance 冷构建拆分

独立 Storage Acceptance 的五个 Cargo 编译变体目前串行运行。预制镜像消除了临时系统库和工具链安装，仍有一次 90 分钟、一次 150 分钟冷构建耗尽预算，均未上传完整归档。150 分钟运行中，`formal-required` 耗时约 67 分钟，`runtime` 约 35 分钟，`endpoint` 约 9 分钟，`distfs-tests` 约 10 分钟。`storage-libs` 进行到 `oasis7_world_service` 后被取消。 [正式构建日志](https://github.com/nil-studio/oasis7/actions/runs/44650838134/job/136005023073)

首个并行候选拆成三个编译分区。分别运行 `formal-required`、`runtime` 加 `endpoint`、`distfs-tests` 加 `storage-libs`。分区内继续使用现有 Cargo 命令、features 及优化测试 profile，保留 opt-level 2、调试断言和溢出检查。不同分区可能重复编译依赖，验收同时记录单分区冷构建、关键路径和总 runner 时间。未取得实测前保留现有构建预算。

复用 Formal 归档汇总的身份和完整性校验。各分区保留相同的源与被测提交、运行 ID、镜像 digest、Rust 工具链、target、配置和编译环境。汇总验证预期变体、二进制、测试清单和文件摘要，拒绝缺失、重复、跨提交或编译环境不一致的产物。合成的完整 Storage 归档沿用现有消费者格式。网络注入和 ENOSPC 继续保留宿主内核准备：前者需要对实际流量所在网络命名空间配置 tc，后者需要真实小容量文件系统挂载；普通目录或进程内模拟不能替代。长期维护及 attempt-release 本身不要求 host PID：其原义务是同 UID 的真实父子进程、SIGKILL/回收与文件锁，以及同一真实文件系统中的硬链接、fsync 和物理计量。容器迁移必须逐消费者验证这些能力，不能把数据改放 overlay 或 tmpfs 来规避原测量。只有完整归档验证通过，才启动验收分片。

第一轮以同一提交和最小正式参数运行，核对全部分区、归档及现有分片的实际测试选择与结果。单分区失败必须使汇总失败；重跑仅复用身份匹配且已完整验证的分区。当前实施和验收证据跟踪于 [#6182](https://github.com/nil-studio/oasis7/issues/6182)。

### Storage producer 的构建中诊断

`storage-required` 复用原生 `BuildObservation` 与资源观察器。每个 Cargo 上下文记录 UTC 开始、最近收到的 `compiler-artifact` 单元与累计数、结束和真实退出码；完成单元不表示当前活跃 rustc 的内部阶段。每秒只读采样可用内存、cgroup 计数、固定 Cargo 身份与后代的 CPU/RSS，并记录同一构建盘可用字节，每 20 秒输出摘要。RSS 是快照求和，可能重复共享页或漏短峰；未知值保留为空，心跳和 Pending 都不证明构建推进、OOM 或成功。

runner 失联时尾上传和日志端点都可能不可用，因此仅明确绑定同一 source 的可信 main 手动 dispatch 在构建期间另存有界白名单 JSON 快照；其他 ref 保留原 shell 构建，不进入带 runtime 凭据的 Node action。上传复用与现行 workflow 相同固定提交的官方 `upload-artifact` 发布包，下载到 runner 临时目录并校验固定 SHA-256；不把第三方 checkout 加到候选源码，不新增 SDK、PAT 或仓库写权限。官方 action 的 runtime 凭据从 Cargo 环境剥离，不写入快照或输出环境/参数；同 UID 的进程不是凭据隔离边界，因此不能把该路径开放给未受信候选。报告读取使用 no-follow 文件描述符和有界读取，再按固定字段、类型及字符串上限投影，不上传任意 JSON 字段。每 5 分钟至多发起一次、同时最多一个上传，单份不超过 256 KiB，最多 31 份（含开始与结束），保存 2 天；总 payload 不超过 7.75 MiB，另有一次约 4.5 MB 官方发布包下载及压缩/协议开销，常驻一个 Node 编排进程、上传时额外一个 Node 进程；其实际 CPU/RSS 尚须在 Linux 诊断中记录，不能按 payload 大小当作进程内存上限。下载及每次上传各有独立 30 秒上限，构建开始前下载与结束后等待最多各占用 30 秒原 job 预算；下载或上传失败只告警，不能重试构建、放宽选择或改写 Cargo/pack 退出结果。

artifact 名字及快照绑定 source、run、attempt、component 和递增序号，诊断不是可消费的 native component，也不参与 bundle 身份汇总。只有官方上传成功返回且可独立读取的 artifact 才证明该快照已保存；最后成功快照与失联之间仍可能缺失，网络或进程同时中断时不能保证保存事故瞬间。不得以诊断到达、构建结束或旧 source 的成功组件代签本轮完整归档及默认分片验收。首次实际诊断必须记录上传次数、大小、失败及时间开销；不改变既有 runner、150 分钟预算、profile、缓存和覆盖。

手动输入 `diagnostics_probe` 默认 false，正式默认分片与汇总继续完整运行。显式启用时先由无 checkout 的 2 分钟准入 job 拒绝非可信来源，避免全 skipped 被误读为验证通过；仅可信 main 的 `diagnostics-probe` job 在 5 分钟内使用固定官方 Node action 调用相同 uploader 主函数，以短 Python 进程和原资源观察器代替 Cargo；不构建或合并 native component，不运行正式验收汇总。快照额外带 `probe_only: true`，正常 producer 为 false；两者始终 `diagnostic_only: true`。probe 必须取得含已完成 fixture 与真实 Linux 资源观测的快照官方 ACK，否则失败；生产 action 保持 best-effort。该短实测还须独立下载已确认 artifact，核 ZIP/成员摘要、source/run/attempt、白名单和真实观察器指标，不能凭控制台 ACK 或 workflow 绿色代签。短实测通过后，完整长构建仍需单独授权与同 source 的全新组件。

### attempt-release 的镜像消费边界

仅该分片在预制镜像内运行完整 Python driver 与真实子进程，使用 Docker init、runner 的同 UID/GID 和默认 PID namespace，不增加 capabilities、privileged 或 host PID。一个 mode 0700 的 runner 文件系统目录单独 bind，TMPDIR 与采样 results 都在该目录内；预检核对宿主传入的 device/inode、容器目录身份和同 device，拒绝 overlay/tmpfs，避免资源采样只观察另一个盘。case 私有根仍由原 fixture 创建，P/R、attempt 和原字节之间的文件系统关系不变。上传只保留结果与诊断，显式排除该目录下 tmp/**（同时承载 HOME）的全部文件，避免失败时残留语料、native 原字节或进程私有文件进入结果归档。

启动原四阶段故障前，真实检查硬链接 inode/nlink、文件与目录 fsync、跨进程 flock、同 UID 的 proc marker/父子 PID namespace 和 SIGKILL/wait；再执行同一完整归档中既有 native original-loaded-bytes 测试，验证实际加载闭包及原字节。任何预检失败均拒绝进入验收，保存预检与失败 summary；不放宽 map_files/原路径的 device/inode 匹配，不通过增加 ptrace 权限绕过。原四阶段、P/R 与 pin 释放、writer kill 清理和默认 12 分片及汇总合同保持不变。

proc 预检必须先等待子进程完成 exec 后发出的显式 ready 字节（最多 10 秒），再读取同 UID、原 marker、PID namespace、PPid 和 RSS；Popen 返回本身不代替 exec 后就绪。ready 缺失、EOF、超时或任何身份检查失败仍拒绝验收，并真实 kill/wait 清理子进程。


dispatch 的 attempt_release_host_reference 默认 false；启用时新增同源、同 run、同完整 bundle 的宿主参考分片，汇总必须额外验证该结果与其 job 成功，未启用时必须 skipped，默认 12 组不变。只增加一次宿主 case，复用原三分区 producer，不另起生产或改写归档身份。镜像冷拉取、预检、原 case 和总耗时分别记录。同源 host/image 对照必须消费各自身份校验通过的完整归档；新 workflow 提交不得冒用旧 source/run 的 bundle。capacity、full-node 与内核故障消费者的迁移仍需各自真实验收，不由本分片通过代签。


### Trusted full regression

`rust-full-regression` extends `rust-custody-wasm` with clang, LLVM, and
the pinned Docker and Buildx clients from `rust-ordinary-builder`. The
image carries no Docker daemon. Publication uses the existing specialized
group with the same source, artifact, scan, and ten-minute requirements.
The candidate smoke verifies the toolchain and clients offline as UID 1001.

The trusted job can adopt the published digest after a Linux witness
verifies the canonical Provider build, receipt, and checker with the
original source and Docker workspace path. Adoption preserves
`ci-tests.sh full-extended` and the 210-minute budget. Verify npm, Trunk,
wasm-bindgen, and sentinel archive path and version compatibility before
moving their writers. Candidate full regression retains its current
runner until its canonical build can use an isolated Docker boundary.


#### 可信全量测试的宿主缓存兼容

四个角色保留原宿主路径、SDK 归档成员和版本。归档安装工作目录固定为
`/home/runner/work/oasis7/oasis7`，Linux X64 使用原 `zstd-without-long` 与
`enableCrossOsArchive=false`。容器内路径或解包后的内容摘要不能替代 SDK
版本输入。

| 角色 | 原始缓存路径 | 键与保存条件 |
| --- | --- | --- |
| npm | `/home/runner/.npm` | 原 setup-node 对 Viewer lockfile 的 `glob.hashFiles` 结果，保留 `node-cache-Linux-x64-npm-`；原主键精确命中时跳过 post 保存 |
| Trunk | `/home/runner/work/_temp/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz` | 原 Linux/X64、固定版本和安装脚本的 GitHub `hashFiles`；安装仍核 SHA 和实际工具版本 |
| wasm-bindgen | `/home/runner/.cache/oasis7/wasm-bindgen-cli/<lock-version>` | 原 Rust、锁定 CLI 版本和两个工具定义文件的 GitHub `hashFiles`；原可信 main、未精确命中、准备成功后保存 |
| sentinel | `/home/runner/work/_temp/cache-permission-sentinel` | 原固定 `oasis7-cache-permission-sentinel-trusted-v2`，只写入明确的可信 main 见证 |

离线角色模块消费实际 metadata 提供的摘要和原主键，拒绝替换根及降级
匹配；它不会计算 setup-node 的 lock 文件摘要或授权保存。后续接线要求
以实际运行的原 metadata 生成器锁定这些值，再用 SDK V2 解包前检查逐角色
限制成员类型、后缀和字节上限。每个真实 post 保存仍需绑定该 job、source、
ref、原动作结果、实际归档大小和仓库预算，拒绝不能触发预约。离线 tar
夹具不证明缓存 API、实际历史版本命中或正常 main 保存。

完整容器入口需预置当前 Rust 稳定版和锁定夜间版、WASM targets/rust-src、
Node22/npm、当前 mold、clang/LLVM、固定 cargo-deny、能执行原 venv 合同的
Python，以及 Trunk 与 wasm-bindgen 的原版本校验。Provider 如从原可信
宿主构建阶段交接，应同 run 锁定真实 source、canonical recipe、构件清单
及 SHA，容器使用原 checker 验证后才接收；候选不获得该宿主权限。

### 有限归档 Adapter 的消费边界

有限入口位于 `.github/actions/portable-cache-transport/`，直接导入上述固定 SDK，不另行安装依赖或打包自有逻辑。它只接受原 `worker-build` 与三种 `browser-archive` component，由原 metadata helper 派生 key、root、条目与字节上限；调用者不能指定 cwd、raw paths 或 fallback keys。SDK 使用固定宿主 canonical 工作根，保留原归档相对成员和缓存版本，不改 `GITHUB_*`。执行前核对实际 checkout HEAD、工作区和动作目录 inode、可信 main workflow/job、V2/read-write 模式与 UID/GID；save 仍依赖原 helper 的 REST/库存/20 GB 准入及测量结果，不新增授权路径。

恢复回调先绑定 exact matched key，再在解包前以 no-follow/nonblocking 描述符检查实际下载归档。有界 zstd 流覆盖 header、PAX、padding 和 payload；仅允许精确旧 prefix 内的普通文件及目录，拒绝链接、设备、sparse、未知 PAX、重复、路径穿越、文件目录冲突和当前目标 symlink。检查器沿现有 process runner 的 60 秒边界与进程回收运行；该边界不是同 UID 沙箱，也不保证硬终止后仍可执行用户态清理。SDK 缺失回调或检查失败不得作为成功命中。browser-archive-cache与worker-build-cache分别以正式runtime3984与rust-build068f（含原固定Rust1.96与zstd）固定摘要运行，使用此有限入口。两个job保UID/GID1001、init与原6/12分钟/main/workflow/source守卫；原mixed Docker launcher和BASH_ENV分派删除，不维护两套执行路径。


两producer只把原runner workspace/temp同目录绑定到固定canonical路径，Worker另把该job已有私有temp绑定到原cache parent，不挂home范围或授daemon权限。预检核对真实dev/inode、checkout SHA、UID、GNU tar/zstd及Docker socket不可连接；runner的实际cwd可以是同inode映射路径，收据记录observed_cwd，SDK始终明确使用canonical archiveWorkingDirectory。原GITHUB_*不重写。Browser命令显式原cache-base，Worker命令显式原root，因此rawpaths/getCacheVersion不变；Browser仅输出原准入report已有upload_bound_bytes，save必须绑定对应原admit输出。

Worker的mold仍由原固定setup.py核对原SHA并执行真实ELF smoke，安装于原owned /opt/oasis7-mold/3.0.0-v1。此job停止写辅助mold archive，避免默认Node cwd变化污染原宿主同key；其它job的mold cache策略不变。源码/工具预检与准备均计入原12分钟，后续正式日志记录setup-seconds；单次观察不作因果提速结论。四种原工具root各执行host默认/container显式cwd四向实际SDK tar回归，缓存API/version/exact hit与实际main准入及sharedsave另行核对，自然exacthit跳过save不能冒签新上传。

### Release Web 的 Node22 headed leaf

#7342 的 `rust-release-web` 继承 `rust-viewer` 的 Rust、Chrome、字体和固定
浏览器 npm 闭包，再复制同一固定 `custody-node-tools` Node22/npm 到
`/opt/oasis7-node22`，并预置原门禁所需 netcat-openbsd/Xvfb/xauth。继承的
Node24 action runtime 保留，生产脚本显式选择 Node22 CLI，不改变其它leaf。
无daemon、socket或新增capability。构建接收实际Ubuntu/Node24/Node22/Rust
父镜像digest及当前稳定/夜间工具链，保原可追溯labels。

构建后在network-none、UID/GID1001的容器内执行真实Node22/npm检查、稳定
wasm32编译和Xvfb中的headless=false Chrome页面回归；不以headless替代
原发布headed门禁。原target扫描/同run构件和main可信发布链保持，新增
release-web独立publish分组仍10分钟，避免给现app-wasm组合追加串行上传
使其越原预算。此片只发布工具，不切换writer/readers或改变其资格/缓存；
正式digest发布后再同批接入三个job，消费PR继续承担22/25/25分钟、历史
tag工具兼容、独立digest缓存族及原20GB/保存准入验收。

### Web 发布门禁与依赖 writer 的镜像批次

以下为 #7342 实施合同；正式leaf已发布，三job接线仍是待合入候选。

`rust-cache.yml::web-wasm-cache` 与 Release/Mainnet 的两个
`release_gate_web` 是同一 Web 工具闭包的 producer/consumer：保留 Node22、
Rust1.96、wasm32 target、Trunk0.21.14，以及原 Xvfb headed 门禁。writer
预算22分钟、两个 reader 各25分钟；原构建、测试、报告和发布资格检查不变。
本批不包含 shared-native writer 或其宿主消费者。实施和实际验收见 #7342。

现 rust-viewer 包含 Chrome/浏览器闭包但为 Node24，不能冒称满足原 Node22
要求。专用 rust-release-web leaf 复用该浏览器与 Rust底座，使用同源固定
Node22/npm，并预置 netcat-openbsd、xvfb、xauth；不含 Docker socket、额外
capability 或新的动态权限。正式发布不可变 digest 后才接生产消费。

两个发布 reader 的镜像选择由执行工作流的可信 plan 决定：以精确
checkoutSha 对照冻结镜像构建源的 rust-toolchain.toml、浏览器 helper/进程
helper、浏览器构建时使用的 apt/DNS helper、浏览器 npm manifest/lock 与
bindgen helper 的 Git blob。所有文件
须为存在的普通文件，目标 commit 及输入须完整。材料变化、缺失、symlink
或未知 commit 均继续既有 host 分支；仅产品源码变化可使用同一镜像。
兼容检测不替代 sourcePlan、原资格核验或允许候选保存缓存。

新镜像 writer 和兼容 reader 只能使用独立完整 digest 的 cache namespace
及真实环境 hash，不沿原 release-packages-web-wasm-trusted-v3 声称同ABI。
旧tag host分支继续只读原旧族，原对象按已有维护保护规则保留；切换后
不再重复预热旧 Web 族。新增族必须贯通原 main/workflow/source/job guard、
20GB库存、post保存准入及maintenance识别，不新增无界双warm或人为cache
miss。原 finite portable-cache adapter 的四角色不包含 Cargo target。

实际验收分列工具/真实 headed-WASM闭包、生产 writer 准入与保存、reader
恢复后的原门禁及历史host fallback；短fixture或sharedcache exacthit导致的
save跳过不得冒签新增上传。正式发布后的新命名空间首次miss如实报告。

本批消费固定正式 `ci-rust-release-web@sha256:f47cf4a0ce572de9b871475aed4e38c35fc7e6e128dda93a7d96e50480e03c59`，
镜像源 d5319297d754a9c7d4de347a2ce8e1a20ba43591、可信发布
45965554793/release-web140396891588已成功，2m48/10m。源码兼容同时
核 executing workflow 与目标tag的完整Docker/build/package清单、browser
Chrome/字体/apt/DNS/npm材料与编译器三项env；不信
候选自报兼容。SOURCEPLAN JSON及原资格核验保持，路由bool单独输出。

镜像激活仍以Node24满足原browser helper合同，后续npm/Web业务选择
Node22；native agent CLI从旧npx fallback0.37.1变为正式canonical0.43.0，
须以未修改参数/断言的原headed Web业务验收，不能仅靠DOM工具smoke。
历史材料不兼容保旧hostroute。镜像不执行宿主stable-only normalize，
避免卸载其不可变nightly；原host分支保此步骤。三个镜像分支沿原Web
完整hash选择集CARGO/CC/CFLAGS/CXX/CMAKE/RUST，并纳入完整
RUST_RELEASE_WEB_IMAGE；host分支不注入哪怕空的新RUST变量，保原v3
指纹。原Web直接stable Cargo不沿native无关的OASIS7_WASM扩展伪造命中。

镜像只运行原固定mold setup.py/SHA/ELF验证，不从这些job写宿主mold
辅助archive；host分支原composite不变。该非业务缓存策略变化需单列cold
成本，原Web主族、20GB/post-preload及历史v3维护保护不变。有限portable
adapter仍只承担原四种下载归档，不接受Cargo target。

writer也必须在任何构建/cache之前通过与reader相同的冻结image-source材料
校验；不能因固定镜像仍可运行就把漂移源码用旧工具写入同family。镜像
分支first checkout使用完整历史/内容以取得实际发布commit；历史host分支
继续原depth1+sparse linker checkout。镜像source不匹配直接拒绝，不能静默
跨ABI保存。当前完整Docker/build文件采用保守整体比对，相关材料变化
须更新受审镜像/pin；不得用省略工具输入换取命中。

The trusted full-regression job uses the image for npm dependency
installation, the original `full-extended` command, and the governance
report. GitHub cache actions remain on the host for npm, Trunk,
wasm-bindgen, and the sentinel, retaining their original keys, roots,
and permissions. Host Rust setup remains for the existing cache action
and tool preparation. Native Cargo restore uses the separate read-only
image identity, so the prior host target cache cannot enter the image.
The runner mounts only the workspace and selected tool data. The
original local Docker socket is bound for the trusted test command
alone after checking the trusted execution identity. Candidate full
regression continues through the original runner.

镜像材料集合只包括实际构建/安装闭包：Dockerfile/build、两包清单、Rust
工具链、viewer-context 内四个 Python helper 与 npm manifest/lock，以及
bindgen 安装定义。`agent-browser-lib.sh` 与 `viewer-web-dist-contract.sh`
没有复制进镜像：前者从当前完整 checkout 调用已激活的浏览器 CLI，后者
描述同一产品源码生成的 Web 产物布局；两者由同一 tested SHA 构建/执行，
不与镜像内一份旧脚本交换数据。因此其产品路径调整不属于工具镜像输入
漂移，不以定向字符串规范化或更换发布来源规避检查。原 CLI 参数与功能
合同继续由该 tested SHA 的原 headed 门禁核验；实际镜像输入任一变化、
缺失或软链仍拒绝镜像，完整 Docker/build 保守比对及三编译器环境保留。

### Storage 诊断探针准入

`diagnostics-probe-admission` 只执行 Bash 内建比较，使用固定 runtime3984
和 UID/GID1001/init；不 checkout、不安装工具、不访问缓存/API或构件。
增加的 packages read 仅用于拉取正式镜像，不持久化凭据。原2分钟预算、
opt-in 和 repository/event/main ref/workflowSHA=sourceSHA 四项拒绝脚本
保持原样；预检确认实际非root Bash5。它只为后继运输探针做来源准入，
不执行 Cargo/Storage 矩阵，不产生验收passed或授予新的写权限。镜像拉取
成本记在该2分钟job内，独立shell夹具不代签自然main准入或Storage验收。

### Pages 完整作业环境

Pages 的单一 deploy job 使用固定 runtime49a、UID/GID1001/init，以及原
Bash shell。四个站点质量脚本只需 Python标准库、Node离线测试和基础工具；
configure-pages使用Node24，upload-pages-artifact使用GNU tar及其原隐藏文件/
.git/.github排除规则。保留手动触发、github-pages环境/URL、5分钟、原
contents/pages/id-token权限和四动作顺序、site路径及默认github-pages
artifact名；仅增packages read拉取正式镜像，不扩大站点部署授权。

迁移实测只用受审隔离host/image两lane、pages read、enablement:false和
独立artifact名，核原site的内容、相对路径和mode。不得在该测试调用
部署动作、索取OIDC或启用Pages；只读A/B运输不代签真实生产部署。

### Release 资格完成后的发布环境

Release `publish-release` 使用正式api-runtime ba7c、UID/GID1001/init和原
Bash语义；只增packages read拉取镜像，保contents write、原10分钟、上游
资格、两个产物下载、七文件SHA256、tag逐层解注释/精确SHA、三平台
SOURCEPLAN与BUILDINFO绑定和最终softprops动作/文件清单。业务shell仅
使用gh/jq/GNU coreutils；action各自使用runner Node24，镜像自身的
/usr/local/bin/node也以绝对路径实测，不借action Node冒认工具闭包。

只读验证复用同一历史成功run/source的七个真实exact产物，核ZIP摘要及
原metadata格式后执行原checksum/tag检查；缺字段原逻辑应拒绝，不生成
或升级receipt。受控错误source用例不授资格，不调用softprops、OIDC或
发布。ECS依赖oss2/STS的供应链预制是独立后批，不在此作业现场pip安装。

### ECS OSS publisher SDK leaf

ECS `publish-preview` 的 OSS SDK 运行时采用独立 `ecs-oss-publisher` leaf，继承已发布 API runtime 的完整 digest；不改既有 Dockerfile/build.sh，避免改变 Release Web 的镜像材料身份。供应链和消费者分别交付，消费者只有在取得正式 digest 与实际能力证据后接线。统一 CI Base Images 增加唯一目标及独立发布组，保留原 12 个目标的构建、全部 OS 扫描、同 run 归档和可信发布，原 40 分钟构建及 10 分钟发布预算不变。

`docker/ci-base/ecs-oss/artifacts.json` 固定 Python3.12/Linux amd64 的 15 个 SDK 发行物（包括 `oss2==2.19.1`、`crcmod==1.7`）和三个构建工具的完整版本、官方 PyPI URL、字节数及 SHA256。下载阶段逐项校验；其后的 wheel 构建和安装以 `RUN --network=none`、`--no-index --no-deps --no-build-isolation` 执行，不允许 PEP517 另取未锁工具。最终层只复制 SDK 专用只读 venv，不包含编译器、构建工具或 pip。

私有父镜像仅由新目标使用只读 packages token 登录拉取；其余目标的 token 为空且不进入该登录分支。临时 Docker 客户端配置不进入上下文或 Docker ARG，构建前清除 token 环境。实际受控 fork PR #7509（head `eng-cc/oasis7-ci-validation`、base `nil-studio/oasis7`）以原 fork 的 Contents/Packages read token 成功拉取固定 ba7 父并离线验证 Node24/Python3.12/GH2.105；本地匿名或个人 token 的拒绝不作为 fork 兼容性的依据。正式 leaf 发布后仍须复核该模型下的 leaf 读取。

非 root、无网络 smoke 验证全部 SDK 版本、真实 crcmod C 扩展和 CRC 已知向量、显式虚假 STS 凭据的纯本地签名；禁止构造 live `OssStore`，不访问 OSS、IMDS，不申请 OIDC。后续消费者 PR 只有取得正式 digest 后才接线，保留原资格、10 分钟、环境审批、短期凭据动作与 immutable/channel-last CAS；真实合法发布仍由原授权运行验收，供应链 smoke 不代签发布。


ECS `publish-preview` 使用正式 `ghcr.io/nil-studio/oasis7/ci-ecs-oss-publisher@sha256:524d6cd5cbeb5190ade258a4f4e6ae0c63dc6839fa84b6700f58f99884ae99a4`（可信 source cdb2bfea、run46135901013，13构建/5发布组全部通过）。UID/GID1001与init、显式Bash、最小packagesread用于固定镜像拉取；原contents/actions read与id-token write、环境审批和串行发布锁保持。作业内不再安装Python依赖。

原可信checkout后、下载和获取短期凭据前，预检实际main/workflowSHA/HEAD、venv路径/只读性、Node24/GH2.105、SDK版本及真实C扩展启用状态。原fresh/retained/rollback参数、下载材料和immutable/channel-last发布命令逐字保留。测试以独立原job快照校验完整序列及安全负例，不靠任意前缀裁剪放宽。

正式leaf已通过真实fork PR #7531/run46136758480 的只读拉取：固定OCI revision与digest，network-none UID1001下15SDK版本、显式假STS本地签名、C扩展直接导入/实际启用和CRC向量通过。该证明不调用OSS/OIDC或发布，实际受保护的ECS业务发布仍由原合法运行验收。
