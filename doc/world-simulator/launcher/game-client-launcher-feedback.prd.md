# 客户端启动器反馈（当前 authority）

> **职责与承接边界**：玩家反馈与支持入口统一收敛至正式支持路径与 Web `viewer`；本文承载 `oasis7_web_launcher` `/api/chain/feedback` 代理边界、反馈结果不代签世界成功以及最小诊断脱敏原则。

- 对应设计: `doc/world-simulator/launcher/game-client-launcher-feedback.design.md`
- 历史迁移、验证与 task 状态：GitHub task issue evidence。

## 目标

- 受支持反馈入口提交 `Bug` 或 `Suggestion` 时返回可区分、可诊断的真实结果，且反馈结果不代签可玩性或世界行动成功。
- 控制面路径经 `oasis7_web_launcher` `/api/chain/feedback` 代理到 chain runtime（`/v1/chain/feedback/submit`），浏览器环境不伪造本地文件保存。
- 自动诊断摘要严格执行最小 allowlist 与凭据/私有路径脱敏保护；本文不新增反馈类型、附件、历史检索、远端遥测、权限模型或 runtime 协议。

## 范围

- 覆盖 `oasis7_web_launcher` 反馈代理边界、反馈结果语义与最小诊断保护原则。
- 不覆盖 feedback 网络的底层规则、反馈检索、附件、世界状态或视觉方案；这些变化需由对应专题和专业角色另行授权。

## 当前合同

| 入口 | 当前行为 | 成功/失败结果 | 边界 |
| --- | --- | --- | --- |
| native launcher | 主面板 `反馈 / Feedback` 打开独立窗口；字段为类别、标题、描述、输出目录 | 仅在链已 `Ready` 时尝试远端提交；成功呈现 `feedback_id/event_id`。该已就绪路径的远端提交失败时保存本地 JSON，并保留可诊断失败信息 | 链禁用或未就绪时，现有门控拒绝提交且不写本地包；草稿在窗口关闭前保持会话内状态；本地目录不可写或字段无效时显示失败，用户可修正后重试 |
| Web launcher | Web 反馈窗口校验草稿并请求 `/api/chain/feedback`；控制面再代理 runtime 提交 | 显示结构化成功、校验或代理失败结果；失败后可继续重试 | 浏览器不能落 native 本地反馈包；链不可用时提交受现有可用性门控约束 |

反馈内容只包含必要文本和已有最小诊断上下文；不得把 LLM 凭据或其他敏感配置写入反馈包或代理请求。

<a id="feedback-diagnostic-minimum"></a>
### 最小自动诊断与保护范围

- `launcher_config` 是显式 allowlist 的诊断摘要，只收录获准的布尔值、枚举值和有界数值；不是完整配置备份。凭据、provider URL 和私有路径不得进入该摘要。
- 自动日志只取最近 200 行，保护已配置的敏感值以及可识别的凭据/私有路径形式，同时保留安全诊断。空格分隔的 CLI 凭据、带引号且含空格的绝对私有路径必须纳入对应回归；不能因值不在当前配置里就忽略可识别凭据。
- 清洗执行保持有界。达到扫描上限时，未经保护的余下内容必须整体遮蔽或舍弃，不能原样写入报告。短配置值的替换不能逐字符损坏非敏感诊断或已有脱敏标记；对无法安全区分的片段保持保护，不用放出敏感值换取可读性。
- 自动清洗保证只覆盖自动诊断；用户手工标题/描述、任意未知秘密格式不在保证内。不扩展为通用秘密发现、遥测或新的反馈协议。

[#4406 F1、F2、F3a、F3b](https://github.com/eng-cc/oasis7/issues/4406) 的既有报告应逐项记录当前适用版本、独立回归结果或有依据的不适用结论。历史 unresolved 标志不证明今日仍有漏洞，历史通过也不代替各报告核对；不得把两项 F3 合并勾选。F4 的 allowlist、200 行上限、Ready 门控和各端传输行为独立保持。

<a id="feedback-local-file-identity"></a>
### 本地反馈文件不覆盖

native 本地保存成功后返回本次实际文件路径。固定时钟下连续写入同 kind、不同标题/描述的报告仍须得到不同路径，先前文件保持原内容且两份均可解析；预存同名文件和并发写入也不能截断既有反馈。仅提高时间戳精度不足以保证唯一性，writer 必须使用独占创建与有界冲突处理。

写入失败只能显示失败，不得声称已保存完整反馈。清理仅限本次明确拥有的未完成文件，不删除已有报告或历史资源；不新增崩溃持久化 SLA。该要求承接 F5，仅适用于现有 native 本地回落，并保留 Ready、远端优先和全部诊断保护。

## 接口 / 数据

- native 提交入口：`/v1/chain/feedback/submit`；成功结果可包含 `feedback_id` 与 `event_id`。
- Web 提交入口：`/api/chain/feedback`，由 launcher 控制面代理上述 runtime 接口。
- 本地回落包保留时间戳和类别的可识别性，以独占创建保证文件身份；承载 `kind`、`title`、`description`、`created_at`、最小 allowlist 的 `launcher_config` 与至多 200 行受保护的 `recent_logs`。

## 已知限制与不作出的承诺

- native/Web 共享反馈语义目标，不等于共享相同存储能力：本地回落仅适用于 native 的 Ready 后远端失败，不能把链禁用或未就绪表述为已保存。
- 现有链状态门控可以阻止提交；本文不声称已实现“禁用 CTA 的就地修复/下一步引导”。该体验改进若要推进，必须另立有视觉交互与控制边界复核的专题。
- 本文保留已迁移的入口和传输边界；上述诊断与文件身份条款补齐目标合同，不表示对应实现回归已经通过。

## 里程碑

- 已完成：native entry/window、Ready 后分布式优先提交与 Web 代理的已实现合同已收敛；三个 2026-03 源三件套已删除，Git 与 `.pm` task evidence 保留追溯。
- 本轮：将上述已实现合同收敛为 stable authority，并更新默认路由。
- 后续：任何禁用态修复 CTA、字段或协议扩展均须作为新的有界任务处理。

## 风险

- 链禁用或未就绪、代理失败或本地目录不可写会阻断相应提交路径；仅 native Ready 后远端失败可写本地包。结果必须保持可诊断，且不能误报为另一条路径已成功。
- native/Web 存储能力不同；将 Web 失败误写为本地保存会造成不可恢复的用户误解。
- 该文档整合只反映已核实行为；若实现变更未同步更新，会重新造成 authority 漂移。

## 验收与追溯

- 确定性合同：`npm --prefix crates/oasis7_viewer run test:feedback-contract` 只覆盖 Viewer feedback mapping，不替代 launcher native/Web 闭环。
- launcher 行为回归按受影响实现路径执行 `env -u RUSTC_WRAPPER cargo test -p oasis7_client_launcher -- --nocapture`、`env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_web_launcher -- --nocapture` 与 wasm check；可见变更另按 `testing-manual.md` S6 采集浏览器证据。
- 迁移追溯：三个 2026-03 源三件套已删除；使用 Git 与 `.pm` task evidence。Web parity 的现有专业专题仍按其当前路径维护。
