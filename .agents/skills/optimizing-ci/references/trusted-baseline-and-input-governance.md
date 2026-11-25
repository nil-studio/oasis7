# 分册：受信基线不变量设计、结构化输入路由与消除前置微 PR

本分册聚焦 CI 门禁自身的工程可演进性：在不削弱 `policy_sha` 权威隔离与 fail-closed 安全边界的前提下，消除“为了合入一个正常业务/工作流 PR，不得不先向 `main` 提一个几行白名单过渡微 PR”的流程摩擦与双跑浪费。

## 1. 两步式前置微 PR 的三类产生根因

当仓库频繁冒出仅修改 1–2 个测试或规划脚本的前置 `Chore` PR（如历史上的 `#6759`、`#6819`、`#6830`、`#6909`、`#6916`、`#6923`）时，先按以下三类根因归类，再对症做根因收敛而非继续叠加单点白名单：

| 现象 | 真实根因 | 根治手段 |
| --- | --- | --- |
| 演进中的 crate 新增合法依赖或切走旧伞包 import，被 `workflow-governance` 的旧 `main` 测试拦死 | 受信基线测试写成了**演进态精确快照**（如 `set(deps) == {...}` 或写死旧路径 import），而非架构红线不变量 | 改写为**单调边界不变量**（见 §2） |
| 新增常规脚本、包驱动或静态 golden 测试夹具（`.json`/`.cbor`）触发 `UnknownInputsError`、`TestPolicyError` 或 CodeQL `unregistered input` | 路径/包清单采用**逐文件枚举或多处手写副本**，缺少受管职责目录与结构化边界判定 | 建立**受管目录 + 结构化排他守卫 + 单一真源**（见 §3） |
| 合法调整工作流拓扑、汇总口径或拆迁断言时，因整个测试文件被 `policy_sha` 全量执行并冻结方法名而无法同 PR 自审 | 高频演进套件未做“固定安全红线 vs 候选完整回归”分层，或 `FIXED_TESTS` 混入了易变拓扑细节 | 提炼非空 `FIXED_TESTS` 红线并纳入 `EVOLVING_POLICY_TESTS`，同步清理旧拆单文档（见 §4） |

---

## 2. 受信基线断言设计：写“单调边界不变量”，禁写“演进态精确快照”（`#6922`）

任何由 `policy_sha`（`main`）加载并针对候选工作区执行的测试（如 `scripts/ci-bin-ownership.test.py` 及各 `FIXED_TESTS` 用例），编写与评审时严守三条准则：

1. **依赖边界用“核心子集 + 禁区黑名单”，禁锁精确全集**：
   - **反模式**：对仍在承接拆迁或迭代的 crate 写 `self.assertIn(set(deps), (old_set, old_set | new_edge))`。每新增一个合法依赖（如 `url`、`reqwest`、`tracing`）都会迫使开发者先提独立白名单 PR。
   - **正模式**：统一采用 **`required_core_edges.issubset(deps)`**（锁定不可退化的核心职责边与 `default-features = false` 等关键约束）加 **`forbidden_direct.isdisjoint(resolved)` / 传递本地路径闭包 `forbidden_transitive.isdisjoint(resolved)`**（递归禁止回流 `oasis7` 伞包、反向依赖上层宿主/CLI/Viewer/Worker 或越权引入 `wasmtime`/`libp2p`，同时解析 `{package = "...", path = "..."}` 别名防绕过）。
2. **跨切片拆迁断言兼容“门面存废双态”**：
   - 在 `[Slice: Dark] -> [Slice: Adapter] -> [Slice: Cutover]` 推进及薄门面清理期间，断言**“至少存在一个活跃 owner、无重复定义且消费者指向规范接口”**，对历史兼容门面使用 `if legacy_path.is_file():` 校验纯重导出，允许后续 Cutover 或薄 shim 清理单 PR 直接删除旧门面而不触发基线判红。
3. **职责归位：不在 Python 治理脚本枚举业务函数与负例名**：
   - 受信基线只守跨包所有权、方向矩阵与可执行目标边界；模块内部的具体业务函数签名、错误文本和单测用例覆盖留给目标 crate 自身的 Rust 单测与 `cargo-dev-architecture.py` 承接。

---

## 3. 输入准入与包清单治理：同 PR 受控登记、受管目录与结构化排他守卫（`#6839`、`#6922`、`#7015`）

- **偶发新增受控路径默认单 PR 收口（`#7015`）**：`ordinary_scope_config_slice_version=2` 已支持在同一 PR 内同时新增 regular 文件并向 `scripts/ci-required-scope.json` 新增受控规则（或向既有规则保序追加 `match` 超集模式，声明非 `full` 且非空有效 `groups`），**切勿再为常规新脚本惯性拆出前置路径登记微 PR**（详见 [减少 Job 数量分册 §3 模式 D](job-count-reduction.md)）。
- **结构化根治触发阈值**：当同一类新增输入连续 **≥ 2 次**需要手工编辑路径/包枚举或触及 `plan-rust-required-scope.py` / `codeql-plan.py` 硬编码分支时，停止逐文件叠加 `match` 或 `elif path == ...`，按以下三步一次性收敛根因：

1. **多处重复配置收敛至单一数据源（Single Source of Truth）**：
   - 如 `#6839` 将散落在 `ci-required-scope.json`、`ci-test-artifacts.py` 与 `ci-tests.sh` 四处的 `oasis7_required_packages` 收敛为从 `ci-required-scope.json` 动态读取并 fail-closed 校验非空/无重复，配合单测锁定 `package_groups` 与驱动解析出的 `-p` 列表严格一致。
2. **按职责建立受管子目录，保留根目录未知路径 fail-closed**：
   - 如 `ci-required-scope.json` 为 `scripts/{governance,ops,packaging,viewer}/**` 预登记职责组映射，新脚本归入对应受管子目录即可自动命中路由；同时保持 `scripts/` 根目录未登记文件与顶层未知目录继续抛 `UnknownInputsError`，不覆盖任何 `CONTROL` 路径。
3. **跨语言敏感规划器使用“结构化排他守卫”（以 `scripts/security/codeql-plan.py` 为例）**：
   - 静态测试夹具不能无条件按 `**/fixtures/**` 全仓放行（避免漏扫 JS/TS `workerd` 或 Python 共享夹具），但可通过可静态证明的三重排他条件自动归入 `rust-repo`：
     1. **路径与后缀受限**：匹配 `crates/<crate>/(fixtures|testdata|tests/fixtures|src/**/{fixtures,test_data,test_fixtures,serialization_fixtures})/**` 且区分大小写后缀精确属于 `('.json', '.cbor', '.zst')`；
     2. **纯 Rust 包身份校验**：`base` 或 `head` 树中存在 `crates/<crate>/Cargo.toml`，且该 crate 子树下不含任何 `package.json` 或 `wrangler.toml`（自动排除 `oasis7_dev_test_worker`、`oasis7_custody_worker`、`oasis7_viewer`、`oasis7_agent_api` 等多语言包）；
     3. **零跨语言引用校验**：仅当命中上述规则时，对 `merge_base` 与 `source_head` 执行 `git grep -I -l -z -F <basename> <oid> -- '*.js' '*.ts' '*.mjs' '*.py' ...`，确认无 JS/TS/Python 源码引用该文件名（同时天然覆盖跨 crate 搬迁夹具时的旧路径删除端与新路径新增端）；任一条件不满足立即回退 `unregistered input` **fail-closed**，符号链接审查与批次门禁原样保留。

---

## 4. `FIXED_TESTS` 红线遴选、`EVOLVING_POLICY_TESTS` 扩充判据与清理“流程惯性”（`#6928`、`#6991`）

### 4.1 `FIXED_TESTS` 入集与禁入标准
`scripts/ci-python-control-contracts.py` 对 `EVOLVING_POLICY_TESTS` 在 `policy_sha` 侧仅运行 `FIXED_TESTS[path]`，而在候选侧运行完整套件（`candidate_tasks[path] = None`，零 skip + completion witness）。遴选 `FIXED_TESTS` 时严格区分两类断言：
- **必须入集（真安全与权威红线）**：未知输入/删除/重命名阻断、跨分支缓存写隔离与可信 `main` writer 身份、`ci-priority` 同仓 PR 路由限制、`required-gate` / `explicit-full-gate` / `full-gate` 回执防伪造与失败不吞没、只读预览无门禁权限、所有可执行 job 声明 `timeout-minutes`、检查定义摘要固定读取 Git revision（非工作树）、以及核心轻量/签名包无传递重依赖。
- **严禁入集（易随优化演进的拓扑/形态细节）**：具体 job 的精确 `needs:` 数组、预构建镜像目标完整枚举、`contract_job_consolidation_version` 归并形态、`main-fast` 具体冷却参数或迁移期模块路径断言。一旦将此类断言塞入 `FIXED_TESTS`，每次合法优化都会退化回两步式前置 PR。

### 4.2 何时将新套件纳入 `EVOLVING_POLICY_TESTS`
- 当某个非 `EVOLVING_POLICY_TESTS` 控制契约套件（例如 `ci-builtin-test-resources.test.py`、`ci-contract-jobs.test.py`）因正常策略优化重复出现 `Step 1/2` 双态过渡 PR，或重型子进程套件在 baseline 与 candidate 重复全量执行拖慢 `workflow-governance` 时：
  1. 盘点该套件，提取非空真安全红线列表写入 `FIXED_TESTS` 与 `ci-python-control-contracts.test.py` 的 `REQUIRED_FIXED_TESTS`；
  2. 将该套件路径加入 `EVOLVING_POLICY_TESTS`（由于 `scripts/ci-python-control-contracts.py` 自身位于 `migration_paths` 的 `boundaries` 内，该升级 PR 在满足完整 `scope=full` 组覆盖时可单 PR 平滑自迁移）；
  3. 常规使用边界与本地 `policy_sha` `FIXED_TESTS` 跨版本秒级仿真命令见 [按需选测与快速失败分册 §4](on-demand-scope-and-preflight.md#trusted-control-split-and-simulation)。

### 4.3 门禁松绑必须同步清理“流程惯性文档”（Process Inertia Guard）
- **实证教训（`#6928` → `#6976`/`#6980` → `#6991`）**：`#6928` 在技术门禁上已允许 `ci-workflow.test.py` 随工作流单 PR 原子更新，但因先前文档仍保留“凡改 `rust.yml` 必须先拆 PR-A 再提 PR-B”的旧口诀，后续 Agent 仍机械拆出了 `#6976`、`#6980` 两个无必要的过渡微 PR，直至 `#6991` 刷新本 Skill 后才彻底归零。
- **强制动作**：凡放宽、合并或重构 CI 门禁与信任边界的 PR，推送前必须用 `rg` 检索 `.agents/skills/`、`AGENTS.md`、`testing-manual.md` 及相关脚本头部注释，**在同一批次淘汰已过时的多步绕行指令**。

---

## 5. 标准本地验证闭环

修改受信基线契约、`FIXED_TESTS` 分层或 CodeQL / Scope 输入路由规则后，推送前执行：

```bash
# 1. 验证控制契约分层、新增 FIXED_TESTS 真实 worker 见证与所有权契约
python3 -W error scripts/ci-bin-ownership.test.py
python3 scripts/ci-python-control-contracts.test.py SelectionAndMigration GovernanceOwnership TrustedContracts.test_fixed_safety_inventory_exists_in_current_repository RealRustControlFixture -v

# 2. 验证 Scope / CodeQL 输入路由正反例（含未知路径、混合包与跨语言引用 fail-closed）
TMPDIR=/dev/shm python3 scripts/plan-rust-required-scope.test.py
python3 scripts/security/codeql-plan.test.py
python3 scripts/security/codeql-rust-capacity-layout.test.py

# 3. 同步设计文档与 Skill 校验
./scripts/lint-skills.sh
./scripts/doc-governance-check.sh
./scripts/check-pr-boundaries.sh --slice Chore --allowed-crates none
git diff --check
```
