# game 模块设计总览

审计轮次: 6

- 对应需求文档: `doc/game/prd.md`
- 对应文件级索引: `doc/game/prd.index.md`

## 1. 设计定位
`game` 模块的 `design.md` 是玩法层的总体设计入口，回答：
- 玩法规则、经济系统、协作治理和发行可玩性如何分层；
- 玩法目标如何映射到运行时、测试与发行门禁；
- 哪些能力留在模块级设计，哪些能力继续下钻到专题 `*.design.md`。

## 2. 阅读顺序
1. `doc/game/prd.md`
2. `doc/game/design.md`
3. `doc/game/prd.index.md`
4. 下钻 `gameplay/` 等专题目录；执行状态从对应 GitHub task issue 获取。

## 3. 设计结构

### 组件职责与分层

- 规则层：定义核心循环、资源与治理边界。
- 系统层：定义 gameplay、发行可玩性、长期在线 P0 的结构分层。
- 验证层：通过 `testing-manual.md`、`doc/testing/*` 和可玩性结果验证玩法质量。

### 玩法到验证的主链路

玩法目标与规则由[活跃玩法基线](prd.md#2-active-gameplay-baseline)路由到声明范围内的专题合同，再由 runtime / world-simulator / viewer / agent 提供实现、观测与验证支撑；发布前可玩性结论回到 `testing-manual.md`、`doc/playability_test_result/` 与 `doc/testing/evidence/`。这是[模块架构与验证关系](prd.md#4-technical-specifications)，不是某个候选已经完成实现或通过验收的声明。

[PRD-GAME-009/010](prd.md#2-active-gameplay-baseline)要求阶段与对外口径绑定同候选证据、QA、产品决策与 LiveOps 同步；gate pass 不自动升阶。证据或执行状态与基线冲突时，按[权威边界](prd.md#3-player-facing-authority-boundary)保留 blocker 并交对应 owner，不直接改写玩家承诺。

### 玩家反馈与验证状态流

- [PRD-GAME-004/014](prd.md#2-active-gameplay-baseline)约束玩家侧状态反馈：Viewer 与 pure API 从同一权威状态提供有效动作、不可用原因和解锁/恢复路径；动作接受、推进、阻塞、反馈与下一步必须可辨认。请求/提案接受不等同于权威规则应用或后果发生；因果反馈保留 reason/evidence、预期后果、替代动作、纠正及纠正后结果。
- 本页只汇总上述玩法语义，不新增 snapshot 字段、UI 或运行时状态机。实际动作与恢复行为按专题合同及专业实现验证；[执行记录与正式 evidence](prd.md#3-player-facing-authority-boundary)分别承载执行状态和验证结果，历史样本不能替代当前候选证据。

## 4. 集成点
- `doc/testing/prd.md`
- `doc/playability_test_result/prd.md`
- `doc/world-runtime/prd.md`
- `doc/world-simulator/prd.md`

## 5. 专题导航
- 玩法专题继续放在 `doc/game/gameplay/`
- 规则和经济相关设计进入对应专题 `*.design.md`
- 发布与可玩性闭环通过 testing / playability 侧文档联动

## 设计目标
- 提供 `game` 模块的总体设计入口。

## 设计范围
- 覆盖模块级结构、主链路、分层与专题导航。
- 不替代专题 `*.design.md` 的细化设计。

## 关键接口 / 入口
- 需求入口：`doc/game/prd.md`
- 执行状态：对应 GitHub Project task 与 issue evidence comments
- 索引入口：`doc/game/prd.index.md`

## 设计演进计划
- 历史建档（2026-03-09，ROUND-006）：模块级 `design.md` 入口来源；原 Task 未唯一定位，记录保留，不作为当前排期。
- 持续结构要求：高复杂度专题在对应 `*.design.md` 维护细化合同；不恢复旧 M2 任务。

## 设计风险
- 若专题设计未及时补齐，模块级 `design.md` 可能承载过多导航职责。
- 若 legacy redirect 未明确标注为兼容跳转，读者可能误判历史入口为当前执行入口。
