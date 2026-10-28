# core 模块设计总览

审计轮次: 6

- 对应需求文档: `doc/core/prd.md`
- 可变项目管理真值: PR、实际 CI 与评审记录（Issue 按需）
- 对应文件级索引: `doc/core/prd.index.md`

## 1. 设计定位
`core` 模块的 `design.md` 是项目级设计总入口：
- 说明全仓文档与模块设计如何组织；
- 给出跨模块评审与治理的阅读顺序；
- 指向各模块的详细内容，不在 core 重复展开。

本文件不重写模块需求，也不替代各模块实现设计。

## 2. 阅读顺序
1. `doc/core/prd.md`：项目目标、模块地图、关键链路与治理基线。
2. `doc/core/design.md`：跨模块设计结构、入口分工、主链路导航。
3. PR、实际 CI 与评审记录（Issue 按需）：任务、依赖、状态与治理轮次。
4. `doc/core/prd.index.md`：核心专题索引与后续扩展入口。
5. 下钻到目标模块：`doc/<module>/README.md`、`prd.md` 与 `design.md`；任务状态从对应 PR、实际 CI 与评审记录（Issue 按需） 读取。

## 3. 设计结构
### 3.1 分层角色
- `doc/README.md`：仓库文档总导航。
- `doc/core/prd.md`：项目级 Why / What / Done。
- `doc/core/design.md`：项目级 How / Structure / Contract。
- PR、实际 CI 与评审记录（Issue 按需）：项目级可变 How / When / Who。
- `doc/<module>/*`：模块内具体设计与专题落地。

### 3.2 core 的设计边界
- 在 core 固化：模块地图、统一术语、跨模块链路、治理规则。
- 不在 core 固化：模块内部组件实现、专题级 API 细节、运行手册步骤。
- 若某个问题需要进入具体实现层，必须继续下钻到对应模块 `design.md` 或专题 `*.design.md`。

### 3.3 跨模块主链路
- 设计链路：`core -> <module>/prd.md -> <module>/design.md -> <topic>.design.md`
- 执行链路：`适用要求与修改范围 -> PR、实际 CI 与评审记录 -> 受影响的长期要求/设计与原始证据`；Issue/Project 按需使用，普通修改可以只有 PR。
- 治理链路：`engineering/doc-governance 的适用规范 -> 模块/专题改造 -> 实际 PR 与检查/评审`；通用流程由[开发流程规范](../engineering/workflow/source-of-truth.md)主责，历史 ROUND 仅作来源。

### 3.4 当前交付依赖

当前目标、全局 P0 和退出条件以 [core PRD](prd.md#当前阶段交付目标与全局-p0)为准。先收敛合同与指标，再并行完成持久单权威与 canonical Smelter 体验核验；二者满足首次开放条件后开展受控真人两次会话。BFT 可并行研发，协议及同世界交接验收后才获得新阶段写权。

game 拥有首产物与持续能力边界；runtime 拥有效果、授权与幂等；P2P 拥有合法 profile、提交证明和 H/H+1 交接；基础设施产品域拥有连续性与恢复服务承诺；Viewer/QA 分开汇总样例结构、候选 live 和真人观察。core 路由依赖与证据，不复制状态机或建立并行台账。

固定世界身份与合法历史贯穿所有里程碑。网络环境、发行阶段、世界保留、权威与经济语义不能互相代签；隔离开发世界不能并入长期世界。已有实现、尚未闭合能力与目标承诺分别表达，可变通过结论回链候选与证据窗口。

### 3.5 数据流与状态流

各产品/专业 authority 提供要求与技术边界，core 将其映射为模块职责、主链路和阅读入口，再由模块设计承接具体实现。当前候选、检查、评审与交付状态由 Git、PR 和实际验证记录提供；core 只维护稳定依赖与证明边界，不将历史审计标记或目标规划转换为当前通过结论。

## 4. 集成点
- `doc/README.md`
- `doc/core/prd.md`
- `doc/core/prd.index.md`
- `doc/engineering/doc-governance/doc-structure-standard.prd.md`
- 各模块入口：`doc/<module>/README.md`、`doc/<module>/prd.md`、`doc/<module>/design.md`

## 5. ROUND-006 入口职责
本节保留历史入口分工；现行文档治理仍由对应专业规范承接，不恢复 ROUND 台账或强制 Issue。
- ROUND 审计历史由 Git history 和 PR、实际 CI 与评审记录（Issue 按需） 追溯；`core` 不维护可变任务台账。
- 文档结构治理的裁定依据来自 `doc/engineering/doc-governance/doc-structure-standard.prd.md`。
- 模块入口补齐、专题文档治理和索引回写的实际修改、检查与评审在当前 PR 中可追溯；已有相关 Issue 时按需关联，只读分析无需创建任务。

## 设计目标
- 提供 `core` 模块的总体设计入口。

## 设计范围
- 覆盖模块级结构、主链路、分层与专题导航。
- 不替代专题 `*.design.md` 的细化设计。

## 关键接口 / 入口
- 需求入口：`doc/core/prd.md`
- 执行入口：PR、实际 CI 与评审记录（Issue 按需）
- 索引入口：`doc/core/prd.index.md`

## 设计演进计划
以下保留 ROUND-006 的历史规划来源，不表示当前排期或已完成验收；当前任务与结果以实际 PR、CI 和评审记录为准，Issue/Project 按需使用。
- 历史 M1 (2026-03-09): 在 ROUND-006 中补齐模块级 `design.md` 标准入口。
- 历史 M2（原规划，完成情况未在此核实）: 按专题继续补齐高复杂度主题的 `*.design.md`。

## 设计风险
- 若专题级设计未及时补齐，模块级 `design.md` 可能承载过多导航职责。
- 若 legacy redirect 未明确标注为兼容跳转，读者可能误判历史入口为当前执行入口。
