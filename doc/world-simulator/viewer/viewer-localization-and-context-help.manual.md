# Viewer 本地化与上下文帮助制作验证手册

本手册服务[本地化与帮助设计](viewer-localization-and-context-help.design.md)，提供来源核对、双语审读及后续采用验证。方法不表示执行结果。新帮助正文未进入实际候选时只做明确标为设计稿的审读；不把源码存在、测试定义、截图或作者自测写成运行或真人验收。

普通文档变更依[测试选择树](../../../testing-manual.md#testing-default-selection-tree)做适用静态检查，不因此启动游戏、浏览器、付费模型或生成翻译/字体文件。以下运行方法供实际消费 PR 在具备原授权和环境时选择；本手册不扩大启动或调用授权。

<a id="lh-static"></a>
## 1. LH-STATIC：来源、单元与链接

在仓库根目录记录候选 source、未提交范围和本次审读文件。核设计 §4.1 的实际交付物，G01～G16 与 H01～H06 应可分别定位；H04/H06 的浏览器与旧管理变体保持同一语义单元。两份文档的相对链接和显式锚点应通过仓库既有文档检查，实际消费路径不能用目录名或搜索结果代替。

下面只读取来源并输出摘要，可用于交接定位，不证明语义、集成或运行：

```bash
python3 - <<'PY'
from pathlib import Path
import hashlib

paths = (
    'doc/world-simulator/viewer/viewer-localization-and-context-help.design.md',
    'doc/world-simulator/viewer/viewer-localization-and-context-help.manual.md',
    'crates/oasis7_viewer/software_safe_src/viewer_locale_preferences_module.js',
    'crates/oasis7_viewer/software_safe_src/viewer_app.jsx',
    'crates/oasis7_viewer/software_safe_src/browser_byok_surface.jsx',
    'crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.jsx',
    'crates/oasis7_viewer/software_safe_src/model_connection_panel.jsx',
    'crates/oasis7_viewer/viewer.html',
    'crates/oasis7_client_launcher/src/main.rs',
    'site/assets/app.js',
)
for value in paths:
    path = Path(value)
    raw = path.read_bytes()
    raw.decode('utf-8')
    print(value, len(raw), hashlib.sha256(raw).hexdigest())
PY
```

路径不存在或无法读取时先修来源索引。哈希只定位所读字节，不证明它已经在 bundle 内。沿当前 source 核实际 `tr` / `localeText` 调用、动态文本来源及组件挂接，记录“现有文字”“本文目标”“实际采用”三者关系。品牌短标签核原矩阵，不在此另抄整表；声明整句复用时对照原句。

<a id="lh-review"></a>
## 2. LH-REVIEW：双语语义审读

输入为本次采用的中英文正文、真实触发事实、当前支持范围、字段与动作来源。制作方和双语 reviewer 对同一事实分别阅读两种版本，记录原意见和修订；未经独立审校的新译文仍标待审校。静态假设情境可以用来发现设计错误，但不得登记为实际世界或 UI 测试通过。

| 单元 | 独立核对方法 | 通过判据 |
| --- | --- | --- |
| G01～G03 | 对照可见不可控、未发送草稿、默认值与 override、已接受未应用的独立输入 | 两语言不把选择、草稿、接受、应用、永久保存互换；品牌标签仍引用原 authority。 |
| G04～G12 | 对照实际方案/结算来源、计划与接受批次、投入/副产物、仅归属相同、只有首笔产物及候选开放 | 不补数值或另一账本，不把产出归属当本次因果，不把生产、稳定、交付、候选和建成混同。 |
| G13～G16 | 比较原操作记录、执行方、模型供应商、已报告/未知用量及估计费用 | 中英文保留各自身份和范围；未知不归零、预算不说成供应商账户硬封顶。 |
| H01 | 分别给查看本人、查看他人可见对象、切换观察对象但旧操作未完的情境 | 点选只解释查看，不发明控制或行动；旧结果仍属原对象和原操作。 |
| H02 | 沿[原 Chat/Prompt 合同](viewer-control-plane-split-live-playback.design.md#prompt-command-entry-applicability)及 [PV14](viewer-manual.manual.md#prompt-command-entry-validation)，分别给预设未发送、Chat 仅收到/结果未知、合法 canonical Chat 已形成持续目标但 Prompt profile 未变、独立配置已应用的输入 | 两语言区分草稿、送达、目标与配置结果；Chat 的合法目标效果无需另做 Prompt Apply，替换目标仍须满足原意图规则，消息不自动改写 profile。未知不推断成功或未执行，不自动重发；目标已确认不证明移动或生产完成。 |
| H03a | 分别给非法输入、权限不足、资源/规则阻塞、当前模式不支持和原结果未知 | 每项 `{reason}` / `{nextStep}` 有来源和真实动作；不全改为重连/重试，不输出空插槽。 |
| H03b | 对已有预检的行动比较同状态合格/不合格、检查后资源或资格变化 | 预检不等于请求受理、预留或成功保证；未提供预检的行动不能因此被要求新增入口。 |
| H04 | 分别读浏览器本机保存路径和旧托管只管理路径的完整文字 | 凭证保存/接收方与实际路径一致；保存、检查、开始独立；费用和离页边界在中英文均完整。 |
| H05 | 比较缺回执、当前确认仍 pending、查询失败、权限不足、缺查询入口 | 不把未知改成未执行/失败；给当前确实支持的核对或安全下一步，连接刷新不冒充世界原件查询。 |
| H06 | 比较停止已确认、停止未知、删除成功、删除失败和旧凭证停用 | 完成措辞依实际确认；本机保存与旧会话凭证不混同；不宣称退款、上游 Key 撤销或世界取消。 |

英文审校需特别核 `accepted` / `applied` / `settled`、`pending` / `unknown`、`Stop` / `Forget`、`provider` / `executor` 的语境和否定词。中文审校核“已”“尚未”“无法确认”“仅”“仍需”的条件。自然流畅不能以省略费用、主体或未知边界换取；术语存在也不证明玩家能理解。

<a id="lh-locale"></a>
## 3. LH-LOCALE：实际语言选择

### 3.1 聚焦源码测试

修改 Viewer locale 消费或偏好逻辑时，在按仓库流程准备依赖的环境执行已有测试：

```bash
npm --prefix crates/oasis7_viewer run test:ui -- \
  software_safe_src/viewer_locale_preferences_module.test.js
```

[现有测试](../../../crates/oasis7_viewer/software_safe_src/viewer_locale_preferences_module.test.js)覆盖 Viewer aliases、旧 query fallback、旧存储迁移、存储/history 不可写与非 alias 路径隔离。它也包含 Prompt 展开偏好测试，该部分不能当作语言正文覆盖。只有命令真实成功才记通过；不证明字体、页面全部字符串或玩家理解。

### 3.2 已采用候选的操作核对

同候选分别核以下独立情境，避免后一个情境被前一个遗留偏好污染：

1. 有效 `locale=zh` 和 `locale=en`；同时存在冲突旧 `language` 时核 `locale` 优先。
2. 无效 `locale` 加有效旧 `language`；无 query 时读取有效保存值；两者都无效/无值时 Viewer 取 `en`。记实际初态，不能以作者浏览器已有偏好当默认。
3. 在三个受支持 alias 间保持偏好；其他 pathname 按原模块隔离。切换后核 `document.lang`、URL 与实际正文，页面语言属性正确不代签所有文字已经变化。
4. 在受控测试环境拒绝 localStorage 或 history 写入，核当前切换仍可使用；重新打开时只按实际可恢复值断言。不得在个人真实账号中清站点数据来制造样例。
5. 切语言前后核当前目标、原操作、可安全保留草稿、权限与结果不改变；缺中文/英文时按 LH-FALLBACK 记缺陷，不删除该语言测试。

Launcher 行为只在实际改动该入口时沿 [main_tests.rs](../../../crates/oasis7_client_launcher/src/main_tests.rs) 的 `parse_ui_language_supports_zh_and_en_aliases` 与 `ui_language_detect_from_values_prefers_current_launcher_value_and_falls_back_to_lang` 核语言别名和 env/LANG/default；其字体安装测试另核嵌入路径，不代签字形呈现。官网变化按 [site-discovery-check.py](../../../scripts/site-discovery-check.py) 和 [site-discovery-browser.test.cjs](../../../scripts/site-discovery-browser.test.cjs) 的配对页/浏览器语言场景；手册 `/doc/` 直接路径与无配对故事页不应被套入 Viewer 默认规则。没有改动或执行的相邻入口只记录源码边界，不声称回归通过。

<a id="lh-parameters"></a>
## 4. LH-PARAMETERS：插槽、动态原文和格式

逐条列出本次实际采用文字的插槽与同候选字段绑定，核中文与英文参数集合相同、来源可见性相同。长名字、引号、`&`、尖括号、CJK 与拉丁混排应作为文本展示；不能执行标记或脚本。不要在测试输入中使用真实 Key、私聊或他人数据。

对数量分别给合法零、正量、缺失、未发布及不合法值，核只有真实零显示零；显示的单位、精度、计划/接受/结算身份沿原来源。涉及批次单复数时核 1 与多个批次，涉及时间时核 tick 没被改写成无依据的现实时间。对同一操作切换语言，比较显示前后的实体/请求关联与实际提交参数；格式变化不能改写请求原文。

对玩家名字、聊天、Prompt 与 Agent 自由文本，核外壳翻译后原文未被悄悄改写；帮助只引用当前有权读取的信息。当前消费者未暴露足够字段时记缺口，不通过设计稿补值。不合法数值可在现有组件测试输入层验证，不能篡改正在运行的世界记录来造样例。

<a id="lh-candidate"></a>
## 5. LH-CANDIDATE：实际采用与运行准备

只在相关内容已经采用、任务授权且环境具备时运行后续章节。沿 [S6 浏览器闭环手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md)选择已有 execution lane、就绪检查、采样和 cleanup；缺构建/WASM/metadata、身份、权限或启动条件时记录 blocker，不用另一 lane 冒充本次环境。LetAI 专属 hold 继续沿 [Viewer 手册](viewer-manual.manual.md#本地真实-llm-游戏测试)，本手册不恢复其排期。

在原 PR/运行记录固定 source/tested tree、bundle、采用 H/G 修订、入口、world、主体、原请求可见范围、locale、viewport、文字缩放、输入方式、实际字体与 lane。沿 [S6 页面采样](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md#3-打开页面并采样)记录操作前、动作、等待/故障、恢复与最终页面；`__AW_TEST__` 只读取诊断，正式操作从可见 UI 完成。只关闭本任务拥有的会话。

世界事实使用当前 lane 的授权 snapshot、journal/receipt 或既有 API 读取；截图、译文和 UI fixture 不能成为世界 oracle。缺查询入口时记录实际能力限制，不造按钮，不把刷新连接当查世界。证据保留原位置、字节/摘要与必要关联，不复制凭据、真实 Key 或原始敏感错误体。

<a id="lh-outcome"></a>
## 6. LH-OUTCOME：原结果、身份与帮助恢复

| 场景 | 实际步骤 | 通过判据与证据边界 |
| --- | --- | --- |
| O1：相同事实的双语状态 | 对独立的当前确认等待、已确认结算、已确认拒绝、模式不支持及已发送但当前结果未知样例，分别记录 authority 与中英文显示；在受控允许的故障场景再观察断连/恢复。 | 状态不会因翻译、断连或切对象互换。旧 accepted/无 receipt 不能单独判继续等待或未生效；未得到真实故障样例则不记故障通过。 |
| O2：反馈归属 | 使用两名独立已鉴权玩家和匿名连接，分别读取各自有权反馈；交叉切语言、观察对象、刷新和换身份，记录当前主体与原反馈关联。 | 他人/匿名失败不成为本人的反馈，旧身份私有内容不残留。公共事件可存在但不冒充本人回执。缺独立主体时记未执行，不能拿两个同账号标签页代替。 |
| O3：H05 原件查询 | 在原结果未知时打开已采用 H05，使用现有合法原结果入口；分别核查询成功、查询失败/无读取资格和当前入口缺查询的范围。 | 成功只按权威结果解释；失败只说明查询局限。安全下一步实际可用，不出现为核结果而新发行动的建议。没有查询的入口明确限制，不能把帮助存在当功能完成。 |

权限泄露、归因错误或重复效果须回原 Runtime/Viewer owner 修复；删词、换颜色或弱化“我的”不能代替修复。内容语义错误由制作方和领域 owner 对照上游修订，只复测受影响场景。

<a id="lh-model"></a>
## 7. LH-MODEL：保存、检查、开始、停止与忘记

先核实际 UI 来自 [browser_byok_surface.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_byok_surface.jsx) 的哪条路径。浏览器模型面板与旧托管 `managementOnly` 面板分开审读、操作和留证；产品目标、源码支持与实际发行证据也分开。

| 场景 | 核对方法 | 通过判据 |
| --- | --- | --- |
| M1：保存与检查告知 | 对浏览器实际保存入口核完整 H04、保存前可见接收方/本机保存/同源脚本/付费说明及单独检查同意；对旧管理入口核只管理版本 | 两语言和实际路径一致；帮助不遮蔽原必读告知，旧会话不说成本机持久 Key。 |
| M2：阶段与许可 | 分别观察未保存、已保存未检查、检查通过未开始、缺有效任务/执行许可、明确开始后的实际状态 | 保存/检查不写成 Agent 已运行，模型就绪不说成有游戏权限；开始绑定实际任务，离页边界有明确解释。未实际运行的阶段不能通过文字审读代签。 |
| M3：支持范围 | 对照当前消费者的供应商、模型、协议与实际检查证据；另核未支持预设的提示 | 一个 DeepSeek 配置的结果不能覆盖 OpenRouter 或目录全部模型；不得通过译文扩大或静默删去原产品要求。 |
| M4：停止及历史后果 | 在原测试允许的输入/场景中分别核停止已确认、停止未确认，且原世界操作分别已受理/未知 | H06 保留停止与历史后果两个事实，不承诺退款、世界取消或供应商 Key 已撤销。 |
| M5：忘记与失败 | 按原存储验证方法分别核保留、删除成功、删除失败及旧会话凭证停用；对账号切换核是否出现旧敏感正文 | 已确认停止但删除失败与停止也未知使用不同句子；删除前不显示成功，旧凭证停用不声称浏览器 Key 已删除。 |

实际修改浏览器连接文字或绑定时可选择现有局部测试：

```bash
npm --prefix crates/oasis7_viewer run test:ui -- \
  software_safe_src/browser_model_connection_panel.test.jsx \
  software_safe_src/browser_model_store.test.js
```

[面板测试](../../../crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.test.jsx)已有独立保存/检查同意、原始输入清除、换账号迟到结果、固定错误和预算区分；[存储测试](../../../crates/oasis7_viewer/software_safe_src/browser_model_store.test.js)已有保存、锁、旧发送失效与损坏/写入失败范围。只有实际执行的测试才计结果，不能由测试文件存在声称新 H04/H06 已显示或全部故障通过。

真实供应商、CSP/CORS、工具往返、账号/多标签页、停止/删除及原件恢复仍按 [BYOK 验收手册](../../testing/manual/self-supplied-model-connection.manual.md)的适用场景与授权执行。局部 UI/存储测试不能代签真实调用；普通语言文档工作不触发付费检查。真实调用缺授权或配置时记未执行。

<a id="lh-fallback"></a>
## 8. LH-FALLBACK：缺译与字体失败

在已采用的受控候选或现有组件测试层，选择一条关键帮助/阻塞正文及一条次级说明，分别提供当前语言译文缺失而原文可用、关键字段未知的输入。核缺译提示、完整源文和安全下一步，未知字段不能被 fallback 补成确定结果。缺译注入只用于测试候选，不改线上资源、不修改世界状态。

字体测试记录实际 OS/浏览器、已加载字体、系统 fallback 和网络条件；分别检查正常、目标字体不可用与长 CJK/拉丁混排。Viewer 系统栈与 Launcher 嵌入字体分开，不能靠 family 声明证明安装或授权。真实缺字时保存字串与截图，核原信息和可用恢复仍可读；未能恢复的关键内容记失败，不能将该语言移出支持矩阵来通过。

当前缺译处理若尚未实现，可先审读设计稿并登记采用缺口；不要把人工在稿上加一条 fallback 提示计为 runtime 已具备降级。

<a id="lh-accessibility"></a>
## 9. LH-ACCESSIBILITY：帮助、长文本和输入

沿 [PW-ACCESSIBILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility)选择当前支持范围内代表性组合：1440×1000、768×1024、390×844、844×390、实际最小尺寸，以及 DPR 1/2、中文/英文、长名字/错误、200% 文字、reduced motion 和适用质量档。不是要求完整笛卡尔积；说明高风险组合、实际覆盖与未覆盖，不能只测英文桌面后概括全部语言。

逐个实际采用的 H 单元完成打开、全文阅读、关闭和继续操作。核触控无悬停也能读；Tab/Shift+Tab 可达，焦点可见，关闭后回到发起处；有滚动时完整否定/费用/未知说明不被截断。输入法组合期间按 Escape 不应误关或丢草稿；输入区快捷键不能误触世界或重复发送。背景刷新、切语言、减少动态效果不抢焦点或重复播报旧结果。

可先用既有聚焦测试核当前舞台帮助接点：

```bash
npm --prefix crates/oasis7_viewer run test:ui -- \
  software_safe_src/main.test.jsx \
  -t "moves presentation notes into the stage help tip"
```

[该测试](../../../crates/oasis7_viewer/software_safe_src/main.test.jsx)检查现有表现层说明的舞台位置、展开状态、正文与 ARIA 关联；它不覆盖新六类正文，不证明触控、键盘焦点、IME、实际字形或辅助技术组合通过。读到 Escape 处理也不能直接判完整 IME 兼容，须观察适用组合的实际行为。

<a id="lh-player"></a>
## 10. LH-PLAYER：目标玩家理解

实际内容采用后，复用 [PW-READABILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-readability) 的目标用户招募、候选准备、五秒首读、预设正确答案和原始回答记录。参与者未预读 fixture；进入可操作页面后计时，加载/登录另记。首读测主决策信息；阅读帮助后的任务另记，不把长篇帮助要求五秒读完，也不以帮助弥补首屏缺少关键阻塞。

为本次真实情境预先写清正确答案，再分别询问：

| 内容 | 提问与必须保留的正确含义 |
| --- | --- |
| H01/H02 | “刚才点选/填入/发送已经做了什么？”分清查看、草稿、对话、配置与世界效果，不能说看到就有控制权。 |
| H03 | “现在为什么不能提交，下一步是什么？”答案来自真实原因；检查通过不保证以后成功。 |
| H04 | “保存、检查和开始各发生什么？谁收到 Key、谁付费？”分清当前路径、费用、权限和离页边界。 |
| H05 | “这笔原操作现在知道什么、还不知道什么？怎样核对？”未知不回答失败或再发一次，连接刷新不当原件查询。 |
| H06 | “停止与忘记保留/删除什么？原世界操作怎样了？”区分本机/旧会话范围、确认/未确认、供应商撤销和世界后果。 |

沿原 PW-READABILITY 的初轮至少 5 位、各核心题至少 4/5 正确及信任负例不误归因标准；扩大样本仍按其比例。按实际被声称的语言记录样本和结果，不能把仅中文的理解结果外推英文，也不能汇总掩盖某一语言缺证。这不是全游戏统一研究门槛或统计显著性证明。

主持人不解释按钮、状态或代操作；独立完成、提示后完成、代操作、未完成和未覆盖分记。缺真人参与记未执行，模型 persona、作者自测和 UI 测试均不能替代。未集成正文可做明确标为静态稿的概念审读，但不计实际入口理解通过。

<a id="lh-handoff"></a>
## 11. LH-HANDOFF：结果与更新传播

在实际采用 PR 中沿一个真实改动追踪：上游变更 → 受影响 H/G 和参数 → 中英文修订 → 原组件采用 → 适用手册/镜像 → 选中的验证与原结果。没有发生的采用或同步明确记未采用/不受影响及理由，不预填“全部完成”。涉及公开 Viewer 操作的变更先核仓库 canonical 手册，再按原 site 流程更新对应中英文镜像。

每项结果包含候选、单元与版本、实际方法/命令、语言和环境、通过/失败/未执行、原证据入口及未证明范围。审校意见保留审校者与实际范围；静态摘要不是语言质量证书，语义审读不是运行结果，运行截图也不代签真人理解。

实际字符串、权限/状态、供应商/模型、字体、布局或构建输入变化后，按设计 §4.2 重新选择受影响场景；不要把旧候选的 PASS 迁到新组合。内容错误回制作方与领域 owner；locale、组件、焦点或降级问题回 Viewer；模型存储/执行问题回 Agent owner。修复后重测具体失败及必要关联范围，不扩成与本次变化无关的全游戏测试。
