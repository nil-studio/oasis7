# 前期工业引导 `test_tier_required` playability 卡组（2026-03-15）

审计轮次: 6

## 目的
- 对应任务：`TASK-GAME-020`、`TASK-GAME-020 / T4 / qa_engineer`
- 对应需求：`doc/game/gameplay/gameplay-top-level-design.prd.md`
- 对应测试主入口：`testing-manual.md`
- 对应标准卡模板：`doc/playability_test_result/playability_test_card.md`

本卡组把“首个制成品 / 停机恢复 / 首座工厂单元”收口为 `test_tier_required` 的人工复核最小集。
当 runtime / viewer / gameplay 改动影响以下任一体验时，必须至少抽跑对应卡片：
- `首个制成品`
- `停机 -> 恢复 -> 继续产出`
- `首座工厂单元`

## 执行约束
- 自动化前置未通过时，不进入玩法结论；先按失败签名阻断。
- 手动回归默认沿用当前 worktree 隔离入口：`./scripts/worktree-harness.sh up`。
- 需要真实产品包 / 制作人式人工验收时，改用：`./scripts/run-producer-playtest.sh --open-headed`。
- 只有底层排障或专项回归需要直接控制 bundle bootstrap 时，才使用：`./scripts/run-launcher-stack.sh --bundle-dir <bundle-dir> --with-llm`。
- Viewer Web 若长期停在 `connecting`，必须先按 `testing-manual.md` S6 图形/环境门禁处理，再决定是否继续填写卡片。
- 每次执行至少回写 1 张正式卡片；若本轮三张都执行，允许写 3 张独立卡片，或 1 张汇总卡片并在“关键操作链路 / 问题摘要”中覆盖三条链路。

## `test_tier_required` 前置命令
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime tests::economy:: -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_viewer_runtime viewer::runtime_live::mapping -- --nocapture
```

覆盖缺口：旧 Rust Viewer package 已退役，原 `ui_text_industrial`、`feedback_tone_for_event_maps_warning_positive_and_info`、`push_feedback_toast_uses_runtime_industry_friendly_detail`、`sync_agent_chatter_bubbles_formats_runtime_industry_feedback` 四项断言尚未找到逐项等价承接测试，不能作为当前可重复执行命令或通过证据。当前 Web UI 可执行 `npm --prefix crates/oasis7_viewer run test:ui` 与 `test:feedback-contract`，但它们不证明这四项工业文案、反馈 tone、toast 与 chatter 断言已恢复；本卡片仍需按下方操作链路采集实际产品证据。Bevy bridge 和后端协议的验证边界另见 `testing-manual.md`，不得用任意新测试替代原断言。

## 当前表面映射缺口

下方 A/B/C 的首产物、停机恢复、首座工厂及可读反馈目标继续有效；“右侧工业摘要”、`Factory Runtime Status:`、`Blocked Factories:`、toast/chatter 与对应事件名是原执行表面的定位/oracle 参考，不是已核实的当前 canonical Viewer 入口或逐项等价映射，也不据此断言当前 UI 不存在。执行前须在同一候选明确合法玩家入口、当前可见反馈、权威世界结果及旧信号的对应关系；无法核定的条目标 `blocked/unverified`，保留具体缺口，不用任意页面 smoke、通用 UI 测试或仅 `connected` 代签本卡通过。下列旧定位说明只在该候选映射已核实后使用。

正式 release UI 通过真实可见玩家输入取证，不以 `window.__AW_TEST__` 存在为前提，也不调用 helper 代替玩家动作；只有显式隔离 visual-test 构建及获准范围可用测试 API，状态采样仅作只读辅助。未执行或未映射的 A/B/C 不能汇总为当前玩法通过；当前操作和证据边界沿 `testing-manual.md` S6 与 Viewer 手册。

## 启动与取证
1. 启动产品链路：`./scripts/worktree-harness.sh up`
2. 记录脚本输出的 Viewer URL，并用 `agent-browser` 打开。
3. 至少采集以下证据之一：
   - `agent-browser` 截图
   - 短录屏
   - 显式隔离 visual-test 构建中的只读 `window.__AW_TEST__` 状态采样（不能代替 release UI 操作或截图）
   - 启动日志 / console
4. 正式填写卡片时，`测试场景` 必须使用本卡组中的场景名之一。

## 卡片 A：首个制成品
### 场景名
- `前期工业引导 / 首个制成品`

### 目标
- 验证玩家首次跑通“原料 -> 加工 -> 产出”时，主界面能看见明确的接受、执行与产出反馈。

### 前置条件
- 当前世界存在至少 1 条可执行配方与足量输入材料。
- Viewer 已进入 `connected`，且右侧文本区可见工业链路摘要。

### 操作步骤
1. 选定能发起生产的对象或链路入口。
2. 触发首个可执行配方。
3. 等待配方进入运行态并完成一轮产出。
4. 观察右侧工业摘要、近期生产反馈与玩家 toast / chatter。

### 通过信号
- 工业摘要出现 `Factory Runtime Status:` 或等价本地化块。
- 近期生产反馈至少出现 1 条 `accepted_and_executing` 与 1 条 `produced`。
- 玩家 toast / chatter 能读到“已接受并执行中”与“制成品已产出”的友好文案，而不是原始 debug 文本。
- 正式卡片中的 `关键操作链路` 可以明确写出“首次产出”的动作闭环。

### 失败签名
- 只有数值变化，没有“已接受 / 执行中 / 已产出”显式反馈。
- 产出已完成，但近期生产反馈缺 `produced`。
- toast / chatter 仍显示难以读懂的原始 `RuntimeEvent` 文本。

### 证据要求
- 至少 1 张截图，覆盖 `accepted_and_executing` 或 `produced`。
- 至少 1 条状态采样，证明 Viewer 处于 `connected`。

## 卡片 B：停机恢复
### 场景名
- `前期工业引导 / 停机恢复`

### 目标
- 验证“缺料停机 -> 补料恢复 -> 继续执行”可由状态与事件同时解释。

### 前置条件
- 当前世界存在 1 条可控的缺料路径（例如故意抽空关键材料后再补料）。
- 至少 1 座工厂已经开始过生产。

### 操作步骤
1. 让目标工厂进入运行态。
2. 制造缺料或等价阻塞条件，确认产线停机。
3. 补足材料或解除阻塞条件。
4. 观察工厂恢复到继续执行或重新开始产出。

### 通过信号
- 工业摘要中的 `running/blocked/idle` 计数发生可解释变化。
- `Blocked Factories:` 中能看到目标工厂、阻塞原因与 detail。
- 近期生产反馈至少出现 1 条 `blocked` 与 1 条 `resumed`。
- 玩家 toast / chatter 能读到“产线停机”“产线恢复”的友好文案。

### 失败签名
- 工厂已停机，但工业摘要没有 `blocked` 或没有阻塞原因。
- 工厂已恢复，但近期反馈缺 `resumed`。
- 玩家侧只能看到 ActionRejected，而无法知道是哪个工厂、哪条配方停机。

### 证据要求
- 至少 2 张截图：1 张停机、1 张恢复。
- 若有 console / 状态导出，必须记录阻塞原因字段。

## 卡片 C：首座工厂单元
### 场景名
- `前期工业引导 / 首座工厂单元`

### 目标
- 验证玩家建成第一座工厂时，系统会把“一次性加工”升级为“持续产能节点”的成就感显式表达出来。

### 前置条件
- 当前世界允许完成至少 1 次工厂建造。
- 相关建造资源与建造链路可被玩家触发。

### 操作步骤
1. 准备建厂所需资源。
2. 触发工厂建造。
3. 等待工厂落成。
4. 观察工业摘要、近期生产反馈与玩家 toast / chatter。
5. 若工厂可立即开工，补做一次开工观察。

### 通过信号
- 近期生产反馈出现 `factory_ready`。
- 玩家 toast / chatter 能读到“工厂落成”的友好文案。
- 工业摘要中的工厂运行态块已经能纳入该工厂（即便初始为 `idle` 也应可见）。

### 失败签名
- 工厂已经落成，但没有任何“工厂落成 / factory_ready”主界面提示。
- 新工厂未进入摘要统计，玩家无法确认它已成为长期产能节点。

### 证据要求
- 至少 1 张截图，显示“工厂落成”提示或 `factory_ready`。
- 若工厂后续开工，再补 1 张显示其进入 `running/idle/blocked` 统计。

## 回归结论模板
| 卡片 | 是否执行 | 结论 | 阻断级别 | 证据路径 | 备注 |
| --- | --- | --- | --- | --- | --- |
| 首个制成品 | `yes/no` | `pass/watch/block` | `none/watch/blocker` |  |  |
| 停机恢复 | `yes/no` | `pass/watch/block` | `none/watch/blocker` |  |  |
| 首座工厂单元 | `yes/no` | `pass/watch/block` | `none/watch/blocker` |  |  |

## 收口规则
- 若改动涉及停机 / 恢复事件、工厂摘要或玩家工业提示，`卡片 B` 必跑。
- 若改动涉及首产出提示或产出反馈映射，`卡片 A` 必跑。
- 若改动涉及建厂、工厂纳管或首座工厂引导，`卡片 C` 必跑。
- 任一卡出现 `block`，`TASK-GAME-020` 不得宣称“前期工业引导体验已收口”。
