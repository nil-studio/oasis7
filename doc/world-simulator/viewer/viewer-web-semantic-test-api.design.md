# Viewer Web 语义测试 API 设计

- 对应需求文档: `doc/world-simulator/viewer/viewer-web-semantic-test-api.prd.md`
- 历史 API 实施与验证记录：GitHub task issue evidence。

## 1. 设计定位
定义 Web Viewer 面向自动化的语义测试 API、观测字段与步进控制边界。

当前 implementation truth 是 `crates/oasis7_viewer/software_safe_src/legacy_core.js`；本设计不把历史 wasm/Bevy bridge 当作可继续扩展的 source layer。

## 2. 设计结构
- 语义层：暴露稳定的 UI/世界状态语义，而非脆弱 DOM 细节。
- 控制层：提供步进、查询与断言友好的测试入口。
- 观测层：输出测试所需的状态摘要与错误信号。
- profile 层：以握手后的 `controlProfile` 限制可提交动作；live 不暴露 seek/seek_event。
- 兼容层：`logicalTime` 与 `eventSeq` 是当前观测字段，`tick` 只兼容等同于 `logicalTime`；空 mailbox、reconnect 和 queued 控制不生成观测增量。

## 3. 关键接口 / 入口
- 语义测试 API 路由与返回结构
- 步进/查询/状态快照入口
- Playwright 或测试脚本消费的稳定字段

## 4. 约束与边界
- API 设计要避免与生产控制面强耦合。
- 语义字段应稳定、可版本化。
- 自动化入口不能影响正常 viewer 交互路径。
- `step` parser 必须拒绝无效 payload 而不 throw；`runSteps` 只复用同一 step-count grammar，不重新引入历史 UI DSL。
- WebSocket error/close、unsupported action、gameplay gate 与 runtime no-progress 必须保持可区分的自动化状态。

## 5. 设计演进计划
- 先完成 Design 补齐与互链回写。
- 后续实现与验证以新建 GitHub-backed task、当前代码和本合同为准。


## 6. 目标、上游与实现边界

Owner：viewer_engineer / qa_engineer。消费者为隔离 visual-test 构建中的自动化驱动；本专题维护测试观测与操作辅助，不授权生产控制，不以 helper 成功代签真实 UI、IME 或 gameplay 验收。

当前 `legacy_core.js::isTestApiEnabled/installTestApi` 同时要求 `__OASIS7_VISUAL_TEST__ === true` 与显式测试 query；release 加 `test_api=1` 仍不安装 API。配对 PRD 的历史 debug/release 注入和 Bevy command queue 记录不能覆盖该构建隔离边界；操作入口以[当前测试手册](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)及实际构建 profile 为准。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [当前 API 合同](viewer-web-semantic-test-api.prd.md#viewer-web-semantic-test-api-current-contract) | `sendControl`/`runSteps` 维持当前 step grammar、profile 与反馈区分；产品适用性：N/A；reason=测试驱动的观测接口不新增玩家控制权限或产品功能；scope=隔离测试构建中的语义API | [语义调用与观测](viewer-web-semantic-test-api.design.md#semantic-observation) | runtime 协议拥有进展事实；viewer_engineer 拥有解析与反馈 | 不恢复 retired UI DSL；不把 queued 当完成 |
| [Chat 输入验证边界](viewer-web-semantic-test-api.prd.md#chat-输入验证边界) | helper 与真实输入分层验证；产品适用性：N/A；reason=本关系仅定义QA辅助方法而非对话产品承诺；scope=测试入口证据不能替代真实UI输入 | [真实输入与错误边界](viewer-web-semantic-test-api.design.md#semantic-input-boundary) | qa_engineer、真实浏览器环境与聊天控制 authority | 不据 helper 宣称 IME、多行、焦点隔离均通过 |

<a id="semantic-observation"></a>
## 7. 调用流程、状态与兼容

驱动先核对测试构建与 API 是否存在，读取握手后的 `controlProfile`，再提交合法动作。无效 step payload 不发控制请求；`runSteps` 只复用 step-count 解析。读取 `lastControlFeedback` 区分 rejected/blocked/queued/发送失败，随后依据 runtime 消息观察 completed/no-progress。`logicalTime` 与 `eventSeq` 不由空 mailbox 或重连合成；`tick` 仅为前者兼容 alias。

上述均为会话内观测状态，不是可持久化事务回执。页面刷新或断链后须重新读取状态，不能自动重放旧控制请求。失败/不支持动作不降级为历史 seek 或 DSL；接口变更须复核驱动消费者，回滚时也保持 release 不安装 API 的安全边界。

<a id="semantic-input-boundary"></a>
## 8. 部署、安全与质量边界

语义 helper 跳过真实键盘组合态、焦点和 DOM 事件顺序；真实聊天输入须分别覆盖 composition、Enter/Shift+Enter、重复事件、权限/空消息/in-flight、超时与显式重试。helper 测试不是这些 UI 交互的替代。

浏览器 runtime fatal 必须可观察，不能记作 connecting 或世界进展；已知 graphics fatal 的有界 reload 也不能抹去错误。自动化保留 state、console、截图和实际请求次数/文本，不记录密钥。测试构建不得作为公开 release 发布；负载与真实玩法持续性由各自专项测试负责，本 API 无吞吐承诺。

## 11. 验证设计

### 11.1 验证映射表

| 上游 requirement | 本设计条款 | 独立 obligation 与适用条件 | 验证方法 / source / scenario / candidate | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [当前 API 合同](viewer-web-semantic-test-api.prd.md#viewer-web-semantic-test-api-current-contract) | [语义调用与观测](viewer-web-semantic-test-api.design.md#semantic-observation) | 隔离测试构建中非法step不发包，live拒绝seek，queued/reconnect不合成logicalTime/eventSeq进展 | [测试手册](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；按实际 profile 核对非法payload不发包、live不支持seek、queued不合成进展；L4A profile/反馈与无进展负例；固定 source、visual-test 构建、query、runtime profile 和浏览器 | 实际PR/验证记录：source/build/profile、请求次数与payload、getState和lastControlFeedback序列、console及失败负例输出 | 手册实时回归不能靠step伪造自然进展；未声称现有自动套件覆盖任意parser输入、真实玩法或生产控制权限 |
| [Chat 输入验证边界](viewer-web-semantic-test-api.prd.md#chat-输入验证边界) | [真实输入与错误边界](viewer-web-semantic-test-api.design.md#semantic-input-boundary) | 语义helper与真实键盘输入分别取证；组合态/多行/焦点/超时恢复不得由helper成功代签 | [测试手册](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；真实键盘输入与可见恢复验证另取证；单元语义与真实UI分层；记录 source、构建、OS/浏览器、输入法及故障注入条件 | 实际PR/验证记录：语义测试输出；真实IME场景的state、console、截图、请求次数/消息文本；fatal注入与有界恢复记录 | helper仅证明所测语义路径；未执行的OS/输入法/浏览器、真实IME、多行及fatal场景不能视为通过 |

## 12. 决策、风险与未决范围

保留稳定语义观测，淘汰历史 UI DSL；不能为自动化方便扩大 live 控制或给 release 注入 API。

当前源码的未锚定正则与未捕获 count 转换异常是[已知实现差距](viewer-web-semantic-test-api.prd.md#当前-step-parser-的实现差距)；完整 grammar、非法输入不发包与 no-throw 仍是目标，须修复并补独立负例，不能以当前实现放宽合同。

文档补录不宣称当前 API、IME 或 fatal 回归已运行；历史验收边界维持不变。
