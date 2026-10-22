# 从热门 Skill 提炼的按需场景检查

这是现有调试、验证、评审与交互方法共享的参考，不是新的默认 Skill、协议权威或每 PR 检查表。只选本次变更触及的场景；通用规则仍归[开发流程规范](../../../../doc/engineering/workflow/source-of-truth.md)，方法效果比较复用 [evaluating-development-methods](../../evaluating-development-methods/SKILL.md)。下面是静态研究后编写的本地方法说明，不代表运行对照已证明收益或产品能力已实现。

来源为 2026-10-21 09:27 UTC 的 skills.sh All Time 前 1200 项快照：1191 项有站点正文，52 项另做重点来源深读；不是 1200 个仓库的依赖/安全审计。排名只用于定位当时条目。源码固定版本见末节，不随上游 main 自动更新。

## 1. 有什么增量，落在哪里

| 借鉴来源 | 本项目已有覆盖 | 此次具体增量与落点 | 不照搬 |
| --- | --- | --- | --- |
| #30 diagnosing-bugs | systematic-debugging 已有复现、可证伪假设、先 red 与根因后设计检查 | 下面的反馈环选型与原症状保留，写入原调试入口 | 新诊断入口、未运行命令就禁止任何初步假设 |
| #7 tdd | 已禁止镜像实现 | 独立 oracle 正反例与公共行为边界，供调试/验证使用 | 逐测试接缝人工审批、另建测试框架 |
| #53 code-review、#22 domain-modeling | 已有风险评审、具体路径/依据、产品与系统 authority | 合同/规范两轴；用边界案例核对术语，写入原评审入口 | 固定双 Agent、每子任务必审、平行 GLOSSARY/ADR |
| #601 api-and-interface-design | Game API 设计与 oasis7-play 已定义原请求恢复、fence 与权威回执 | 把现有合同展开成重复/冲突/崩溃/保留边界负例，不新定 API | 通用数据库唯一约束方案、One-Version Rule、统一 HTTP 重试策略 |
| #655 harden、#478 clarify、#569 accessibility | game-interaction-design 已有状态、恢复与可访问性原则 | 下面的 unknown/业务结果文案与键盘异常路径检查，写入原交互入口 | React/Next 实现、强制 polish、自动换请求的重试按钮 |
| #675 observability-and-instrumentation | 世界与请求已有身份/证据链，验证方法已有同 HEAD 原则 | 用入口来源加原请求追查丢失环节；明确观测和精确 CI 的证明边界 | 新遥测平台、无界指标标签、全量正文日志或全局仪表门禁 |

产品/系统要求若有缺陷，回写其原主责文档，不把本参考升格为协议来源：
- [外部 Runtime 产品要求](../../../../doc/product/agents-world-simulation/external-agent-runtime-play.prd.md)：区分目标要求与已验收组合。
- [Game API 系统设计](../../../../doc/world-simulator/llm/external-agent-game-api.design.md)：按具体切片确认原 RequestKey/payload、完整世界/owner/task fence、canonical receipt、历史保留及权限。
- [正式游戏 Skill](../../../../skills/oasis7-play/SKILL.md)：读取玩家当前可用路径与业务结果判定；本参考不修改玩家指令。
- [Viewer 结构标准](../../../../doc/world-simulator/viewer/viewer-frontend-structure-standard.prd.md)：保留 SolidJS 与 native/generated-source 边界，不搬 React 规则。
- [测试手册](../../../../testing-manual.md)：使用已有测试入口、层级与消费者范围，不另设 full 等于全部 required 的假设。

## 2. 从真实症状选一条反馈环

| 信号 | 优先尝试 | 可观察判据与局限 |
| --- | --- | --- |
| 特定输入/编码/恢复状态失败 | 最小固定 fixture | 原症状能稳定判红；保留输入、版本与失败，而非只保留修后截图 |
| 多阶段请求、掉线/恢复顺序相关 | 原事件或 trace 回放 | 首个状态分歧可定位到原请求；回放不证明真实网络/Runtime 已验收 |
| 多个合法输入组合触发不变量破坏 | 性质测试或有界 fuzz | 预先写明合同不变量，缩减到反例；随机跑完不证明穷尽 |
| 同一输入在宿主/实现间不一致 | 差分与固定独立样例 | 对齐版本、输入与 canonical 编码；两实现相同可能共同出错 |
| 明确历史好/坏版本之间的回归 | bisect 加同一判红用例 | 找到首个坏候选后仍核实根因；环境漂移或 flaky 信号不能直接归罪提交 |

比较时看同源码/同输入/同预算下到首个有效 red 的时间、无关改动、真实漏检与最终消费者结果。可以保留 H0，不把工具种类或发现数量当收益。

## 3. 独立 oracle 与原请求恢复负例

先固定本次适用的合同切片与观察边界。预期结果来自规范中的明确案例、独立手算值或可核对的原始权威材料；不要调用被测 decoder、projection 或关联函数生成 expected 再与自身比较。

例：一次制造动作返回 cognition committed，但业务回执为 rejected，正确结论是候选已提交、业务未成功；“HTTP 200 / committed 即产出成功”的断言应判红。把样例中的 world、原 request 或适用 generation/fence 换成不匹配值，应被相关验证拒绝，而不是只看状态字符串。没有真实运行时，这只是待执行的合同用例，不算回归通过。

| 受影响场景 | 测试输入/故障点 | 按适用合同核实的结果 |
| --- | --- | --- |
| 同意图重复与并发 | 原完整身份、原 payload 重投；并发到达 | 复用同一原结果或明确未决，不产生第二次权威业务效果；不能只数响应次数 |
| 同操作标识异内容 | 在合同定义的同身份范围改变 payload | 关联冲突/拒绝，不能误回另一意图的成功；不同合法意图不能被误合并 |
| 效果后响应前断连/崩溃 | 原请求已发出但调用方无最终响应，随后恢复 | 先查原请求，仅在原合同确认允许重投时保留原签名/nonce/payload/correlation；否则仅回查并报告阻塞；未决不换 ID、世界或 Context |
| 原请求 pending/unknown | 尚无可确认终态；服务暂不可读 | 保持未确认，不等同失败，也不制造竞争写；按当前读取授权恢复 |
| Context 到期、Stop 或撤权 | 原结果回读与新副作用分别尝试 | 原结果读取仍受当前读授权；恢复证据不延长旧 grant，不允许新写；按合同停止/恢复 |
| 历史保留/近期窗口边界 | 结果退出 recent window，或原历史材料不可得 | 窗口缺项不证明失败；按合同查原结果/当前订单资源，HistoryUnavailable 与 Unknown 不混用，不靠新请求替代丢失历史 |

幂等作用域、错误码、保留时长及“何时允许新意图”由原合同决定。例如明确报价已拒绝后重新报价，与 unknown 原请求恢复不是同一种重试。不能从上游支付示例推导本项目统一 TTL、数据库方案或所有操作的 exactly-once 保证。

## 4. UI 状态、文案与可访问性的最小旅程

状态名按当前接口映射，下列是解释方向，不是新增协议枚举或承诺。

| 已知事实 | 文案/交互需要让玩家知道 | 不可作的推断 |
| --- | --- | --- |
| 仅 transport acknowledgement / volatile Received | 已收到传输响应，但持久结果尚未确认 | 不直接标 Game pending、任务已创建或业务成功 |
| 合同确认的 pending/accepted | 哪一层已受理，下一步等待/查询什么 | 不把受理当 committed，更不当产物已交付 |
| unknown / 断连后结果不明 | 结果未确认，可查询原请求；当前可做与不可做的事 | 不展示“失败，重新提交”并换新身份 |
| 明确 rejected/failed | 原拒绝原因、受影响范围与获准下一步 | 不编造服务器原因，不自动扩大权限 |
| recovered-readonly | 可读取什么，尚不能执行什么 | 不把恢复可读写成可服务/可继续行动 |
| 无绑定/空态/过滤无结果 | 区分首次、过滤、权限和读取失败 | 不将读取失败伪装空世界或清空玩家成果 |

只覆盖变更涉及的异常路径：重复点击、断线、刷新恢复、错误后再操作、关闭/取消后的焦点回位；再用键盘检查可达性、可见焦点和错误关联，用实际辅助技术检查状态播报才可声明其支持。按受影响视口验证长文本、窄屏、200% 缩放与触控目标；模拟 viewport 不等于实机触摸验证。保留实际状态/版本/视口证据，截图不代替输入或业务结果，Web 不代签 native。

## 5. 观测关联与精确验证结论

先问“请求从哪个入口发出、停在哪个边界、是否有原权威结果”。按现有 schema 在允许的诊断证据中关联 entry point 与原 request/correlation、world 和适用 fence；入口来源应由入口记录并跨边界保留，不根据时间接近猜测。缺字段时标未知，不能补造关联。此处不要求每行日志新增全部字段。

只读已有证据可先做；故障注入限定获准隔离环境。选一次中断检查诊断链能否找到丢失环节，再回到权威回执验证，不把 trace success 当世界成功。高基数请求/用户身份不放指标标签；日志/trace 也只保留授权必要字段，避免私有目标、完整 payload、密钥和个人信息，公开前脱敏。

CI 对当前 source HEAD 分别核对实际选中的必要任务、结论与未覆盖部分。若运行的是合并树，说明真实 base/event/tested tree；full、局部测试和某个绿色 workflow 的名称不证明全部 required 已完成。复用 PR/CI 原证据，不新增统一证书，不改检查选择或保护设置，不仅因 main 前进重跑。

## 6. 固定来源、裁剪与采用边界

以下只为方法溯源；本文件是针对 oasis7 的独立归纳，没有复制上游代码/长段正文，也不执行其安装命令。

- mattpocock/skills，MIT，固定 `5beb35b040e3de97e54fa9e386d7e42d15c295c2`：[diagnosing-bugs](https://github.com/mattpocock/skills/blob/5beb35b040e3de97e54fa9e386d7e42d15c295c2/skills/engineering/diagnosing-bugs/SKILL.md)、[tdd](https://github.com/mattpocock/skills/blob/5beb35b040e3de97e54fa9e386d7e42d15c295c2/skills/engineering/tdd/SKILL.md)、[code-review](https://github.com/mattpocock/skills/blob/5beb35b040e3de97e54fa9e386d7e42d15c295c2/skills/engineering/code-review/SKILL.md)、[domain-modeling](https://github.com/mattpocock/skills/blob/5beb35b040e3de97e54fa9e386d7e42d15c295c2/skills/engineering/domain-modeling/SKILL.md)。
- addyosmani/agent-skills，MIT，固定 `aeac41d4d005999432bb7312f7ff06383a91d59d`：[api-and-interface-design](https://github.com/addyosmani/agent-skills/blob/aeac41d4d005999432bb7312f7ff06383a91d59d/skills/api-and-interface-design/SKILL.md)、[observability-and-instrumentation](https://github.com/addyosmani/agent-skills/blob/aeac41d4d005999432bb7312f7ff06383a91d59d/skills/observability-and-instrumentation/SKILL.md)。单 Skill 安装可能缺 repo 级 references；依赖闭包与宿主兼容性未完全审计，数据库/TypeScript 示例不是本项目实现建议。
- pbakaus/impeccable，Apache-2.0，固定 `dad8dbb3aac21ad1fa236c110315ef4abece23e8`：[harden](https://github.com/pbakaus/impeccable/blob/dad8dbb3aac21ad1fa236c110315ef4abece23e8/.agents/skills/impeccable/reference/harden.md)、[clarify](https://github.com/pbakaus/impeccable/blob/dad8dbb3aac21ad1fa236c110315ef4abece23e8/.agents/skills/impeccable/reference/clarify.md)。榜单独立入口已在此版本整合为 reference；不附带 teach/polish 路由。
- addyosmani/web-quality-skills，仓库元数据标为 MIT，固定 `afa8da942115f2961fdbfa80807ea0b232ff6c00`：[accessibility](https://github.com/addyosmani/web-quality-skills/blob/afa8da942115f2961fdbfa80807ea0b232ff6c00/skills/accessibility/SKILL.md)。该项子目录许可例外与完整依赖闭包未完全核实，限主题参考，不直接复制或安装；可访问性启发不是合规认证。

已有规范中的原请求、fence、业务回执和 CI 边界不是上游新增能力。本次只是补可用检查案例，不改变产品合同、玩家 Skill、默认加载数量、工具权限、Issue 管理或日程。不安装第三方包，不自动升级 Vite/Vitest，不启用 Codex/GPT Work，不把静态审读叫做真实方法试验。以后要执行对照，沿用既有 H0/候选、同预算、pilot/holdout 与失败保留；任何安装、外部调用和写操作另按已有授权处理。
