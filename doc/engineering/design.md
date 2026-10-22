# engineering 模块设计总览

审计轮次: 6

- 对应需求文档: `doc/engineering/prd.md`
- 执行记录: Git、PR、实际 CI 与评审记录；Issue 和 Project 按需使用
- 对应文件级索引: `doc/engineering/prd.index.md`

## 1. 设计定位
`engineering` 模块的 `design.md` 承担工程治理的总体设计入口：
- 说明工程规则如何从规范文档落到门禁、索引、迁移与审读流程；
- 说明文档治理、角色协作与质量门之间的关系；
- 指明工程规范专题的衔接方式，并保留结构治理轮（如 ROUND-006）的历史来源。

## 2. 阅读顺序
模块级当前导航统一从 `doc/engineering/README.md` 开始；本设计文档只在进入模块三件套后承载结构设计、规则载体与执行链路，不再复制一套根入口顺序。

专题精确检索由 `doc/engineering/prd.index.md` 承担；历史 PRD 审读与 legacy 迁移证据见 Git history。

## 3. 设计结构
### 3.1 规则载体分层
- `prd.md`：定义工程规则 Why / What / Done。
- `design.md`：定义规则如何组织、如何落地、由哪些脚本与文档承接。
- Git、PR、实际 CI 和评审记录：承载交付事实；Issue/Project 按需要跟踪复杂工作的依赖和进度。
- `doc-governance/*`：定义 `doc/` 文档树的组织规范、默认阅读面减重规则与早期文档治理收口。
- `rust-governance/*`：定义 Rust 文件体量、长期零扫描与语义化结构拆分契约；不维护冻结基线或一次性 burn-down 台账。
- `command-center/*`：开发指挥中心专题；[产品设计](command-center/command-center.prd.md) 拥有云端与本地的用户行为和验收，[系统设计](command-center/command-center.design.md) 拥有 Sites 网页、云端后端与个人数据、本地宿主及工具、来源身份、安全导航和恢复合同，通用流程仍由 workflow source of truth 主责。
- 历史 PRD 审读/治理台账：保留在 Git history logs；当前新增/变更文档追踪由模块入口与 doc governance 规则承接。

### 3.2 工程治理执行链路
- 规范定义：`doc/engineering/doc-governance/*`、`doc/engineering/rust-governance/*`、`doc/engineering/governance/*`
- 项目追踪：按需使用 Issue/Project；结果查看 PR、实际 CI 与评审记录（Issue 按需）
- 过程记录：PR、实际 CI 与评审记录（Issue 按需）；历史 `.pm/tasks/*` 记录见[固定版本归档](https://github.com/nil-studio/oasis7/blob/4d8c742a9f5eca75c02a4156678d71ccc7e87d53/.pm/github-project-sync/task-archive.jsonl)，不是当前任务入口
- 静态校验：`scripts/doc-governance-check.sh`
- ROUND 台账：Git history

### 3.3 文档结构治理链路
- 权威规则：`doc/engineering/doc-governance/doc-structure-standard.prd.md`
- 规范正文：`doc/engineering/doc-governance/doc-structure-standard.design.md`
- 当前执行记录：Git、PR、实际 CI 与评审记录；Issue/Project 按需使用
- 历史逐文档执行面：Git history（compact snapshot entrypoint；pre-compaction 逐行证据见 git snapshot），不作为当前任务台账

### 3.4 主链路、数据流与状态流

专业规范定义规则，模块入口与专题文档承接规则，检查脚本读取受影响文件并输出检查结果。Git、PR、实际 CI 和评审记录承载修改与验证事实，按需 Issue/Project 展示跟踪状态；历史 Git snapshot 只保存当时记录，不代表当前已验证。检查结果不替代专业语义评审，适用门禁与合入条件仍由[开发流程规范](workflow/source-of-truth.md)规定。

## 4. 集成点
- `AGENTS.md`
- `.agents/roles/*.md`
- `.agents/roles/templates/*.md`
- `doc/engineering/doc-governance/doc-structure-standard.prd.md`
- `doc/engineering/doc-governance/doc-structure-standard.design.md`
- Git history logs
- `scripts/doc-governance-check.sh`
- `testing-manual.md`

## 5. ROUND-006 入口职责
以下为历史治理分工，不恢复该轮执行义务。`engineering` 当时负责提供 ROUND-006 的规则定义与裁定标准；原治理方法要求发现规范空白时先回写专业规范。现行规则冲突与有界修复按[文档规范](doc-governance/doc-structure-standard.design.md#25-权威裁决与有界修复)和[开发流程规范](workflow/source-of-truth.md)处理。

## 设计目标
- 提供 `engineering` 模块的总体设计入口。

## 设计范围
- 覆盖模块级结构、主链路、分层与专题导航。
- 不替代专题 `*.design.md` 的细化设计。

## 导航边界
- 模块 landing 与当前分流：`doc/engineering/README.md`
- 本文：工程治理的结构设计、规则载体与执行链路。
- 专题文件级精确检索：`doc/engineering/prd.index.md`

## 设计演进计划
以下保留 ROUND-006 的历史规划来源，不表示当前排期或已完成验收；当前任务与结果以实际 PR、CI 和评审记录为准，Issue/Project 按需使用。
- 历史 M1 (2026-03-09): 在 ROUND-006 中补齐模块级 `design.md` 标准入口。
- 历史 M2（原规划，完成情况未在此核实）: 按专题继续补齐高复杂度主题的 `*.design.md`。

## 设计风险
- 若专题级设计未及时补齐，模块级 `design.md` 可能承载过多导航职责。
- 若 legacy redirect 未明确标注为兼容跳转，读者可能误判历史入口为当前执行入口。
- 若治理专题继续回流到 `engineering` 根目录，根入口会重新退化为专题堆放区。

## 私有资料工具

普通用户态资料工具与 signer 共用纯加密实现，详细边界见 [私有资料系统设计](private-context/private-context.design.md)。
