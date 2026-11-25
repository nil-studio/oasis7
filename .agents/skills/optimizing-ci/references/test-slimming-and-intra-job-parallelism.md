# 分册：单机零新增 Runner 的测试分层瘦身、夹具治理、多核并发与分片重平衡

本分册隶属 [optimizing-ci](../SKILL.md)，汇总 `#5374` 慢 Job 提速轨道（`#5804`、`#5823`、`#6295`、`#6296`、`#6321`、`#6401`、`#6606`、`#6620`、`#6653`、`#6667`、`#6677`、`#6697`、`#6712`、`#6749`、`#6780`–`#6997`）验证有效的方法：如何在**不新增 GitHub Actions Job 或矩阵分片（零额外队列与产物扇出压力）**且**不删减测试身份或弱化安全不变量**的前提下，将超 10 分钟慢测试 Job 压回目标窗口。权威门禁合同以 [PR 快速门禁与定期全量设计](../../../../doc/testing/ci/ci-pr-fast-and-periodic-full-design.md) 和 [CI 分层执行设计](../../../../doc/testing/ci/ci-tiered-execution.design.md) 为准。

## 1. 诊断拆解与“零新增 Runner 优先”原则

治理测试执行慢的 Job 前，先拉取 4 核标准 Hosted Runner 的步骤日志与进程回执，区分四类根因并**优先穷尽零新增 Runner 手段**（盲目扩分片会加剧低配额池排队、重复容器拉取与 GB 级产物重复下载）：

1. **重型规模/多节点/双压缩/穷举矩阵在普通 PR 全量陪跑**：单项 `>60s` 的多节点重启 Playtest、`32/64 MiB` 大体积负载或数十处同构 `[Raw, Zstd]` 循环占满 `oasis7-required` 分片。
2. **测试夹具级 I/O、子进程与容器文件系统放大**：稀疏树覆盖产生 90% 不可达中间页逐次 `fsync`、每个用例重复 `git init` / 启动上千次 Python 假 `cargo` 子进程，或临时目录落在容器层 `/tmp`（`overlayfs`）。
3. **4 核 Runner 单核串行闲置**：数十个独立 Python/Shell 契约脚本或百余个前端 JSDOM 测试以 `maxWorkers: 1` 串行执行，其余 3 核空转。
4. **瘦身后静态 LPT 装箱权重失真**：大幅压缩或迁移测试后，`ordinary_harness_costs` / `FILE_SECONDS` 仍保留 `3×–8×` 旧高估耗时或新目标回退 `60s` 默认值，导致多个重型 harness 扎堆落入同一分片。

---

## 2. Rust 重型测试双模式分层（编译期门控 vs 运行期矩阵瘦身）

按“是否需要裁去整个测试项/目标”在两种模式间选择，全量覆盖均 100% 保留在 `periodic-full` / `candidate-full`（`--features test_tier_full`）：

| 分层模式 | 语法机制 | `test_policy_changed` 影响 | 典型适用场景与实证 |
| --- | --- | --- | --- |
| **模式 A：编译期门控** | 在 `#[test]` 或 `mod` 加 `#[cfg(feature = "test_tier_full")]`；独立全量文件在 `Cargo.toml` 设 `required-features = ["test_tier_full"]` | **`true`**（本迁移 PR 在 CI 自动双跑 `required` + `test_tier_full` 验证双层全绿；合入后普通 PR 恢复 `false`） | 单项 `>60s` 多节点/重启恢复场景（`#6810`、`#6827`）、`32/64 MiB` 大负载拆分（`#5823`、`#6780`）、12 域重复 `*_pages.rs` 缓冲页矩阵（`#6821`）、超慢单测（`#6836`） |
| **模式 B：运行期矩阵瘦身** | 保留 `#[test]` 签名与数量不变，在函数体用 `if cfg!(feature = "test_tier_full")` 分流循环/压缩/抽样 | **`false`**（不改变测试清单身份，迁移 PR 自身即按普通 `targeted` 快速路径运行） | `[Raw, Zstd]` 双压缩循环收敛（`#6845`、`#6852`、`#6854`）、长轮数/等待窗口/`SIGKILL` 阶段抽样（`#6962`）、多 owner/坏根枚举抽样与测试态读写缓冲（`#6936`、`#6992`、`#6997`） |

### 实施红线与保真技巧
1. **禁触碰受保护固定库存**：分层前检索 `scripts/ci-required-scope.json`（如 `chain-authority` 14 项、`world-state-lib`、`pr_fast_incremental` 最低用例）与 `scripts/ci-formal-process-suites.json`，受保护用例必须保留在 `required` 层。
2. **保留 `required` 代表性机理与阈值不等式下界**：
   - 拆出 `32/64 MiB` 用例时，抽取共享 `exercise_*` 在 `required` 保留 `2 * VALUE_CHUNK_BYTES + 17` 跨块边界用例（`#6780`）；门控 12 个域 `mod pages;` 时保留最轻量的代表性 `incremental_fixed_economy_records::pages` 完整覆盖双格式缓冲页传输与取消恢复（`#6821`）；收敛重型套组为 `&[PageCompression::Raw]` 时，在同 crate 保留约 10 个快速套组无条件覆盖 `Raw + Zstd`（`#6845`、`#6852`）。
   - 缩减 `pr-fast` 规模或时间边界时，**严禁跌破被测不变量阈值**：如验证超出 `512 KiB` 读取配额时从 `1 MiB` 缩至 `576 KiB`（保持 `> 512 KiB`，`#6992`）；压缩条目从 `4,000` 缩至 `200` 时改用相对断言 `compact_bytes < legacy_bytes`；缩短 late-clock `original_boundary`（`14s -> 2s`）时保留 `consumer_deadline = 14s` 防止慢 Runner 抖动假阴性；缩减环形缓冲同步轮数（`17 -> 3`）时同步维护前置淘汰计数不变量（`#6962`）。
   - 对 `pr-fast` 坏根/故障矩阵（`#6992`、`#6997`），按全部 owner 与失败机理抽样代表性子集，并在非 `test_tier_full` 分支用 `BufferedPageWriter` 替代逐节点 `fsync`、为只读验证会话开启 `with_read_memo(...)`；`test_tier_full` 保持无缓存冷读与逐次物理落盘。
3. **不可缩规模的全量目标用“逐比特等价块向量化”提速（`#6992`）**：对本身带 `test_tier_full` 执行的 `required-risk-*`（如 `incremental_full_scales` 32 MiB 契约），不得缩小规模或改哈希预期；可利用确定性生成函数的周期双射性质（如预计算 256 元素 CBOR/原始字节块做块级拷贝）消除 debug 模式逐字节循环开销，保持字节流与 SHA/BLAKE3 摘要逐比特等价（实测 `311.5s -> 96.84s`）。
4. **双层 `clippy -D warnings` 零死代码守卫**：仅 `test_tier_full` 使用的测试 helper/import 必须标注 `#[cfg(feature = "test_tier_full")]` 或 `#[cfg_attr(not(feature = "test_tier_full"), allow(dead_code, unused_imports))]`，本地同时跑通 `--features test_tier_required` 与 `--features test_tier_full` 的严格 Clippy；单 PR 非锁文件改动超 `300` 行时按 crate 拆为同批独立 PR（如 `#6992` + `#6997`）。

---

## 3. 夹具级 I/O、算法热点与容器 `overlayfs` 治理

1. **稀疏树不可达暂存页裁剪 + `OnceLock` 创世快照复用（`#6677` `distfs` `10m25s -> 6m22s`）**：
   - 多轮稀疏 Merkle `append` 产生的中间分支页可达 90% 被覆盖成孤儿页（如 `43,901` 页仅 `4,350` 页可达）；先在内存暂存写入并校验预算，提交后沿正式根接口遍历可达闭包（`into_reachable`），再经 `buffered_page_writer` 一次性刷盘（硬链接数直降 `90%`）；
   - 多测试共用同一默认创世状态时，用 `OnceLock<SeedSnapshot>` 预计算一次可达页快照并分别刷入各测试隔离库，同时保留至少 1 个 `Raw` 用例走未缓存原始构造路径保障端到端覆盖。
2. **容器化 Job 优先使用宿主机 `RUNNER_TEMP` 规避 `/tmp` `overlayfs`（`#6677`）**：
   - 容器内 `std::env::temp_dir()` 默认指向容器层 `/tmp`（`overlayfs`），密集硬链接与文件创建/删除存在显著元数据放大；测试目录构造器应优先读取并规范化宿主机原生挂载的 `RUNNER_TEMP`（本地无该变量时回退 `temp_dir()`）。
3. **Python 治理/运维契约套件与进程监督器提速（`#6606`、`#6620`、`#6653`）**：
   - **消除监督器固定轮询尾延迟**：`scripts/ci_process_runner.py` 用读取线程完成 `Event.wait()` 替代每次子进程固定 `sleep(0.2)` 尾延迟，首次 `owned_processes()` 为空即跳过 3 轮 `/proc` 重扫；目录计量用迭代式 `os.scandir` 替代 `Path.rglob('*')`（保持软链/FIFO 过滤语义不变），子进程已退出时 `selector.select(0)` 零等待排空（`#6653`）；
   - **`setUpClass` 只读模板 + 纯 Bash 假 `cargo` 桩 + 不变量守护缓存**：将每例重复执行的 `git init/commit`、CodeQL pack 写入或 5 节点签名证据生成提至 `setUpClass` 构建只读模板，每例 `shutil.copytree` 派生隔离工作区（拷贝 Git 工作区后按需刷新 index stat 缓存）；把拉起上千次 Python 解释器的 `#!python3` 假 `cargo` 替换为零子进程 Bash 参数展开脚本（`#6653`）；只读基线证据验签仅在锚点路径/摘要/inode/权限、全文件字节哈希未变且 `subprocess.run` 未被 mock 时复用首次真实子进程结果（`#6620`）；
   - **缓存容量对齐与纯算法热点降维**：确认 `@lru_cache(maxsize=...)` 大于夹具文件总数避免循环驱逐（如 70 个 Markdown 夹具把 `maxsize=64` 打成零命中，`#6606`）；纯 Python 密码学热点（如仿射坐标 Ed25519 每次点加 2 次 255-bit 模逆）改用扩展射影坐标完备公式并用 RFC 8032 官方测试向量锁定等价性（`#6620`）。
4. **被动后台夹具静默周期高于单 Job 超时（`#6749`）**：手动驱动的被动 `NodeRuntime` 夹具若以 `with_tick_interval(60s)` 抑制自主 tick，在共享 CPU 争用下用例跨过 `60s` 会触发后台无 hook 提交报错（`formal runtime is degraded`）；应设为高于 CI 超时的 `3600s`（关闭时经 `stop_tx` 立即唤醒退出）。

---

## 4. 单 Runner 4 核有界并发 vs 物理挂钟窗口串行红线

1. **Shell 与 Python 契约工作池（`#6295`、`#6321`、`#6606`）**：
   - `scripts/ci-tests.sh` 的 `run_parallel_contract_commands` 与 `scripts/ci-python-control-contracts.py` 的 `run_contract_batch` 默认取 `min(4, cpu_count)` 有界并发，按耗时降序优先提交重套件（单文件巨型套件补充按 `case.id()` 排序切分的确定性 `--shard INDEX/TOTAL`，如 `scripts/p2p-public-testnet-full-network-clean-room-adapter.test.py --shard 0/4 .. 3/4`）；
   - 每个 worker 使用独立临时目录，按任务序号缓冲并加锁原子刷出 `stdout`/`stderr` 防止并发日志交错，任一任务失败立即停领新任务并精确透传首个非零退出码；
   - **本地检查兼容硬约束**：当 `OASIS7_LOCAL_CHECK=1` 时必须强制回退单流串行（`max_jobs=1`），兼容 `check-local.py` 的逐命令证据采集契约，同时提供 `OASIS7_*_JOBS=1` 串行排障开关。
2. **前端 Vitest 双池分流与受控浏览器非阻塞并发（`#6697` `viewer-js-required` `10m08s -> 7m29s`）**：
   - **Vitest 隔离分池**：`maxWorkers = Math.max(1, Math.min(4, availableParallelism()))`；依赖原生 Node realm 原型链、Web Streams 或全局 fake timers 的少数测试保留 `pool: "forks"` + `isolate: true`，其余 UI/组件测试使用 `pool: "vmThreads"` 让每个 worker 仅解析一次 `jsdom`（在 `setup.js` 补齐 `node:crypto` `webcrypto`）；
   - **受控浏览器回归**：直接调用本地 `node_modules/.bin/vite` 并发构建输出目录互不重叠的 `visual-test` 与 `release` bundle；为每个并发 driver 分配独立短路径 `AGENT_BROWSER_SOCKET_DIR` 与不截断 PID/序号的唯一 session 名（`<=40` 字符），常规关闭改用异步子进程轮询（消除 `spawnSync` + `Atomics.wait` 对同进程 HTTP fixture server 事件循环的阻塞），并按确定性矩阵顺序归并 `summary.cases`。
3. **物理挂钟窗口串行硬红线（`#6712` 反例）**：
   - 当测试强依赖真实短挂钟新鲜度/租约窗口（如 `custody-node` 中本地 `World` + `workerd` + `native` 共享 **5 秒授权窗口**）时，**严禁在 4 核 VM 内跨文件并发执行**（实测双并发 CPU 争用会导致 `source_age_ms = 5070 > 5000ms` 假阴性）；
   - 正确手段：保持分片内单文件串行零争用，将超长单文件（如 `258s` 的 `test-session-paid`）按既有 suite 注册入口无损拆分为两个独立 `.test.mjs` 文件（同步更新 `rejection-diagnostic.test.mjs` 静态守护列表），再交由既有分片做 LPT 权重均衡。

---

## 5. LPT 装箱权重刷新闭环与第三方引擎瞬态故障受控重试

1. **批量瘦身后的 LPT 分片成本表刷新（`#5804`、`#6401`、`#6919`、`#6712`）**：
   - 完成一批测试分层或拆分后，必须核对静态装箱权重（`scripts/ci-required-scope.json` 的 `ordinary_harness_costs` 与 `scripts/ci-custody-platform.py` 的 `FILE_SECONDS`）；
   - 刷新 `ordinary_harness_costs` 时单独提交 1 文件校准 PR：绑定真实标准 4 核 Hosted GHA `evidence`（跨多 run 合并时以同机未改目标如 `--lib` 归一化消除机器差异），**零修改其他顶层键**（保持 `test_policy_changed=false`，走 `ordinary_scope_config_slice_version` 快速切片）；提交前先跑静态分配回放确认最大分片显著下降（若回放变差或改善 `<2%` 则不提交，`#5374` 实证）。
2. **第三方引擎已知瞬态故障的受控进程内单次重试（`#6667`）**：
   - 自有代码竞态或超时（如 `.tmp` 原子写竞态 `#6365`）一律修根因，严禁靠重试掩盖；仅对已由 GDB 定位到第三方引擎精确指令位点的外部瞬态崩溃（如 Cloudflare `workerd` `0x573f313` `SIGSEGV`），可在捕获器（`scripts/ci-custody-workerd-capture.py`）内做进程内单次重试避免 ~11m 整 Job 重跑，须同时满足四条红线：
     1. 精确匹配故障 PC 偏移、信号与进程启动先后时序（如识别 `finally { await mf.dispose(); }` 在崩溃退出后顺序拉起并 `SIGKILL` 收尾的第 2 子进程 `post_fault_dispose_only`，继续拒绝并发双进程或未知崩溃）；
     2. 首败报告自身的 `passed` 与 `validate()` 保持严格 `False` / 拒绝不变；
     3. 完整保留并归档首败原件（`workerd-fault-attempt-1`），最多重试 **1 次**，二次失败立即 fail-closed 判红；
     4. 保持上游根因缺陷 Issue 开放，并用真实线上故障原件 `records.jsonl` 锁定回归单测。

---

## 6. 标准本地验证闭环

```bash
# 1. Rust 测试分层/瘦身：双层严格 Clippy + 选测策略核验（确认 test_policy_changed 符合预期）
./scripts/cargo-dev.sh clippy -p <crate> --lib --tests --features test_tier_required -- -D warnings
./scripts/cargo-dev.sh clippy -p <crate> --lib --tests --features test_tier_full -- -D warnings
python3 scripts/plan-rust-required-scope.py --profile pr-fast --base-ref origin/main --head-ref HEAD

# 2. Python 契约提速/并行化：核对 test_inventory 用例数与身份零变化 + 本地串行兼容回归
python3 scripts/ci-python-control-contracts.py local-governance
python3 scripts/ci-local-check.test.py
TMPDIR=/dev/shm python3 scripts/ci-workflow.test.py

# 3. 刷新 ordinary_harness_costs 时校验分片成本表契约
RUST_TC="$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)"
TMPDIR=/dev/shm python3 scripts/plan-rust-required-scope.test.py -k harness
TMPDIR=/dev/shm RUSTUP_TOOLCHAIN="$RUST_TC" python3 scripts/ci-formal-process.test.py TargetedArtifactPartitions

# 4. 切片边界与快速基线检查
./scripts/check-pr-boundaries.sh --slice Chore --allowed-crates <crate_or_none>
python3 scripts/check-local.py --profile quick --base-ref origin/main
git diff --check
```
