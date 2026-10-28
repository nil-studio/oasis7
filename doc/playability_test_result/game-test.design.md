# 游戏发布前测试（game-test）设计

- 对应需求文档: `doc/playability_test_result/game-test.prd.md`
- 可变任务状态与历史: GitHub task issue evidence comments

## 1. 设计定位

### 目标与范围

本设计规范发布前真实玩家闭环测试，统一启动脚本、`agent-browser` 进入方式、卡片填写与证据沉淀路径。由测试编排者准备环境，实际玩家保持 PRD 的黑盒游玩限制；本设计不要求玩家阅读实现。

### 当前风险与未决

本次合同核对基线为 `27db6f894bd106e9a4fee9ee3062cd8fd86e1056`。旧 L4B runner 的 build/session、自动评分和失败结果写出尚未满足新合同；scaffold 的 JSON 输出实现也仍待修复。文档纠正不代表这些实现、真实登录或试玩已经通过，[#4438](https://github.com/eng-cc/oasis7/issues/4438) 保留为历史来源；按其[关闭决定](https://github.com/eng-cc/oasis7/issues/4438#issuecomment-6127544475)，后续依据 [PR #4641](https://github.com/eng-cc/oasis7/pull/4641) 承接的现行规范落实，停止独立 Issue 跟踪不表示实现验收完成。

## 2. 设计结构
- 启动编排层：当前 worktree 内回归优先通过 `scripts/worktree-harness.sh up` 启动隔离栈；制作人 / 发布前人工验收优先通过 `scripts/run-producer-playtest.sh --open-headed` 启动真实产品链路；只有高级排障或专项回归需要直接复用 bundle / 底层 launcher bootstrap 时才使用 `scripts/run-launcher-stack.sh`。
- 真实游玩层：测试者只按玩家视角进入游戏并完成实际操作，不依赖其他实现文档。
- 证据沉淀层：将卡片、录屏与产物输出到 `doc/playability_test_result/` 与 `output/playwright/playability/`。
- 样本治理层：维护活跃样本与历史归档分层，降低旧结果对当前判断的噪音。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) | L4B 真实入口与受控回归不互相代签；完整保留 [game-test 入口专业合同](game-test.prd.md#game-test-entry-contract) 的默认 release 正式 UI、当次隔离栈实际 URL 和显式测试产物限制 | [构建与入口](#game-test-build-entry) | Viewer 构建、harness/producer；QA 编排者；锁定 PRD 继续规定启动操作 | 不为 release 安装测试 API，不改变 alias；不证明实际试玩或 L5 |
| [REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) | 本条仅保留实际候选/build、当前 worktree 与入口来源，使证据可回链；沿用 [game-test 入口专业合同](game-test.prd.md#game-test-entry-contract)，不得复用另一 worktree/历史 URL | [构建与入口](#game-test-build-entry) | harness/producer 输出及当次产物身份；QA 编排者 | 不覆盖产品结论比较单位的全部评审维度，不以 URL 或构建记录代签游玩 |
| [L4 Hosted 会话](../testing/prd.md#l4-hosted-session) | 登录/恢复、player 和 readiness 在实际执行 session 核对 | [会话与失败路径](#game-test-session) | Hosted 正式接入、账号环境；玩家/QA | 不自动签发身份，不以其他会话或 trusted local 代签 |
| [L4 执行结果](../testing/prd.md#l4-execution-result) | 每次尝试保留 RunResult、停止阶段、清理与当前指针 | [结果与评审](#game-test-evidence) | 自主试玩 RunResult、L4 summary；QA | 不借旧成功覆盖新阻断，不新增通用运行状态机 |
| [L4 事实与判断](../testing/prd.md#l4-evidence-judgment) | 五项观察分别给证据，体验结论由制作人/QA 评审 | [结果与评审](#game-test-evidence) | playability card、producer/QA | 不用 ACK/进度自动评分，不宣称 L5 |

## 3. 关键接口 / 入口
- `./scripts/worktree-harness.sh up`
- `./scripts/run-producer-playtest.sh --open-headed`
- `./scripts/run-launcher-stack.sh --bundle-dir <bundle>`（底层 / 高级 / 专项回归入口）
- `agent-browser` 打开本次实际 URL，随后由正式 UI 登录与游玩
- `doc/playability_test_result/playability_test_card.md`
- `output/playwright/playability/`

<a id="game-test-build-entry"></a>

### 构建与入口

默认 release 与正式 UI 组合；准备者记录实际候选、bundle 与构建身份，不能从 URL 的 `test_api=1` 推断测试构建。canonical `viewer` 与兼容 `software_safe` 均遵循 [Viewer entry compatibility](../world-simulator/viewer/viewer-web-entry-compatibility.design.md)：release 不安装 `__AW_TEST__`；需要该接口的受控回归必须显式选择隔离 visual-test 产物并启用测试参数。

驱动与产物不匹配时先记录 capability blocker，不继续等待登录/玩法超时。

worktree 回归用 `worktree-harness.sh url` 读取当次地址；producer 试玩使用其当次输出。不得复用固定端口、其他 worktree 或历史卡片的 URL。保留原有入口与锚点，纠正当前操作示例；历史测试卡仍是对应候选的历史证据。

<a id="game-test-session"></a>

### 会话与失败路径

准备者创建/传递本次 owned session，登录、操作、录屏和状态核对使用同一实际 session。默认 Hosted clean guest 经正式 UI 登录/注册成为 authenticated；恢复路径也须核对 player 和 gameplay readiness。另一个 producer/headed session 已登录不放行当前会话。缺配置、登录失败、身份不一致分别记录停止原因；不自动签发 Hosted 身份，也不切换 trusted local 规避失败。

准备命令结束不关闭待交接的 browser；显式传递实际 session ID，后续独立 shell 不重新生成会话。

凭据输入期间暂停/不采集录屏和截图，脱敏后开始/恢复玩法采证，不把秘密写入日志或结果。

结束、取消或阻断都先停录、保存已有证据和初步结果，再清理本次拥有的 browser/harness，并追加真实清理结果；不能以清理其他任务资源恢复本次测试。清理失败单独记录，不覆盖原停止原因。

<a id="game-test-evidence"></a>

### 结果与评审

复用 [RunResult](../testing/governance/agent-runtime-self-playtest.design.md#6-接口与数据合同)及 [L4 summary](../testing/templates/playability-l4-summary-template.md)，记录候选/build、实际 URL/session、`execution_mode`、`current_attempt`、已到阶段、停止原因、证据和清理结果。每次尝试独立保留；缺前置 `blocked`、未知 `inconclusive`、已执行且违约 `fail`，未游玩不能填写玩法成功/失败。旧 runner 缺结果时明确缺失，使用新 artifact 保留已评审旧卡，不借旧 success 判定当前尝试。

自动采集只写事实，旧导入默认为 `unreviewed`；无实际试玩时 L4B 保持 `missing`。动作、可归因世界变化、结果理解、新决策和继续理由分别指向证据，由制作人/QA 审阅后填写继续结论与五分项；缺证保持未知，进度或 ACK 不能自动填满分或 `world_activity_only=no`。重跑不得覆盖已评审 verdict。内部真人只构成 L4B 校准，不能因此宣称 L5。

## 4. 约束与边界
- 测试过程必须以真实玩家视角执行，不可用实现细节替代体验验证。
- `doc/playability_test_result/game-test.prd.md` 保留用户锁定边界；本次按用户对 #4438 文档修复的授权纠正构建、入口和执行前置，不开放无关修改。
- 样本归档与活跃视图必须保持可追溯且不过度堆积历史噪音。

## 5. 设计演进计划
- 先固定启动脚本与进入游戏入口。
- 再统一卡片、录屏和证据沉淀路径。
- 最后持续维护活跃样本/归档分层与复测门禁。

### 11.1 验证映射表

以下是实现时的验证义务；本次文档校验不代表运行场景已执行。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 与条件 | 证据落点 | 未覆盖项与原因 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) | [构建与入口](#game-test-build-entry) | 沿用 [game-test 入口专业合同](game-test.prd.md#game-test-entry-contract)：release/测试产物边界正确，动态 URL，alias 保留，不误等 API；受控回归不代签 release 试玩 | [L4-C01 / L4-C05](../../testing-manual.md#l4-toolchain-validation)：同候选构建四格与不同 worktree 入口 | 当次产物身份、入口检查和 browser evidence | 不证明 Hosted 或游玩完成，不证明 L5 |
| [REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) | [构建与入口](#game-test-build-entry) | 按 [game-test 入口专业合同](game-test.prd.md#game-test-entry-contract) 记录实际候选/build和当次URL，不复用其他worktree/历史入口 | [L4-C01 / L4-C05](../../testing-manual.md#l4-toolchain-validation)：核对同候选产物、四格入口和两个隔离 worktree 的各自输出；将历史/其他 worktree URL 作为拒用负例 | 各自候选/build、harness/producer URL来源与未串用记录 | 不证明完整比较单位评审、Hosted或玩法结果 |
| [L4 Hosted 会话](../testing/prd.md#l4-hosted-session) | [会话与失败路径](#game-test-session) | 同一执行 session 完成正式登录/恢复；负路径停止且不降级 | [L4-C02](../../testing-manual.md#l4-toolchain-validation)：guest、恢复及缺配置/错会话负路径 | 当前 attempt 的身份脱敏记录和 blocker | 不以 transport/W3 或另一窗口代签 |
| [L4 执行结果](../testing/prd.md#l4-execution-result) | [结果与评审](#game-test-evidence) | 早退、取消和新阻断可追溯，当前指针不借旧成功 | [L4-C03](../../testing-manual.md#l4-toolchain-validation)：可控故障、中断与旧成功后新阻断 | RunResult、summary、清理记录 | 输出目录不可写时报告无法落盘 |
| [L4 事实与判断](../testing/prd.md#l4-evidence-judgment) | [结果与评审](#game-test-evidence) | 机械事实不能代签继续/满分，五项缺证未知且评审不被覆写 | [L4-C04](../../testing-manual.md#l4-toolchain-validation)：机械事实、五项完整/缺项及重跑样本 | 原始会话、playability card、评审记录 | 固定脚本不证明自主决策，L4B 不代签 L5 |
