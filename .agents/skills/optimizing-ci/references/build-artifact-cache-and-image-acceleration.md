# 分册：编译调优、跨 Job 产物瘦身、缓存命中率配对与预制镜像加速

本分册隶属 [optimizing-ci](../SKILL.md)，汇总 `#5745`、`#6031`、`#6205`、`#6296`、`#6315`、`#6492`、`#6539`、`#6547`、`#6583`、`#6654`、`#6712`、`#6730`、`#6732`、`#6739`、`#6771`、`#6802`、`#6910`、`#6929`、`#6935`、`#6968`、`#6969`、`#6970`、`#6971`、`#6977`、`#6989`、`#7001`、`#7005`、`#7007`、`#7010`、`#7014`、`#7016`、`#7053`、`#7061` 验证有效的构建期、跨 Job 传输期、缓存层与容器底座提速方法及跨环境防坑清单。权威镜像规范见 [CI 基础镜像设计](../../../../doc/testing/ci/ci-base-images.design.md)。

## 1. 诊断信号：拆分非测试执行耗时

当慢 Job 的瓶颈不在测试运行本身时，先按步骤时间戳切分五类开销：

1. **环境准备冷启动重复消耗**：每个 Job/分片重复耗时 `1–5m` 执行 `apt-get`、`rustup`、Python/Node 安装或被封装工具（如 `worker-build`）隐式从 GitHub Release 二次下载 CLI 并偶发 HTTP 503。
2. **工作区/组件缓存低命中或单侧断流**：`ordinary-workspace-v1` 或组件缓存频繁 miss，使 `8–10m` 热构建退化为 `25–37m` 全冷编译。
3. **单包 LLVM 优化与 Thin-Local LTO 长尾**：`cargo check` 仅十几秒，但 `cargo build --release` 在单个大胶水/WASM crate 上消耗 `3–7m`。
4. **粗粒度附属资源误触发构建**：测试产物构建器未区分 `lib` 与 `test` 目标，令普通 `targeted` PR 串行空跑 `~7m` 无关 Docker WASM 构建并额外绑定宿主 Docker。
5. **跨 Job 传输未剥离符号表的数 GB 测试二进制**：即便开启 `debug = 0` 或 `gzip` 打包，百余个未优化 Linux ELF 仍携带约 `35%` 非加载 `.symtab`/`.strtab` 符号节（`compression-level: 0` 时更甚），放大打包哈希、跨节点上传下载与分片解压耗时。

## 2. 关键文件与职责接缝

| 文件 | 在构建、产物、缓存与镜像加速中的职责 | 对应本地单测 |
| --- | --- | --- |
| `scripts/ci-workspace-cache.py`、`scripts/ci-cache-archive.py` | 计算 `ordinary-workspace-v1` 等缓存身份、守卫 `main` 可信写权限并打包归档 | `scripts/ci-workspace-cache.test.py`、`scripts/ci-cache-policy.test.py` |
| `scripts/ci-cache-maintenance.py` | 守卫 20 GB 仓库缓存容量上限，在容量压力下分级回收已停用历史依赖环境 | `scripts/ci-cache-maintenance.test.py` |
| `scripts/ci-image-cache.py` | 定义 `rust-cache.yml` 各容器组件编译上下文的精确预热命令集合 | `scripts/ci-image-cache.test.py` |
| `scripts/ci-test-artifacts.py` | 构建普通/Formal 测试产物、暂存 `ordinary` ELF 符号剥离、按需门控 `BuiltinTestArtifact`、并发打包分片产物及 `cache_restore()` 增量 mtime 刷新 | `scripts/ci-formal-process.test.py`、`scripts/ci-builtin-test-resources.test.py` |
| `scripts/ci-custody-platform.py` | Custody 原生产物副本 `strip`、流式 `zlib` 双摘要压缩打包与原子校验恢复 | `scripts/ci-custody-platform.test.py` |
| `.github/workflows/ci-base-images.yml`、`docker/ci-base/` | 构建、离线自检、Trivy 扫描并发布分层预制 CI 镜像及不可变 `@sha256` 摘要 | `scripts/ci-workflow.test.py` 及镜像目标烟测 |

---

## 3. 五大验证有效的构建、产物、缓存与镜像提速模式

### 模式 A：分包精准 Cargo Release Profile 与关闭 Thin-Local LTO（`#6771`）
- **根因盲区**：Cargo 默认 `[profile.release]` 的 `lto = false` **并未关闭 LTO**，而是默认启用包内跨 16 CGU 的 **thin-local LTO**（只有 `lto = "off"` 才真正关闭）。在 `custody-wasm-production` 中，这使 `oasis7_custody_core` 单次 WASM 编译达 `216s` 且因 feature 差异重编两次。
- **精准调优（禁全局降 `opt-level`）**：
  1. **保留密码学与编解码依赖 `opt-level = 3`**：切勿全局调低 `profile.release.opt-level`，否则 `ed25519-dalek`、`curve25519-dalek`、`sha2`、`blake3`、`serde_cbor` 在 `workerd` 5 秒授权窗口内会因运行变慢超时；
  2. **仅对业务胶水/路由 crate 设 `opt-level = "s"` 并关闭 LTO**：在构建脚本透传 `--config 'profile.release.lto="off"'` 与 `--config 'profile.release.package.<crate>.opt-level="s"'`（实测合并 WASM 编译从 `964s` 降至 `489s`，净省 `7m55s`，且 `.wasm` 体积同步缩小）；
  3. **同 Job 多脚本 `--config` 严格一致**：用单测（如 `verify-stages.test.py`）锁死同目录所有角色/夹具构建脚本的 `--config` 标志完全一致，防止 Cargo 元数据哈希分裂触发 `target/` 二次重编。

### 模式 B：跨 Job 产物剥离压缩、按消费者分包与重打包并行组装（`#5745`、`#6296`、`#6654`、`#6712`、`#7061`）
1. **暂存副本 `strip --strip-unneeded` 与摘要自洽闭环（`#6712` Custody、`#7061` Ordinary）**：
   - 即便设置 `CARGO_PROFILE_TEST_DEBUG=0`，未优化 Linux 测试与辅助 ELF 仍携带约 `35%` 非加载 `.symtab`/`.strtab` 符号表；
   - **`ordinary` 暂存期剥离（`#7061`）**：在 `scripts/ci-test-artifacts.py::build()` 将 `target/` 产物通过 `shutil.copy2(source, destination)` 拷入 `bundle/bin/<logical>` 之后、**计算 `manifest.json` 的 `sha256` 与 `bytes` 之前**，严格限定 `scope == 'ordinary'`、Linux 常规文件（`not is_symlink()`）且头部魔数为 `b'\x7fELF'` 时执行 `strip --strip-unneeded`。既保持 `target/` 原件（不影响 `ordinary-workspace-v1` 库缓存与本地 Cargo 新鲜度）、`bundle/lib/*.so` 及 `formal*`/`storage*` 作用域 100% 不变，又保留 `.eh_frame` 栈展开与 `.dynsym` 动态符号（生产实测整包上传 `2.98 GB -> 2.22 GB` 净省 `-753.6 MB`，4 分片总下载 `5.22 GB -> 3.87 GB` 净省 `-1.34 GB`，分片解压校验耗时 `-40.4%`）；
   - **`compression-level: 0` 通道下的流式 `zlib` 双摘要恢复（`#6712`）**：对无自带压缩通道的 `custody-native` 工件，在副本 `strip` 后以标准库流式 `zlib`（`level=1`，`4 MiB` 分块）压缩（`1,282 MB -> 210 MB`，`-83.6%`，三路下载 `80–102s -> 5–12s`），`manifest.json` 同时记录传输态 `(size, sha256)` 与解压态 `(raw_size, raw_sha256)`，`restore()` 设解压硬上限并在临时目录校验双摘要及零尾随字节后原子替换。
2. **按消费者切片交付与多线程并发打包（`#5745`、`#6296`）**：将单一大包拆分为共享 `common` 辅助包 + 各分片唯一归属 payload 包（`controls` 零下载 native 测试二进制，总传输字节 `-70.6%`），`pack_ordinary_delivery` 按 `native_cpu_budget()` 多线程并发 `gzip` 压缩并按确定性键序写出 `delivery.json`。
3. **超预算串行重打包拆分为同源并行 Producer + 零 Cargo 组装（`#6654`）**：当单 Job 串行执行“重原生 release 编译 + 前端/WASM 构建”逼近超时上限时，拆为检出同一明确 `source_sha` 的两个并行构建 Job，再由短组装 Job 按同 run 产物 ID 强验来源、归档摘要、ELF、`BUILDINFO` 与新鲜度清单后打包（组装阶段零编译）。

### 模式 C：测试附属重资源按 `target_kind` 精准按需构建（`#6989`）
- **做法**：先全仓检索附属资源加载器（如 `OASIS7_BUILTIN_WASM_FETCHER` 与 4 个 `m1.*` 内置测试 WASM 模块）的实际消费目标。确认仅由 `--lib` 单元目标消费、37 个集成测试目标零调用后，在 `scripts/ci-test-artifacts.py` 的 `BuiltinTestArtifact.required(manifest)` 增加 `binary.get('target_kind', ['lib']) == ['lib']` 过滤——既为命中 `ordinary_risk_targets`（`target_kind == ['test']`）的普通 `targeted` PR 省去 `~7m12s` 串行 Docker WASM 编译，又解除对宿主 Docker 的不必要耦合。
- **跨版本兼容细节**：缺省值必须保留 `['lib']`（`binary.get('target_kind', ['lib'])`），确保 `policy_sha` 上未传 `target_kind` 字段的既有非演进单测夹具（`ci-builtin-test-resources.test.py`）100% 向后兼容通过。

<a id="workspace-and-component-cache-pairing"></a>
### 模式 D：工作区/组件缓存高命中率治理与跨环境严格配对（`#6315`、`#6492`、`#6539`、`#6547`、`#6583`、`#6732`、`#6929`、`#6935`、`#6968`、`#6969`、`#7005`、`#7010`、`#7014`、`#7053`）
1. **工作区缓存主键剔除高频无关噪音（`#6929`、`#6935` `ORDINARY_STABLE_IDENTITY`、`#7053` `Cargo.lock` 前缀解耦）**：
   - `scripts/ci-workspace-cache.py` 的 `ordinary` 前缀不哈希无关的 `ci-formal-process-suites.json`、高频演进的 `ci-test-artifacts.py` 整文件、成员 `Cargo.toml` 内容或 `Cargo.lock` 内容，仅绑定工具链、镜像摘要、`units.CACHE_SCHEMA`、根 `Cargo.toml` 及排序后的 `member_manifests` **路径列表**；成员 `Cargo.toml` 与 `Cargo.lock` 内容变化保留在精确提交后缀并交由 `ci-test-artifacts.py::cache_restore()` 基于 `(bytes, mode, sha256)` 刷新 mtime 触发 Cargo 安全重算受影响指纹（使增删内部依赖边或小幅依赖对齐的 PR 仍能通过 `restore-keys` 复用工作区单元缓存）；构建与消费端的测试二进制 `--list` 清单校验采用 4 线程有界并发。
2. **严防无过渡覆盖的单侧 Writer 迁移，落实非 root 候选读端与可信写端精确配对（`#6732`、`#6713`、`#7005`、`#7040`）**：
   - **反例教训（`#6732` 早期）**：当缓存身份包含容器镜像摘要（如 `ORDINARY_BUILD_IMAGE`）时，若直接迁走宿主读端唯一依赖的 writer 且未对齐读端环境哈希，会导致普通 PR 全面退化为 `20–37m` 冷编译；
   - **可信写端与候选非 root 读端配对**：可信 `main` 的 `oasis7-test-build-trusted` 运行于 `ci-rust-ordinary-builder` 并声明 `ORDINARY_BUILD_IMAGE` 写入镜像依赖缓存与 `ordinary-workspace-v1`；`oasis7-test-build-candidate` 在受信任计划确认无需宿主 Docker canonical WASM 材料（`ordinary_candidate_image == 'true'`）时，以 `--user 1001:1001 --init --tmpfs /opt/oasis7-mold:uid=1001,gid=1001,mode=0755,exec` 只读进入同一完整摘要镜像（跳过 `setup-python` 避免注入 `PKG_CONFIG_PATH`，保持默认 `/opt/oasis7-mold` 链接器路径使 Swatinem 与 `ordinary-workspace-v1` 环境哈希与 writer 100% 一致，并在预检中强验 `1001:1001` 无附加组且连接 `/var/run/docker.sock` 返回 `PermissionError`）；仅在 `test_policy_changed=true` 或 `candidate-full` 等确需 Docker WASM 构建的罕见路径回退宿主 `ubuntu-24.04`（保留 SDK 回收与 `ci-shared-native-trusted-v4`）。
3. **构建期源码树输入快照防篡改与嵌套独立 `[workspace]` `target/` 清理不变量（`#7014`、`#7010`）**：
   - **诊断信号**：trusted writer 构建成功，但诊断产物 `ordinary-workspace-cache-report-*.json` 报 `{"save": false, "reason": "source inputs changed during the build"}` 跳过落盘；
   - **根因与不变量**：producer 会对比构建前后完整源码快照（含未跟踪与 ignored 文件，仅排除 `.git` 与根 `target/`），任何构建期写入源码树的副产物都会触发 fail-closed 拒存。调用独立非根 `[workspace]`（如 `crates/oasis7_builtin_wasm_modules/*`，要求 `CARGO_TARGET_DIR` 未设置）构建 canonical 材料时，必须在编译启动前（且确认目标 `target/` 尚不存在、不含初始输入快照文件、不豁免 `cargo metadata` 阶段副作用）通过 `CanonicalWorkspaceOutputs` 预登记受限输出目录（`#7014`），并在复制产物后的 `finally:` 中安全清理本次新建的嵌套 `target/`（先判 `is_symlink()` 只 `unlink` 不穿透外部软链，`#7010`），严禁放宽 `_cache_inputs` 源码防篡改校验。
4. **`@actions/cache` 跨宿主/容器的 `archive version` 与相对路径落点对齐（`#6315`）**：
   - `actions/cache` 的内部 archive version 同时取决于**归档路径列表字符串**与**压缩工具（`zstd` vs `gzip`）**；容器复用宿主 writer 的只读缓存时，镜像必须预装并预检 `zstd`、恢复步骤显式采用 writer 路径字符串维持 archive version 一致；
   - 注意 `@actions/cache` 按相对 writer `GITHUB_WORKSPACE` 打包工作区外路径（如 `../../../.cache/...` 在容器挂载目录解压后落到 `/.cache/...` 而非 `/home/runner/.cache/...`），须按已验证 workspace 词法计算实际 `restored_root` 并强验工具版本，损坏命中直接 fail-closed 报错而非静默回退重装。
5. **预热编译上下文、链接器路径哈希对齐、显式工具路径注入与 20 GB 休眠环境回收**：
   - `scripts/ci-image-cache.py` 预热命令须与真实消费者子命令/target/features 严格一致（纯二进制包用 `--bins` `#6583`；禁误传 `--no-deps` `#6547`；补齐重型上下文后 `workspace-support-other` 从 `30.7m` 降至 `17.3m` `#6492`）；**新增预热目标前必须核对两端 `Swatinem/rust-cache` 恢复键环境哈希一致**——例如 `rust-image-native-cache` 以 root 默认 `/opt/oasis7-mold` 预热（且仅 `workflow_dispatch` 手动触发、`timeout-minutes: 20`），若消费端（如 `custody-native`）使用 `--user 1001:1001` 与独立 `install-dir: ${{ runner.temp }}/...`，双方 `CARGO_TARGET_*_LINKER` 哈希不同会导致预热完全无法命中且平白挤占缓存池与超时预算；
   - 对内部会二次联网拉取 CLI 的封装器（如 `worker-build` 下载锁定版 `wasm-bindgen`），复用经 `Cargo.lock` 版本校验的只读工具缓存并显式传入绝对路径环境变量（如 `WASM_BINDGEN_BIN`，消除 ~100s 下载与 HTTP 503，`#6968`）；
   - `scripts/ci-cache-maintenance.py` 在 20 GB 容量压力下按“同命名空间新代已就绪 + 旧代 `>=6h` 无读取 + 非活跃运行保护 + 双清单重校验”回收停用依赖快照（`#6539`），且缓存输入轮换自动绕过 `main-fast` 120 分钟冷却立即热库（`#6969`）。

### 模式 E：预制 CI 基础镜像消除冷启动与容器安全隔离六守则（`#6031` 系列）
将重复安装工具链/系统库的慢 Job 迁入已发布固定镜像（如 `#6802` 省去 4 个 `native` 分片各 `4–5m` 安装、`#6971` 将 CodeQL 平台观察压至 `41s`、`#7001` `required-gate` `16s`、`#7007` `custody-native` `8m42s`）时，严守六条边界：
1. **按最小职责选镜像叶子并固定 `@sha256` 摘要**：Python/Node 轻量汇总用 `ci-runtime`、GitHub API 只读观察用含固定 `gh` 的 `ci-api-runtime`（`#6949`/`#6971`）、站点浏览器检查用预装锁定 Playwright/Chromium 的 `ci-browser`、常规 Rust 编译与分片消费用预装固定 Rust stable/WASM nightly + `zstd` + 原生依赖库的 `ci-rust-build`（`#6802`/`#6910`/`#7007`；`mold` 仍通过 `./.github/actions/setup-linux-mold` 按作业安装）、CodeQL Rust 分析用预装分析器固定工具链及 GTK/WebKitGTK/D-Bus 开发包的 `ci-codeql-rust`、可信 ordinary 生产用仅补静态 `docker` + `buildx` CLI 的 `ci-rust-ordinary-builder`（`#6977`/`#7005`），Job 仅授予最小 `packages: read` 拉取权限。
2. **不可信 PR 容器严禁访问 Docker Socket（`#6970` vs `#7005`）**：可信 `main` 生产者可在固定镜像内有界预检（验证 daemon 可达与默认 `docker` driver）并复用 runner 默认挂载的宿主 Docker daemon；**不可信候选 PR 容器严禁赋予访问 `/var/run/docker.sock` 的组权限或 root 特权**——候选路径须先按需收敛对宿主 Docker 的耦合（`#6989`），仅在无需 Docker WASM 构建时以 `--user 1001:1001 --init` 进入镜像并强验连接 `/var/run/docker.sock` 被拒（`--forbid-docker`），确需 Docker 时条件回退宿主。
3. **保持 Canonical 工作区绝对路径（防 Runner `/__w` 重写）、同 `dev+inode`、非 root `--user 1001:1001` 与链接器路径对齐（`#6732`、`#6802`、`#6910`）**：Rust 测试二进制与 `CARGO_BIN_EXE_*` 绑定编译期绝对路径 `/home/runner/work/oasis7/oasis7`，且 GitHub Actions Runner `StepHost.cs` 会把 `defaults.run.working-directory` 或 job `env` 中的宿主工作区前缀自动重写回 `/__w`——跨宿主消费的容器 producer 必须显式挂载 `/home/runner/work/oasis7/oasis7:/home/runner/work/oasis7/oasis7`、声明 `ImageOS=ubuntu24`（预检 `os-release`）并通过 `mode 0600` `BASH_ENV` 启动文件在每步 shell 显式 `cd` 至该原路径，同时预检默认 workspace 与 alias 的 `dev+inode` 及 `Git HEAD` 一致；常规非 root 容器设 `container.options: --user 1001:1001 --init`（保留 `mode 000` 权限测试语义并回收僵尸进程），独立缓存命名空间可为 `setup-linux-mold` 传入可写 `install-dir: ${{ runner.temp }}/...`，而需与 root writer 共享默认 `/opt/oasis7-mold` 链接器路径的只读容器须挂载 `--tmpfs /opt/oasis7-mold:uid=1001,gid=1001,mode=0755,exec` 保持 `CARGO_TARGET_*_LINKER` 字符串一致。
4. **缓存按完整镜像 `@sha256` 命名空间隔离**：不同镜像摘要不得混写同一 Cargo 缓存 key；新增镜像缓存 family 须同步接入准入校验与 `ci-cache-maintenance.py` 清单，PR 侧始终只读。
5. **先发布正式摘要（必要时先合入 `FIXED_TESTS` 白名单前置如 `#6948`/`#7000`），再用冻结历史真实产物做宿主/镜像双端回放验收**（核对正常汇总与缺分片/上游失败/draft 拒绝行为逐字一致后再切流，`#7001`、`#7016`）。
6. **CodeQL 专用分析线与打包/发布边界工作流额外红线（`#6031` 第 4/5 批、`#6730`、`#6739`）**：
   - **CodeQL 8 个专用分析/诊断工作流**：必须保持固定 CodeQL CLI bundle、JVM 预算及 `ci-codeql-rust` 专配提取工具链（如固定 Rust `1.99`，**严禁用通用 `ci-rust-build` 工具链替代**）；接入镜像时先在低权限隔离模式（`cache: none`、关闭 SARIF 上传）下完成同冻结源宿主 vs 容器本地 SARIF 摘要一致性验证，通过后再切正式分析与 SARIF 写权限；
   - **Release / Mainnet / Testnet / ECS / Pages 打包与发布边界**：镜像迁移本身不授予发布权限，必须保持 OIDC / token / tag / `SOURCEPLAN` / `BUILDINFO` / 不可变产物 digest 顺序，先对冻结产物做只读回放与 `dry-run` 逐字比对（`#6739`）；当发布工作流回放历史 tag/commit 遇到不在当前镜像内的旧编译器 pin（如历史 `1.92`，`#6730`）时，仅对该历史 pin 隔离到作业自有 `owned-home` 安装，严禁自动探测改写共享镜像工具链或篡改历史源码 pin。

---

## 4. 标准本地验证闭环

修改缓存身份、产物打包、预热上下文或容器镜像接线时，推送前按范围执行（注意 `ci-builtin-test-resources.test.py` 等非 `EVOLVING_POLICY_TESTS` 套件若未触发全量策略迁移，还须用 `origin/main` 基线测试源码对候选脚本做跨版本兼容校验）：

```bash
RUST_TC="$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)"

# 1. 工作区缓存、容量回收与镜像预热上下文单测
RUSTUP_TOOLCHAIN="$RUST_TC" python3 scripts/ci-workspace-cache.test.py
RUSTUP_TOOLCHAIN="$RUST_TC" python3 scripts/ci-cache-policy.test.py
python3 scripts/ci-cache-maintenance.test.py
RUSTUP_TOOLCHAIN="$RUST_TC" python3 scripts/ci-image-cache.test.py

# 2. 测试产物构建、内置 WASM 按需门控与 Custody 剥离压缩单测
RUSTUP_TOOLCHAIN="$RUST_TC" python3 scripts/ci-builtin-test-resources.test.py
TMPDIR=/dev/shm RUSTUP_TOOLCHAIN="$RUST_TC" python3 scripts/ci-formal-process.test.py
python3 scripts/ci-custody-platform.test.py
python3 crates/oasis7_custody_worker/scripts/verify-stages.test.py

# 3. 工作流契约、控制面回归与切片边界检查
TMPDIR=/dev/shm python3 scripts/ci-workflow.test.py
python3 scripts/ci-python-control-contracts.py local-governance
./scripts/check-pr-boundaries.sh --slice Chore --allowed-crates none
python3 scripts/check-local.py --profile quick --base-ref origin/main
git diff --check
```
