# headless-runtime PRD（原 nonviewer）

审计轮次: 6

命名说明：
- 模块目录已统一为 `headless-runtime`。
- 为保持历史追踪连续性，PRD-ID 继续沿用 `PRD-NONVIEWER-*`。

## 目标
- 建立 headless-runtime 模块设计主文档，统一需求边界、技术方案与验收标准。
- 确保 headless-runtime 模块后续改动可追溯到 PRD-ID、任务和测试。

## 范围
- 覆盖 headless-runtime（原 nonviewer）模块当前能力设计、接口边界、测试口径与演进路线。
- 覆盖稳定 headless-runtime 要求及验证方法；本次要求与执行证据的关联由对应 PR/已有 Task 承接。
- 不覆盖实现代码逐行说明与历史过程记录。

## 接口 / 数据
- PRD 主入口: `doc/headless-runtime/prd.md`
- 项目管理入口: 对应 PR、实际 CI 与评审记录（Issue 按需）；分工见[开发流程规范](../engineering/workflow/source-of-truth.md#information-record-placement)
- 文件级索引: `doc/headless-runtime/prd.index.md`
- 追踪主键: `PRD-NONVIEWER-xxx`
- 测试与发布参考: `testing-manual.md`

## 里程碑
- M1 (2026-03-03): 完成模块设计 PRD 主体重写与任务改造。
- M2: 补齐模块设计验收清单与关键指标。
- M3: 建立 PRD-ID -> Task -> Test 的长期追踪闭环。

## 风险
- 模块边界演进快，文档同步可能滞后。
- 指标口径不稳定会降低验收一致性。
## 1. Executive Summary
- Problem Statement: headless-runtime（原 nonviewer）链路承担长稳运行、协议交互、鉴权与归档，但设计信息分散，影响生产链路一致性和故障定位效率。
- Proposed Solution: 以 headless-runtime PRD 统一定义无界面运行链路的目标、接口、稳定性指标与安全约束。
- Success Criteria:
  - SC-1: headless-runtime 关键链路（启动、鉴权、归档）均有可追溯 PRD-ID。
  - SC-2: 长稳运行用例在 `test_tier_full` 下可复现且通过率达到目标阈值。
  - SC-3: 鉴权协议变更具备文档、测试、回归三方联动记录。
  - SC-4: 线上问题可通过日志/归档定位到明确阶段与责任模块。

## 2. User Experience & Functionality
- User Personas:
  - 运行链路维护者：需要稳定的 headless-runtime 生命周期定义。
  - 协议开发者：需要明确鉴权与状态同步边界。
  - 运维/发布人员：需要故障可追溯与可回滚策略。
- User Scenarios & Frequency:
  - 生命周期维护：每次启动流程改动前后各执行 1 次核验。
  - 协议兼容检查：每次鉴权协议变更执行回归。
  - 长稳巡检：按周执行长稳与归档链路检查。
  - 线上事故复盘：每个高优事故后回补追溯证据。
- User Stories:
  - PRD-NONVIEWER-001: As a 维护者, I want a deterministic headless-runtime lifecycle, so that online stability improves.
  - PRD-NONVIEWER-002: As a 协议开发者, I want explicit auth and transport contracts, so that client/server evolution stays compatible.
  - PRD-NONVIEWER-003: As an 运维人员, I want traceable archive and incident evidence, so that postmortems are efficient.
- Critical User Flows:
  1. Flow-NV-001: `启动 headless-runtime -> 完成鉴权 -> 进入稳定运行 -> 周期归档`
  2. Flow-NV-002: `协议变更 -> 执行兼容回归 -> 校验连接与鉴权结果 -> 放行`
  3. Flow-NV-003: `线上异常 -> 回放归档证据 -> 定位阶段 -> 输出修复动作`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 生命周期管理 | 启动参数、鉴权状态、运行阶段、归档标记 | 启动/停止/重连 | `init -> auth -> running -> archived` | 阶段按时序推进，不允许跳级 | 运维可执行，开发可观测 |
| 协议鉴权链路 | 节点身份、签名、令牌、超时阈值 | 发起鉴权并返回结构化结果 | `pending -> accepted/rejected` | 失败按错误类型分类 | 未授权节点拒绝接入 |
| 归档追溯 | 时间戳、链路ID、错误签名、恢复动作 | 失败时自动归档并可检索 | `captured -> indexed -> replayed` | 按事故等级优先检索 | 仅授权人员可读取完整归档 |
- Acceptance Criteria:
  - AC-1: headless-runtime PRD 定义生命周期、鉴权、归档三条主线。
  - AC-2: 对应 PR/已有 Task 记录本次范围、适用要求、验证版本与结果；Issue/Project 按需，长期 PRD 不承载可变任务状态。
  - AC-3: 鉴权、防重放、恢复与归档语义与当前协议、runtime storage authority 及定向回归一致。
  - AC-4: 对外行为变更时同步补齐测试证据；任务执行证据记录在对应 PR、实际 CI 与评审记录（Issue 按需）中。
- Non-Goals:
  - 不在本 PRD 中重写 viewer UI 行为。
  - 不替代 p2p 共识层详细设计。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 长稳脚本、协议回归测试、日志归档与审计工具。
- Evaluation Strategy: 以连续运行稳定性、鉴权失败率、故障恢复时长、可追溯证据完整度评估。

## 4. Technical Specifications
- Architecture Overview: headless-runtime 作为无界面运行链路，连接 runtime/p2p 能力，支持生产运维，强调协议一致性与可恢复性。
- Integration Points:
  - `crates/oasis7_proto/src/viewer.rs` 的 `PlayerAuthProof` 与受保护请求字段。
  - `crates/oasis7_client_api/src/viewer_auth.rs`、simulator live auth 路由与 `WorldModel.player_auth_last_nonce`。
  - `doc/world-runtime/runtime/runtime-storage-footprint-governance.prd.md` 的 checkpoint、replay、retention 与 GC 合同。
  - `testing-manual.md`
- Auth and replay contract:
  - 签名负载必须版本化、规范化，并绑定动作类型、关键业务字段、`player_id`、`public_key` 与 nonce；proof 与请求字段不一致、签名篡改或缺失 proof 必须显式拒绝。
  - 每个 player 的 nonce 必须严格单调递增；服务端在业务授权与业务逻辑之前完成 proof 校验并消费 nonce，重复 nonce 显式返回 replay rejection。
  - 已接受 nonce 随 snapshot 持久化并在恢复后继续生效，防重放不能因重启或 replay 静默回退。
- Long-run retention contract:
  - pending actions、动态 peer、committed batches 与热日志必须有明确容量、TTL 或热窗口边界；达到边界时保留可诊断计数与拒绝、跳过或淘汰原因。
  - 冷数据使用内容寻址 blob 与小型 refs/index 保持可追溯性；headless 层不承诺单机冷数据永久可得，复制、checkpoint、retained-height replay 与 GC 服从 world-runtime storage authority。
  - 事故追溯继续使用 `templates/longrun-archive-incident-template.md`；敏感 proof、key 与凭据不得进入未脱敏归档。
- Edge Cases & Error Handling:
  - 网络中断：连接断开时进入可重连状态并保留会话诊断。
  - 鉴权失败：返回明确拒绝原因，不允许进入运行态。
  - 空归档：归档写入失败时需保底本地落盘并告警。
  - 超时：协议握手超时后自动释放资源并记录失败签名。
  - 并发冲突：重复启动请求必须幂等处理，避免多实例争用。
  - 数据损坏：归档校验失败时阻止回放并输出修复建议。
- Non-Functional Requirements:
  - NFR-NV-1: 长稳运行路径可持续执行并周期产出归档证据。
  - NFR-NV-2: 鉴权失败场景可在 5 分钟内完成初步定位。
  - NFR-NV-3: 协议变更兼容回归覆盖率 100%。
  - NFR-NV-4: 归档证据不得泄露敏感凭据字段。
  - NFR-NV-5: 关键异常恢复路径需具备可重复演练能力。
- Security & Privacy: 非 viewer 链路涉及鉴权密钥与节点身份，必须执行最小权限、日志脱敏、签名校验和审计留痕。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03-03): 固化 headless-runtime 生命周期与协议边界。
  - v1.1: 补齐归档与回滚流程的一致性验证。
  - v2.0: 形成在线稳定性与鉴权质量的长期趋势指标。
- Technical Risks:
  - 风险-1: 协议版本演进导致兼容性回归。
  - 风险-2: 归档链路异常时影响问题追溯完整性。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-NONVIEWER-001 | TASK-NONVIEWER-001/002/005 | `test_tier_required` | 生命周期阶段校验、长稳抽样回归 | 运行稳定性与阶段一致性 |
| PRD-NONVIEWER-002 | TASK-NONVIEWER-002/003/005 | `test_tier_required` + `test_tier_full` | 协议兼容测试、鉴权失败分支覆盖 | 协议兼容性与接入安全 |
| PRD-NONVIEWER-003 | TASK-NONVIEWER-003/004/005 | `test_tier_full` | 归档追溯演练、事故复盘模板检查 | 故障定位效率与审计完整性 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-NV-001 | headless-runtime 维持独立生命周期定义 | 复用 viewer 生命周期 | 生产链路目标不同，需独立约束。 |
| DEC-NV-002 | 鉴权失败必须结构化返回 | 仅写日志 | 结构化信息更利于自动化回归。 |
| DEC-NV-003 | 归档链路失败时保底落盘 | 失败即丢弃 | 保障线上事故可追溯性。 |
