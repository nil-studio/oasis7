# 文档分工与组织规范正文（2026-03-09）

- 对应需求文档: `doc/engineering/doc-governance/doc-structure-standard.prd.md`
- 历史执行记录: Git history、PR 与实际验证记录；本页不承载任务状态。
- 执行记录: Git、PR、实际 CI 与评审记录；Issue 和 Project 按需使用

## 1. 规范定位
本规范是仓库 `doc/` 文档树的顶层组织约定，用于回答两个问题：
1. 新文档应放在哪个目录；
2. 新文档应承担什么职责。

产品文档的内容合同、REQ/AC 追踪、玩家体验与证据边界由配对的 [`product-documentation-standard.design.md`](product-documentation-standard.design.md) 承接；专业技术设计的内容合同与验证映射由 [`system-design-writing-standard.design.md`](system-design-writing-standard.design.md) 承接，任务记录和组合验收由 [`project-management-record-standard.design.md`](project-management-record-standard.design.md) 承接；本文继续拥有目录落位、文件职责、入口与专题可达性。共同规则维护现有四模块产品树，不新增第五模块。

本规范只规定组织形式与职责边界，不处理历史迁移节奏。

## 2. 核心原则

### 2.1 目录按对象
目录用于表达“正在描述什么对象”，而不是“这是一类什么文档”。

对象从大到小分三层：
- 模块：如 `doc/world-runtime/`、`doc/world-simulator/`。
- 专题：如 `doc/world-simulator/launcher/`、`doc/game/gameplay/`。
- 分册：专题内部因复杂度拆出的子目录或分册集合。

### 2.2 文件按职责
文件后缀用于表达“这份文档回答什么问题”。

推荐职责后缀如下：
- `*.prd.md`：Why / What / Done
- `*.design.md`：How / Structure / Contract
- `system-design-writing-standard.design.md`：专业技术设计的十二段内容、需求承接与验证映射
- `project-management-record-standard.design.md`：按需任务说明与组合验收内容
- Git、PR、实际 CI 与评审记录承载交付事实；Issue/Project 按需追踪工作，不是唯一任务载体或开发准入
- `*.manual.md`：How to use / verify
- `*.runbook.md`：How to operate / release / recover
- `README.md`：目录导航
- `prd.index.md`：专题 PRD 索引

过程与长期内容的统一归属见[开发流程规范](../workflow/source-of-truth.md#information-record-placement)：本页的目录/后缀分类不意味着每个 Task、评审或运行报告都要落成仓库文件。日期化的原始证据、版本身份和发行资产按其职责保留，不按关键词机械删除。

### 2.3 同对象优先同目录
同一个对象的 PRD、Design、Runbook、Manual 优先放在同一目录中，避免读者跨仓库跳转；可变任务记录直接留在 Git、PR 与实际验证记录。

### 2.4 同专题优先同名
同一专题的核心文档优先采用同一个 basename：
- `foo.prd.md`
- `foo.design.md`
- 可选 Issue（跟踪）

这样可以保证读者一眼识别同一专题的不同视角。

<a id="doc-authority-resolution"></a>
### 2.5 权威裁决与有界修复

承接 [GOV-AC-1–5](doc-structure-standard.prd.md#doc-authority-and-validation)。
先识别争议对象、内容类型和声明 scope，再找拥有该内容的既有权威；同一权威内才使用审定版本先后。
core 协调跨域影响，产品层拥有用户承诺，专业层拥有规则和技术合同，工作流拥有日常开发规则；
索引、时间戳与历史任务摘要都不获得跨域覆盖权。范围不明时记录具体冲突、受影响结论及对应责任人，
由现有 PR/评审或按需 Issue 承接，不增加准入、角色证明或放行回执。

修复顺序为“确认仍使用的规则/入口 → 修改既有主责条款 → 同步活跃消费者 → 对该缺口验证”。
共享设计可以承接重复正文，但独立 PRD 的需求和验收不消失；删除前核对路径、锚点、目录约定和动态消费者。
没有当前影响的旧迁移批次不恢复；旧 source、日期、PASS/hold 和 frozen fixture 保持历史语义。

<a id="doc-validation-boundary"></a>
### 2.6 检查实现与验证分配

检查结果按真实错误与编辑提示分开累计。Markdown 长度只能进入提示集合，不进入 failures 或任何上层
baseline/required 的失败条件；有效性错误仍按既有合同失败。help、调用方和回归同步，Rust 体量规则不变。
结构通过不证明语义正确，登记更新不证明审核完成，引用存在也不证明 required 实际运行了检查。

| 上游验收 | 实现责任与判定 | 验证设计（待在实施候选执行） |
| --- | --- | --- |
| GOV-AC-1/2 | 当前 authority 与活跃入口；交付事实回到 PR/CI，任务身份按需 | 较新 core 与较旧专业合同冲突仍按内容归属；活跃消费者不依赖退役 `.pm`/project 入口，历史链接不机械改写 |
| GOV-AC-3 | `scripts/doc-governance-check.sh` 的提示/失败通道及其真实调用者 | 999/1000/1001 行均不因长度非零；各自叠加缺失引用仍失败；检查 help 与 baseline/required 调用，不降低其他错误 |
| GOV-AC-4 | registry 精确 entry/domain/导航、product_overlay、owning-module 与 tools 引用范围 | 错域同前缀、overlay 互换、topic 合法 metadata、tools 中活跃退役引用分别作正负例，不只查总数 |
| GOV-AC-5 | 每个链接及其 fragment 独立绑定，证据入口按真实集合导航 | 同行交叉错绑 ID/fragment 必须检出；PRD 索引未覆盖的 evidence 仍可从真实目录或 corpus 定位 |

当前 v3 只更新受影响对象/组，不能为追平 main 重写旧单体清单或冻结摘要。任何语义复核记录的更新须
保留实际复核范围和未证明事项；自动 sync 不更新批准结论。上述验证分别记录候选、命令和结果，不要求
新增一套全仓治理台账或在无相关改动时重复运行全部历史检查。

## 3. 一级模块组织规范
每个一级模块目录固定保留以下入口：

```text
doc/<module>/
  README.md
  prd.md
  design.md
  prd.index.md
```

职责如下：
- `README.md`：模块目录导航。
- `prd.md`：模块目标、范围、验收与边界。
- `design.md`：模块总体技术设计与阅读入口。
- 可选 Issue（跟踪）：模块级任务拆解、依赖、状态。
- `prd.index.md`：模块内专题 PRD 与 Git、PR 与实际 CI 记录 可达索引。

### 3.1 模块级阅读顺序
固定阅读顺序为：
1. `prd.md`
2. `design.md`
3. 可选 Issue（跟踪）
4. `prd.index.md`
5. 下钻专题目录

### 3.2 模块级 design.md 的职责
模块级 `design.md` 不应重复 `prd.md` 的用户故事，而应承担：
- 模块架构总览
- 核心组件职责划分
- 模块内/跨模块主链路
- 关键数据流与状态流
- 主题分册导航

### 3.3 模块级 README.md 的职责边界
模块级 `README.md` 是 landing page，不是第二份规范正文。

应保留的内容：
- 按读者目标分流到 `prd.md`、可选 Issue（跟踪）、`prd.index.md` 或高频专题；
- 模块特有的入口分工、公开镜像、命名迁移或例外说明；
- 当前最值得优先阅读的少量高频专题。

不应重复维护的内容：
- repo-wide 的根入口集合定义；
- 通用的目录落位规则；
- 各模块都相同的维护约定模板。

这些共享规则统一回链到本规范正文，不在各模块 README 重复维护。

### 3.4 产品组合层的薄覆盖例外

`doc/product/` 是按玩家价值组织的产品组合层，不是与 `game`、`world-runtime`、
`world-simulator`、`p2p` 并列的工程专业域。因此它采用薄产品覆盖层，不适用本章的
四入口要求，也不为形式对称创建空的 `design.md`、可选 Issue（跟踪） 或 `prd.index.md`。

固定根入口为：

```text
doc/product/
  README.md
  world-rules-core-gameplay/prd.md
  world-infrastructure/prd.md
  agents-world-simulation/prd.md
  player-entry-distribution/prd.md
```

这四份 `prd.md` 是唯一的默认产品入口和唯一的产品模块 PRD；
`doc/product/README.md` 的表格必须精确、且只能列出这四行。它不是全部产品
专题的平铺清单。模块目录可以包含活跃产品专题，但专题只能是已存在模块的下钻，
不能成为第五个模块或另一个产品总入口：

```text
doc/product/<module>/
  prd.md
  <topic>.prd.md
  <topic>.design.md     # 可选；存在时必须与 <topic>.prd.md 同名配对

```

专题准入与索引契约：

- 活跃 `<topic>.prd.md` 必须由其所属模块 `prd.md` 的 `### 活跃产品专题` 小节以
  Markdown 链接显式列出；未声明的产品专题不得留在 `doc/product/`。
- 专题只能说明一个产品子主题的承诺、范围、跨域组合和验收边界；它不拥有模块级
  产品承诺、默认入口、专业规则或技术合同的裁决权。
- 同名 `*.design.md` 是可选配对文件；存在时必须显式回链同名 PRD。
  它们不能替代 工作说明或按需 Issue truth，也不能把实现计划、测试步骤、运行处置或
  任务状态复制进产品层。
- 专题必须回链模块 `prd.md`，并将规则、实现、测试、发布/运维和任务证据链接到
  对应专业域权威。产品专题只保留产品组合所需的引用和验收摘要。

职责与权威边界：

- `doc/product/README.md` 是唯一产品总入口，只枚举且必须枚举上述四个产品模块。
- 产品模块 PRD 拥有产品承诺、范围、跨域组合、非目标和端到端成功标准。
- 专业域 `prd.md` 拥有该域规则、实现合同、专业 PRD-ID 和测试机制；`design.md` 拥有技术 How。
- 实际实现与验证由 Git、PR、CI 和评审记录承载，Issue/Project 按需跟踪，产品 PRD 不维护实现任务表。
- 根 `README.md` 拥有公开的当前状态与 claim envelope；“玩家接入与发行”PRD 只组合发现、访问、安装和验证体验。

冲突时，产品 PRD 对用户价值、产品范围和端到端结果拥有裁决权；专业域 PRD 对其拥有的规则与
技术合同拥有裁决权。若产品承诺与可实现性冲突，必须建立显式跨域决策或 工作说明或按需 Issue，不得在任一文档中
静默覆盖另一层权威。

每份活跃产品模块 PRD 必须使用稳定 Product PRD-ID，声明模块 slug/名称、生命周期、owner、复核日期、后继文档、
产品入口和下层权威。生命周期只允许 `proposed`、`draft`、`active`、`superseded`、`retired`；默认四模块表中只允许
`active` 文档。`superseded` 必须指向后继者并退出默认入口，`retired` 必须同步公开 claim 与下游权威。活跃产品专题
不分配并列 Product PRD-ID；它以模块入口中的显式索引和同名配对关系取得可达性。

每个产品成功标准必须在追踪表中有唯一可定位的追踪行，保留专业 owner、专业域 PRD-ID、权威文档、验证证据和测试层级；同一行可以关联多个适用专业条款和证据，一项专业条款也可服务多个成功标准，不是要求产品与专业文件一对一。
产品入口、产品 PRD 与专业域权威文档必须双向可达。

### 3.5 一级物理目录 registry 与例外

`doc/` 的物理目录树与 `doc/product/` 的四模块逻辑产品树是两层 taxonomy：专业域、治理域、测试/证据域和受控例外目录不会成为第五个产品模块。

`doc/.governance/top-level-directory-registry.json` 是一级物理目录的机器可读结构登记。每个 live `doc/` 一级目录必须登记 `name`、`type`、`owner`、`entry` 和 `reason`；`entry` 必须位于本记录的 domain 内，并在 `doc/README.md` 的人工阅读导航中精确可达；只匹配目录前缀不充分。`product_overlay` 类型只允许用于 `product` 记录，其他记录不得占用。registry 负责结构真值和可验证性，README 负责读者路径，不从 registry 自动生成完整文件系统清单。

例外目录还必须登记非空的 `entry_conditions`、`review_trigger` 与 `exit_conditions`。历史摘要与短周期证据池只可按其例外条件存在，不得默认为长期模块或运行态真值。新增、删除或重新分类一级目录时，必须同步更新 registry、根 README 和相关 landing page，并运行文档治理检查。

### 3.6 证据生命周期

证据目录不能以“已有 README”为理由无限增长。`qa_engineer` 拥有证据有效性、保留与删除语义；`repository_health_engineer` 拥有密度、导航和阈值治理。count、age 或 duplication 达到子域 README 定义的触发器时，必须发起聚合、归档或删除复核；复核不等于自动删除，也不能改变发布或领域正确性的 owner。

### 3.7 Legacy Redirect 的职责边界
root-level legacy redirect 只承担兼容跳转职责。

推荐最小结构：
- 一个明确的标题；
- 当前主入口；
- 必要时补一个相关入口（如配套的 可选 Issue（跟踪） / `design.md`）；
- “仅保留兼容跳转，不承载正文”的声明。

不再推荐的做法：
- 为 redirect 再写一整套 `目标 / 范围 / 里程碑 / 风险`；
- 为 redirect 补完整的任务状态壳；
- 把 redirect 继续当作活跃业务入口维护。

## 4. 专题级组织规范
对于单一专题，优先使用下列PRD/Design 最小文档集；这不是每个产品专题都必须新建同名专业设计的要求。专业技术义务可以由准确链接的既有专业设计条款承接，按适用义务保持多对多追踪；产品体验层原有同名配对及其适用条件仍按产品内容规范执行：

```text
doc/<module>/<topic>/
  <topic>.prd.md
  <topic>.design.md
```

当专题需要操作说明时，再追加：

```text
  <topic>.manual.md
  <topic>.runbook.md
```

### 4.1 专题文档角色边界
- `<topic>.prd.md`
  - 说明为什么做、做成什么、如何验收。
  - 不写任务拆解，不写每日进度。
- `<topic>.design.md`
  - 说明系统怎么设计、如何分层、接口契约、状态机、错误处理。
  - 不写任务排期，不写单日开发日志。
- 可选 Issue（跟踪）
  - 说明怎么拆任务、先后顺序、依赖、owner、状态。
  - 不重写目标态需求。
- `<topic>.manual.md`
  - 说明如何使用、如何操作、如何验证。
- `<topic>.runbook.md`
  - 说明如何发布、值班、排障、回滚、恢复。

### 4.1.1 测试相关文档分工
测试专题或功能专题中的测试内容，按“要求 / 设计 / 步骤 / 值班剧本”四层拆分：

- `*.prd.md`
  - 写测试目标、覆盖范围、门禁等级、验收口径。
  - 适合描述 `test_tier_required/full`、必须覆盖的用户路径、通过标准与证据要求。
  - 不写逐步点击、逐条命令、逐屏检查细节。
- `*.design.md`
  - 写测试系统或测试方案如何设计。
  - 适合描述分层测试策略、自动化架构、fixture/mock、测试 API、观测点、采证模型、失败注入与覆盖矩阵。
  - 当测试本身涉及状态机、时序、并发、桥接协议或数据采集模型时，应放在 `*.design.md`。
- `*.manual.md`
  - 写详细执行步骤。
  - 适合承载手动测试步骤、自动测试执行步骤、环境准备、启动命令、操作顺序、预期结果、截图点、结果记录方式。
  - 这是“详细测试步骤”的默认落点。结果记录方式、模板和输出字段可以留在手册；每次实填运行日志与后续修复/CI 流水回到对应 PR/Task，原始 evidence 按其保留合同维护。
- `*.runbook.md`
  - 写发布前验证、线上巡检、故障排查、回滚验证、值班处置剧本。
  - 只要测试步骤明显偏向运行值守或发布处置，就不应继续放在 `*.manual.md`。

判断规则：
- 测试“要测什么、算什么通过” -> `*.prd.md`
- 测试“为什么这样测、测试系统怎么搭” -> `*.design.md`
- 测试“具体怎么一步一步执行” -> `*.manual.md`
- 测试“线上/发布/故障场景怎么验证和处置” -> `*.runbook.md`

### 4.2 什么时候必须补 `*.design.md`
满足以下任一条件时，专题必须补齐适用的专业设计说明；可更新或链接真实承接本义务的既有 `*.design.md` 条款，无须为同名配对重复建文件：
- 存在明确的组件边界；
- 存在 API / 协议 / 接口契约；
- 存在状态机、并发语义或时序约束；
- 存在异常处理、回滚、兼容性设计；
- PRD 已经开始承载“怎么实现”的细节。

### 4.3 什么时候可以暂不写 `*.design.md`
当专题同时满足以下条件时，可以只更新适用的既有 PRD 条款并使用 PR 与实际验证记录；没有独立长期要求的简单编辑无需新建 PRD：
- 范围很小；
- 没有新增结构设计；
- 没有独立接口或状态机；
- 预期不会演化为长期维护主题。

一旦条件失效，应立即补齐适用设计义务，可由既有专业设计承接；尚未形成的设计如实标为缺口，不以文件存在或数量相等声称已覆盖。

## 5. 分册级组织规范
当 `*.design.md` 或专题内容过长时，可在专题目录下继续拆分分册。

推荐形式：

```text
doc/<module>/<topic>/
  <topic>.design.md
  design/
    <topic>-architecture.design.md
    <topic>-interfaces.design.md
    <topic>-state-machine.design.md
```

规则：
- 保留一个总入口 `<topic>.design.md`。
- 分册命名继续显式携带 `.design.md`，不要退回自由命名。
- 总入口负责索引分册，不要求读者直接从分册开始。

## 6. 命名规则

### 6.1 推荐命名
- 模块根入口：固定名 `prd.md` / `design.md` / 可选 Issue（跟踪）。
- 专题文档：`<topic>.prd.md` / `<topic>.design.md` / 可选 Issue（跟踪）。
- 分册文档：`<topic>-<aspect>.design.md` / `manual.md` / `runbook.md`。

### 6.2 不推荐命名
以下命名不应再作为新专题的主入口类型：
- `*architecture*.md`
- `*interface*.md`
- `*integration*.md`
- `*overview*.md`

这些名字可以作为 `*.design.md` 的分册名，但不再作为统一入口类型。

## 7. 引用关系规范
文档间应形成固定的引用链：

```text
doc/README.md
  -> doc/<module>/prd.md
  -> doc/<module>/design.md
  -> 可选 Issue（跟踪）

 doc/<module>/prd.md
  -> doc/<module>/design.md
  -> 可选 Issue（跟踪）
  -> doc/<module>/prd.index.md

 doc/<module>/<topic>/<topic>.prd.md
  -> <topic>.design.md
  -> 可选 Issue（跟踪）
```

专题级文档与实际执行记录按适用关系关联：
- PRD 推荐指向适用 Design；Design 回链真实上游条款。
- 涉及新增或实质修改的跨层要求时，按[全局要求追踪](cross-layer-requirements-traceability.design.md)在原条款声明 Trace ID，设计和 Task 引用同一 ID 并保留准确路径/锚点。
- Task 可以直接是 PR，链接实际要求、适用设计与验证结果；尚无任务或设计时如实说明，不制造占位记录或要求规范正文镜像每次交付状态。

注：交付事实使用 Git、PR、实际 CI 和评审记录，Issue/Project 按需使用，不要求额外台账。

## 8. 例外规则
以下情况允许偏离PRD/Design 最小文档集，但必须说明原因：
- 纯索引文档：如 `README.md`、`prd.index.md`。
- 纯手册文档：对外或对内操作手册，不承载需求和设计。
- 纯运行手册：如发布、故障处理、回滚剧本。
- 历史保留文档：暂时保留旧命名，但不作为新增建档模板。

例外规则不应成为绕过 `*.design.md` 的常用手段。

## 9. 建档决策树
新增文档前，按以下顺序判断：
1. 这是哪个模块/专题对象？先确定目录。
2. 这份文档回答的是 Why/What/Done 还是 How/Structure/Contract 还是 How/When/Who？再确定后缀。
3. 如果同时涉及需求、设计、执行，拆成多份文档，不混写在一份文档中。
4. 如果是操作步骤，判断是 `manual` 还是 `runbook`，不要落到 `design`。

## 10. 推荐模板组合

### 10.1 模块级
```text
doc/<module>/
  README.md
  prd.md
  design.md
  prd.index.md
```

### 10.2 常规专题级
```text
doc/<module>/<topic>/
  <topic>.prd.md
  <topic>.design.md
```

### 10.3 带操作文档的专题
```text
doc/<module>/<topic>/
  <topic>.prd.md
  <topic>.design.md
  <topic>.manual.md
  <topic>.runbook.md
```

### 10.4 超大专题级
```text
doc/<module>/<topic>/
  <topic>.prd.md
  <topic>.design.md
  design/
    <topic>-architecture.design.md
    <topic>-interfaces.design.md
    <topic>-state-machine.design.md
```

## 11. 裁定原则
当文档落位出现争议时，按以下优先级裁定：
1. 是否符合“目录按对象、文件按职责”。
2. 是否能让读者在最短路径内找到同一专题的配套文档。
3. 是否把需求、设计、执行、操作混在了一起。
4. 是否保留稳定入口，避免每次都重新学习命名。

如果一个方案同时满足以上四点，则视为合规。
