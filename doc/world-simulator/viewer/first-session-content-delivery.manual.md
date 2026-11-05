# 首局工业体验内容制作与验证手册

本手册服务[首局内容设计](first-session-content-delivery.design.md)，提供静态素材检查、语义审读与后续实际入口/玩家验证方法。方法不是执行结果；每次运行在原 PR 和原证据入口记录候选、场景、结果及未覆盖范围。普通文档变更按[测试选择树](../../../testing-manual.md#testing-default-selection-tree)执行适用静态检查，不因此启动游戏、模型或新 CI。

<a id="fs-content-static"></a>
## 1. FS-CONTENT-STATIC：固定版本与素材结构

在仓库根目录执行。先用 `git rev-parse HEAD` 和 `git status --short` 记录真实 source 与未提交范围；版本应与本次审读/采用的候选一致。以下命令只读取现有文件，不构建、启动或修改资源：

```bash
python3 - <<'PY'
from pathlib import Path
import hashlib
import json
import xml.etree.ElementTree as ET

root = Path.cwd()
base = root / 'doc/world-simulator/viewer/assets/ui-ux/full-function'
catalog = json.loads((base / 'coverage-catalog.json').read_text())
print('catalog source_revision:', catalog['source_revision'])
features = {row['id']: row for row in catalog['features']}
for key in ('R-GAME-11', 'R-GAME-15', 'R-GAME-18'):
    row = features[key]
    source = root / row['source']['path']
    assert source.is_file(), source
    print(key, row['source']['path'], row['test_status'])

paths = (
    'gameplay/frames/industrial-settled.svg',
    'gameplay/frames/goal-execution-blocked.svg',
    'gameplay/frames/fallback-choices.svg',
    'gameplay/frames/fallback-no-safe.svg',
    'world-diagnostics/W01-world-targets.svg',
    'world-diagnostics/W09-narrow-world.svg',
)
for relative in paths:
    path = base / relative
    raw = path.read_bytes()
    tree = ET.fromstring(raw)
    assert tree.tag == '{http://www.w3.org/2000/svg}svg', path
    assert tree.get('viewBox'), path
    fonts = sorted({node.get('font-family') for node in tree.iter()
                    if node.get('font-family')})
    refs = sorted({value for node in tree.iter()
                   for key, value in node.attrib.items()
                   if key == 'href' or key.endswith('}href')})
    print(relative, len(raw), hashlib.sha256(raw).hexdigest())
    print('size:', tree.get('width'), tree.get('height'), tree.get('viewBox'))
    print('fonts:', fonts, 'references:', refs)
PY
```

**判定与交接：** 路径缺失、XML/JSON 无法解析或来源条目不存在时，先修原内容/引用并重新检查。通过仅表示文件存在、结构可读且摘要可定位。逐个核对字体/外链依赖：没有 href 不代表没有字体、CSS 或许可依赖；列出的字体族也不证明机器已安装或已获再分发授权。将实际尺寸和版本对照原稿，不强行要求桌面与窄屏相同尺寸。

在实际支持 CJK 的浏览器或 SVG 编辑器打开上述原件，按原稿声明的尺寸检查文字、换行、裁切、示例标注和已画状态。首产物稿必须读成设计示例；不能把其中 36/48/24 等数字、缺少 carbon_fuel/slag 的示例或字体显示效果升级为运行合同。记录使用的查看器、字体可用性与未检查项；截图输出继续放原 `output/playwright/` 验证目录，保留“设计预览”身份。

<a id="fs-content-review"></a>
## 2. FS-CONTENT-REVIEW：事实到表达的审读

输入为采用范围内的实际文字/图稿、同版本 canonical 配方/工厂、原动作/结算合同及已有字段来源。设计者与消费方逐项填写或讨论下表的答案，结果放原 PR；发现缺字段/动作交给原技术 owner，不从图稿补值。

| 内容单元 | 可独立核对的步骤 | 通过判据 |
| --- | --- | --- |
| C01/C02 | 分别读可行、无安全链、建造接受、实际建成四种输入；追到当前 profile 和 construction 结果 | 不默认 profile、不把采用目标/选中/接受当建成；已有合法工厂不要求重造。 |
| C03 | 从 [recipe catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)和 [factory catalog](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs)定位准确 ID；再按 [M4](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)核候选实际报价字段 | 同 factory/recipe/batches、消费账本、已发布投入/产出/副产物/能源语义一致；不以示例或其他账本补值，查询与正式动作区分。 |
| C04/C06 | 对照输入不合法、权限不足、材料/电力不足、原结果未知、查询不可用这些独立语义 | 已知原因未被统一改成重连；未知保持未知；恢复动作来自实际支持的能力，不给不存在的补料/改道按钮。 |
| C05/C07 | 比较 fresh authority 明确确认“已接受未开始”和“正在执行但未结算”两个状态，另核旧 accepted/无 receipt 无法确认当前结果、无进展、匹配生产结算、仅 owner 相同的独立输入 | 两个 fresh 状态分别表达接受与执行；旧 accepted/无 receipt 的未知输入转 C06，不能据此推定未生效。只有匹配成果使用有限成功语义；归属与“本次决定导致”分开，不由余额反推生产，不代签稳定/交付。 |
| C08/C09 | 核候选开放但建设条件未满足、重入旧成果、旧请求未知及观察 A 后切 B | 候选不是建成；旧成果不重复庆祝；原请求不改名归 B；离场不冒充取消，也不恢复失效授权。 |
| 语言与素材 | 对照 [原双语标签](viewer-brand-system-2026-06-05.design.md#25-bilingual-terminal-copy-matrix-target)、实际字符串与图形语义，分别记录目标文案与源码已有文案 | 不宣称新建议已经上线；尚缺译文或字体的语言组合保持缺口；颜色/动画/声音不独自传递结果。 |

审读无需凭空制造一套运行 fixture；尚未有实际输入的状态可以审查设计语义，但不能登记为运行通过。英文只核仓库既有文本及实际新译文，不能由中文设计建议推定英文本地化完成。

<a id="fs-content-focused-tests"></a>
## 3. 现有聚焦测试的选择

实际修改首产物卡/归一化时，在已按仓库流程准备 Viewer 依赖的环境使用现有命令：

```bash
npm --prefix crates/oasis7_viewer run test:ui -- \
  software_safe_src/industrial_starter_outcome.test.js \
  software_safe_src/industrial_starter_outcome_card.test.jsx
```

[归一化测试](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome.test.js)检查真实摘要字段、前置候选不冒充 Assembler、缺结算不从余额补值和畸形数字；[卡片测试](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.test.jsx)检查实际内容文本与稳定/交付边界。其 fixture 是局部输入，不是 canonical 配方完整样本或实际世界生产记录。只有命令实际执行且退出成功才记通过；这两个测试不证明权限、真实成本、网络恢复、像素呈现或玩家理解。仅编辑本手册/设计时不强制重跑 UI 测试。

<a id="fs-content-candidate"></a>
## 4. 实际候选准备与证据定位

运行时复用 [S6 浏览器闭环手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md)，按实际任务选择已经具备条件的 execution lane。需要本地普通 starter 路径时，可遵循其 [W3 DevLocal Builtin LLM](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md#w3：devlocal-builtin-llm-免邮箱启动)原步骤和就绪检查；不得把该 lane 改称 ProviderBacked 或 public_testnet。缺 WASM/metadata、有效授权、端口、构建或允许启动条件时记录 blocker。LetAI 专属开发 hold 仍按 [Viewer 原手册](viewer-manual.manual.md#本地真实-llm-游戏测试)执行，本文不恢复其排期。

1. 在原 PR/运行记录固定 source/tested tree、实际 bundle/资源身份、profile/revision、world/branch/window、主体与权限、场景/有效生成输入、语言、viewport 和 lane。外部内容另留可取得的固定版本；`session.meta`/`GAME_URL` 及资源加载记录保留原位置。
2. 按 [S6 页面采样](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md#3-打开页面并采样)建立独立浏览器会话，使用本次真实 `GAME_URL`，在桌面与窄屏保存状态、console/error 和截图；使用其 cleanup，只关闭自己的会话。正式输入从可见页面执行，`__AW_TEST__` 只读取状态/诊断。
3. 按 [QA receiver 边界](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md#qa-infrastructure-receiver-evidence-boundary)保留 before/action/pending-or-outage/fresh-recovery/final 和同候选 authority 对照。验证路径、字节数和 digest；记录可以公开的身份关联，不把凭据写入报告。
4. 世界 oracle 使用当前 lane 的已有权威 snapshot、journal/receipt 或授权 API 读取；页面截图和本地文案不能充当世界结算 oracle。预置世界可以用于受限场景，但说明其来历；不修改运行中的世界状态、nonce 或 receipt 来制造正例。

<a id="fs-content-runtime"></a>
## 5. FS-CONTENT-RUNTIME：实际入口与结果核对

在上述同一候选上执行。开始前为每个场景记下实际初态、期望语义和所需授权。以下是操作核对，不保证任何当前候选已支持全部路径；缺少可用动作、查询或独立主体时记录未覆盖，不能用 DOM 注入、作者解释或新按钮代签。

| 场景 | 实际步骤 | 预期结果与证据 |
| --- | --- | --- |
| R1：首目标与同源方案 | 从原 Next Move 进入首局目标，记录 feasibility/profile 与实际工厂。已有合法 Smelter 则直接核对；否则按实际建设入口提交并观察建设结果。打开固定生产方案，记录对象、配方、批次、消费账本及已发布成本/产出。 | 采用目标/查询不产生生产结算；建造接受与建成不同；无安全链保留真实原因和恢复。方案/提交参数相同，缺少输入明确未知。记录前后权威状态与页面。 |
| R2：接受到首产物 | 在合法输入、电力与授权满足时明确提交当前方案；用当前适用权威状态分别核“已接受未开始”“正在执行尚未结算”和实际生产结算，不能从旧 accepted 或无 receipt 推断阶段。沿原结果读取 receipt、所属 site 的 owner-bound ledger、正量铁锭与持久 milestone。 | 页面实际投入/产出与权威结果一致；接受与执行分别表达，无法确认当前结果则保持 C06，不庆祝完成也不判未生效。结算后只完成 production_only，Assembler 为候选，稳定与交付独立；若实际结果含卡片未投影的决策细目，保留缺口。 |
| R3：离场、重入与未知 | 有结算后离开并从同一授权入口回来，检查旧成果与下一步。另在受控本地场景使用原运行方法造成连接中断；只在确实观察到原请求未知时，恢复连接并使用已有原请求查询。 | 旧结果不重复产生/播报；断连不撤销成果；未知只按原 identity 核对，查询失败不变成原操作拒绝。保存原请求与前后 world 值；未取得未知样例时只记重入覆盖，不宣称丢回复/重启通过。 |
| R4：真实阻塞与恢复 | 在同来源的合法准备场景选择一个已知缺料或缺电的方案；读取预览/拒绝及实际可用恢复动作。仅通过当前支持的正式动作补足或暂缓，然后重查同一方案。 | 主 blocker、保留/已耗价值、复查条件可读；拒绝不编造成功/退款；恢复后的文案随事实更新。某种补足操作不存在时，记录可用的安全下一决策及功能缺口。 |
| R5：跨入口同义 | 对同一主体、world/profile/window，用原 pure API 授权读入口取状态，与 Viewer 的目标、blocker、完成边界、结果和下一步逐项对照。另需验证 canonical 生产 smoke 时，沿[首产物合同 §5](../../game/gameplay/gameplay-industrial-starter-completion-contract.prd.md#5-exactly-once、replay-与跨-surface-验收)及[原脚本](../../../scripts/oasis7-pure-api-parity-smoke.sh)选择其已有参数。 | 逐项含义一致而非仅字符串相似。smoke 会执行真实建厂/排产，须使用相应隔离测试世界；它自身的另一窗口结果不代替本次同世界双入口 join，也不证明灾备或 Agent parity。 |
| R6：语言与可访问性 | 对 R1/R2/R4 的实际页面按 [PW-ACCESSIBILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility)选桌面、窄屏、zh/en、长中文、文字放大和 reduced motion；用 Tab/Shift+Tab、返回/Escape 及原可用触控路径完成查看。 | 四个决策锚点、主要成本、动作和返回入口不被裁切；缺译文/字体如实降级，切语言不改世界结果；焦点不被更新夺走。记录实际覆盖组合，图形故障仅在实际注入并观察到时另按 PW-RECOVERY 计入。 |
| R7：本人结果与其他主体 | 在两名已鉴权玩家各自已有可信反馈后，分别由本人、另一玩家及匿名连接发起其有权尝试的拒绝请求；交叉刷新、重连/换身份并观察各自原反馈。 | 请求方只读本人可见拒绝；另一玩家反馈不被覆盖，匿名失败不归属已鉴权玩家；换身份不残留旧私有结果。公开世界事件仍可存在但不冒充个人回执。没有两个独立主体时记未执行。 |

R3/R7 若出现权限泄露、重复效果或错误归因，先保留 exact failure 与原证据，交 Runtime/Viewer owner 修复；不要靠换文案掩盖技术错误。素材结构问题由内容制作方修；内容含义不符由视觉交互设计与玩法 owner 对照上游修；实际加载、焦点与裁切由 Viewer 负责，之后只复测受影响场景。

<a id="fs-content-player"></a>
## 6. FS-CONTENT-PLAYER：目标玩家理解

复用 [PW-READABILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-readability)的招募、五秒首读、预先答案与原始回答记录方法；参与者符合当前目标用户且未预读 fixture。进入已可操作页面后计时，加载/登录耗时单记。主持人只给情境和目标，不说按钮名称、解释状态或代操作。

在本次固定场景上预先给出以下正确答案：

| 所观察状态 | 提问与正确语义 |
| --- | --- |
| C01/C03/C04 | “现在的目标、主要阻塞和下一步是什么？”答案必须对应当前对象与真实条件；不能把独立查询理解成已经配置提交。 |
| C05/C06 | “世界已经发生了什么，什么还没确认？”当前权威确认的已接受未开始与正在执行未结算须分清；只有旧 accepted/无 receipt 且无法确认当前结果时答未知，不能答确定失败、未生效或再生产一次。 |
| C07/C08 | “已经得到什么、属于谁，下一项现在能做什么？”分清实际首笔铁锭、归属、候选资格与尚待满足的建设条件；不回答已经稳定/交付/建成 Assembler。 |
| C09/切换观察焦点 | “回来后哪个结果是旧成果？这个结果来自谁的哪次行动？”原主体/原请求不随观察对象改变；无法精确归因时不猜玩家功劳。 |

原 PW-READABILITY 范围内沿用至少 5 位、各核心题至少 4/5 正确和信任负例不得误归因的初轮标准；5 人时至少 4 人正确，扩大样本时仍按该比例。它不是全游戏统一样本门槛或统计显著性证明。记录每人的原始答案与独立完成/提示后完成/代操作/未完成/未覆盖，行为观察、主观评价和研究者推断分开。提示后的理解不能回填为独立理解。

新目标文案未进入实际候选时，可另做标明“静态稿”的概念审读，但不能计为上述运行理解。缺真人参与则记未执行，作者自测、模型 persona 和 UI 单测均不能替代。继续意愿、经营分支和 Agent 主体感若需结论，仍按[首局分项验证](../../product/world-rules-core-gameplay/first-session-and-continuation.design.md#val-first-decision-sample)独立组织，不由本手册的识读结果推导。

需要把识读与首局目标、真实选择、Agent 委托和返回后的继续行为连成实际研究时，按[首局真人试玩手册](../../testing/manual/first-session-human-playtest.manual.md)执行筛选、两次会话、提示与原始记录；本节的局部可读性判据保持原范围，各分项分别报告。

## 7. 结果与复测范围

结果直接进入原 PR/运行证据，说明采用的内容单元与版本、命令或操作、环境、通过/失败/未执行、原 artifact 路径及未证明范围。静态检查只证明其结构和审读范围；运行与玩家结果必须来自实际执行。本手册不预填通过记录。

配方/profile、原结果归属、动作可用性改变时复查相应 C/R 场景；文字/译文/字体/布局变更复查语义与可访问性，涉及理解的关键表达再按 PW-READABILITY 复测。资产/构建输入改变按原 freshness 合同重建核验。引用旧结果时先核候选、来源与适用场景相同，不能以新文档版本改写旧失败或未执行。
