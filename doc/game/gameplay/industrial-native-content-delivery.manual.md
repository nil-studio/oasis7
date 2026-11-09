# Native 工业内容制作与验证手册

本手册配套[内容目录与制作设计](industrial-native-content-delivery.design.md)。普通文档变更执行静态来源核对和适用的文档检查；只有实际改动配方、初态或消费者时才选择对应技术/游戏验证。方法存在不代表已执行，运行结果沿原 PR 与 QA 入口记录，不另建生产台账或 CI gate。

<a id="ic-catalog"></a>
## 1. IC-CATALOG：目录、定义源与身份

在仓库根目录先用 `git rev-parse HEAD`、`git status --short` 固定 source revision 和未提交范围。以下只读脚本核对本文设计的目录 ID 与 canonical match arms，读取对应模块的 ID/marker/材料种类，报告真实数量。它使用当前源格式；格式变化导致拒绝时先核原源，不静默跳过内容。

```bash
python3 - <<'PY'
from pathlib import Path
import hashlib
import re

root = Path.cwd()
api = root / 'crates/oasis7_agent_api/src/world_quote'
modules = root / 'crates/oasis7_builtin_wasm_modules'
doc = (root / 'doc/game/gameplay/industrial-native-content-delivery.design.md').read_text()
catalog = doc.split('<a id="native-content-catalog"></a>', 1)[1].split(
    '<a id="content-dependency-and-consumption"></a>', 1)[0]
recipes = re.findall(r'^\s*"(recipe\.[^"]+)" => \(',
                     (api / 'recipe_catalog.rs').read_text(), re.M)
factories = re.findall(r'^\s*"(factory\.[^"]+)" => FactoryModuleSpec',
                       (api / 'factory_catalog.rs').read_text(), re.M)
for kind, ids, expected in [('recipe', recipes, 11), ('factory', factories, 2)]:
    assert len(ids) == len(set(ids)) == expected, (kind, ids)
    rows = re.findall(r'^\| `(' + kind + r'\.[^`]+)` \|', catalog, re.M)
    assert len(rows) == len(set(rows)), (kind, 'duplicate table row')
    assert set(rows) == set(ids), (kind, 'catalog/source mismatch')
    print(kind, 'native content count:', len(ids))

def constant(source, name):
    match = re.search(r'^const ' + name + r':[^=]+=(.*?);', source, re.M | re.S)
    assert match, name
    return match.group(1).strip()

for recipe in recipes:
    path = modules / ('m4_' + recipe.replace('.', '_')) / 'src/lib.rs'
    source = path.read_text()
    assert constant(source, 'RECIPE_ID') == '"' + recipe + '"', path
    assert constant(source, 'MODULE_ID') == '"m4.' + recipe + '"', path
    marker = constant(source, 'REQUIRED_FACTORY_MARKER').strip('"')
    assert marker == recipe.split('.')[1], (path, marker)
    parts = {key: re.findall(r'\("([^"]+)",\s*-?\d+\)', constant(source, key))
             for key in ('CONSUME_PER_BATCH', 'PRODUCE_PER_BATCH', 'BYPRODUCTS_PER_BATCH')}
    print(recipe, marker, parts, 'file_sha256:', hashlib.sha256(path.read_bytes()).hexdigest())

for factory in factories:
    path = modules / ('m4_' + factory.replace('.', '_')) / 'src/lib.rs'
    source = path.read_text()
    assert constant(source, 'FACTORY_ID') == '"' + factory + '"', path
    assert constant(source, 'MODULE_ID') == '"m4.' + factory + '"', path
    print(factory, path.relative_to(root))
PY
```

**判定：** 两个 canonical 集合与设计目录逐项一致，各模块存在且 identity/marker 对应；将脚本输出的输入/主产物/副产物种类逐行与设计表及 native `MaterialStack` 对照。脚本不自动判定中文关系说明或 native/WASM 数值一致，后者使用 §3 的现有测试。源文件 SHA-256 只定位本文件，不是 WASM source closure、build identity 或安装证明。

另读 [factory_catalog.rs](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs) 的传入 `factory_id` 赋值及 [factory quote 测试](../../../crates/oasis7_runtime/src/tests/agent_delegation/factory.rs) 的 `registered.factory` 实例：确认 kind 与实例分离。核对 [Miner 定义](../../../crates/oasis7_builtin_wasm_modules/m4_factory_miner_mk1/src/lib.rs)位于本 native 范围之外，不能把 2 个 kind 写成全部工厂。新内容获准加入原目录时同步更新实际数量和方法，不能为保持 2/11 而过滤新增项。

<a id="ic-initial-state"></a>
## 2. IC-INITIAL-STATE：配置与有效初态

按下表读取实际入口，不因某个 JSON 没写材料就宣称最终 Runtime 缺材料。静态核对只报告源关系；实际可行性声明需要同一运行候选的有效状态。

| 步骤 | 静态输入与具体检查 | 实际候选追加证据 |
| --- | --- | --- |
| 识别入口 | 读[场景消费矩阵](../../world-simulator/scenario/scenario-files.prd.md#场景初始化与消费入口矩阵)，确认 named scenario、generated sidecar、固定 starter 或既有 snapshot | 原运行入口、source/tested tree、world/profile/revision、实际配置与 snapshot 来源。 |
| 还原生成输入 | 读所选 JSON、有效 defaults 或 [formal_release_default_seed_model](../../../crates/oasis7/src/runtime_host/gameplay_actions.rs)；记录 origin、Agent、seed、fragment/bootstrap chunk 的来源 | 比较实际初态模型与来源；generated provenance 沿原合同分别报告存在、内容绑定及可复现证据。 |
| 还原 Runtime 准备 | 读 [bootstrap.rs](../../../crates/oasis7/src/runtime_host/bootstrap.rs) 实际调用链，区分 world 库存、owner 建造账本与两个 site 账本；核 site owner、location 与 construction power/profile | 授权读取该候选的实际 ledger/authority。源中有预置不能证明现存世界仍有这些材料。 |
| 核对首局资格 | 读 [starter_industrial.rs](../../../crates/oasis7_world_state/src/state/starter_industrial.rs) 的 profile、固定实例、feasibility 与持久 milestone 条件 | 同一 fresh snapshot 的 `candidate_available` 或 `no_safe_starter_chain`、blocker/next_action/next_recheck；生产完成再核匹配 receipt 和 owner-bound 正量结果。 |

通过判据是能够逐项回答“这个内容来源于哪个输入、由哪个消费者写入哪一类账本、当前是否仍适用”。源码确认不能填为 live PASS；缺来源或权限则记录未核实。首笔铁锭结果、builder 预置铁锭和 Assembler site 预置铁锭保持分别归因，不能把后两者算为首局生产贡献。

<a id="ic-integration"></a>
## 3. IC-INTEGRATION：按变化选择原有验证

以下是已有测试入口的选择方法，非本次运行记录。按[测试选择树](../../../testing-manual.md#testing-default-selection-tree)取最小适用范围，执行时检查实际命中非零测试及退出码，保留候选与输出；零测试不计通过。`test_tier_full` 在这里启用对应测试定义，并不要求无过滤执行全套。

| 真实变更 | 现有精确入口 | 可证明与不能证明 |
| --- | --- | --- |
| native recipe 或对应 WASM 定义 | `env -u RUSTC_WRAPPER cargo test -p oasis7_agent_api builtin_recipe_plans_match_all_eleven_wasm_definitions`；`env -u RUSTC_WRAPPER cargo test -p oasis7_agent_api builtin_recipe_catalog_rejects_aliases_and_noncanonical_ids` | [原测试](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)用 11 份独立定义，按 1、7、`u32::MAX` 批次核完整 plan，并拒绝零批次和非 canonical ID。证明源定义一致，不证明编译模块、模块安装或世界可执行。 |
| native recipe 的选择/报价消费者 | `env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --features test_tier_full builtin_recipe_quote_` | [原测试](../../../crates/oasis7_runtime/src/tests/agent_delegation.rs)分别覆盖只读 quote 与实际 native 生产一致，以及授权不足、缺料和非 canonical/零批次拒绝；仍是局部 fixture，不证明全配方组合或实际世界可行。 |
| factory kind/spec、原选择/报价消费者 | `env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --features test_tier_full builtin_factory_quote_` | [原测试](../../../crates/oasis7_runtime/src/tests/agent_delegation/factory.rs)覆盖非 kind 同名实例、quote 不改世界、实际建造、篡改规格与授权/准入拒绝；仍是局部 fixture。 |
| 固定 starter bootstrap 或资源准备 | `env -u RUSTC_WRAPPER cargo test -p oasis7_viewer_runtime --lib --features test_tier_required bootstrap_formal_release_runtime_world_uses_seeded_fragment_bootstrap_agent`；相同参数将 filter 换为 `bootstrap_formal_release_owner_can_fund_first_twelve_batch_smelter_run` | [原测试](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/support.rs)核 seeded Agent/site 输入及首个生产路径的 fixture；不证明每个 named scenario、正式激活或玩家入口。 |
| WASM 定义、模板、构建依赖或工件实际交付 | 沿[WASM 构建管线](../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md#build-source-input-rule)和[原测试手册](../../../testing-manual.md)选中受影响模块，保存原 receipt/hash/identity | 新源码对应新候选的构建/消费证据；内容目录和源文件摘要不能代替此项，本文不授权发布写入。 |
| 实际首局内容集成/入口行为 | 沿[首局内容手册的候选准备](../../world-simulator/viewer/first-session-content-delivery.manual.md#fs-content-candidate)及[首产物合同](gameplay-industrial-starter-completion-contract.prd.md#5-exactly-once、replay-与跨-surface-验收)选择原运行方法 | 使用隔离且已获准的测试世界，核原操作、实际账本、匹配结算、重连与同候选入口；不从另一窗口 smoke 推定当前 Viewer 组合通过。 |

仅更新目录/方法时运行 `./scripts/doc-governance-check.sh`，不启动 Rust 全套、浏览器或游戏。首局源码已有方法不意味着 factory_core 的全依赖链已有可玩场景；扩大该链的交付前需补真实输入来源、账本适用性及所需运输/执行能力。

<a id="ic-blockers"></a>
## 4. IC-BLOCKERS：保留真实失败边界

内容作者与消费者在同源设计/输入上分别检查以下反例；已有适用运行记录可直接引用，没有实际输入时只记录语义审读。

| 输入或情形 | 应得到的内容/交接判断 |
| --- | --- |
| `recipe.iron_ingot`、大小写/空格变体、零批次 | native 选择拒绝；duration helper 能识别旧名字不构成绕过依据。 |
| kind 为 Smelter，实例为 `registered.factory` | 目录仍只有一个 Smelter kind；使用该实例自己的 site/profile/授权，不把它当 canonical starter 实例。 |
| world 总量足够但实际 site ledger 缺 `carbon_fuel` | 真实缺料仍阻塞；不隐式转移或以另一资源抵销。 |
| site 有材料但 owner/site/profile、construction power 或合法授权缺失 | 保留具体缺口和原恢复/复查路径；目录条目不能授予资格。 |
| 存在旧 accepted/报价，但原请求最终结果不明 | 保留原请求未知并查询当前合法结果，不写成未生效或重复提交。 |
| 匹配 production receipt 已有，delivery/建成 Assembler/稳定窗口未证 | 仅陈述已证首产物及实际候选，不补成后继完成。 |

发现源/consumer 冲突先交原专业 owner 定位；目录错就修目录，运行规则缺陷按原设计/实现流程修复。不能靠删掉反例、缩小失败用语或换一个更容易的初态，把原目标记为通过。

<a id="ic-change"></a>
## 5. IC-CHANGE：一次具体变更的交接

以设计中的 iron_ingot 传播样例或本次真实受影响 ID 为输入，逐项完成以下工作。纸面演练和实际制作分开记录；只有实际发生的变化才登记为传播证据。

1. 在原 PR 固定旧/新源，说明变更是解释、参数、因果关系、新内容还是初态；链接实际上游决定。若只有目录修正，明确运行源未变化。
2. 从 recipe/factory 原定义检查直接引用，再追到实际 quote、执行/结算、starter 和 bootstrap 消费者。记录受影响和经核对不受影响的依据，不要求机械修改所有提及它的文件。
3. 若需模块工件，复用原 pipeline 的输入闭包、模块选择和版本记录；核模板/依赖变化使哪些身份及证据需重取。旧 source/WASM/初态的组合不能冒充新候选。
4. 核中文表达、主/副产物、来源/许可与需要保留的署名；本文目录不是对全部依赖的许可审查。实际导入外部内容再附真实来源、版本和授权范围。
5. 按 §3 选择验证，核对一次适用的变化如何传播至产物、实际加载/运行和受影响证据。保存准确失败/通过范围；局部测试失败交回原 owner，之后只复测实际受影响面。

交接完成应能让接收方从当前 PR 取得准确源版本、内容身份、消费者和适用验证。执行分派、时间和进度继续留在原 Task/PR，本文不复制可变台账。

<a id="ic-production"></a>
## 6. IC-PRODUCTION：从实际制作记录形成范围估算

先按设计的三种代表复杂度选择与当前制作相符的样本，说明为什么覆盖本次新增内容及哪里不能外推。gear、iron_ingot、factory_core 是复杂度参照，不是三件必须重新制作的交付任务；同一旧内容重复阅读也不是重复生产样本。

每个实际样本在原制作记录保存：内容 ID/版本、目标品质和入口、制作类别、所用模板/工具、参与职责、首次准备人时、重复制作人时、等待依赖日历时间、集成/验证/返工人时，以及完成/失败结果和消费者版本。缺实测填“未测”，不填 0；多项并行的人时不等于经过时间，自动生成耗时也不等于全部人工成本。

用同类样本的实际区间及数量形成重复制作估算，单列固定准备、集成、验证与返工，说明样本数、范围差异和外推假设。按设计公式避免重复计费；先把缺少来源/可用运输等依赖列为阻塞，再估其受影响范围，不用乐观平均数掩盖。货币成本需有实际费率/报价；日历排期需按依赖与可用人员另行安排。

**允许的结论：** 静态核对可报告本范围 2 个工厂 kind / 11 个配方的源与结构；单个实际样本可报告该样本制作/集成结果；代表性重复样本和一次适用的规格/依赖传播完成后，才按原[规模证据要求](../../engineering/doc-governance/product-documentation-standard.design.md#content-scale-evidence)评价对应范围的扩产风险。没有这些记录时，方法已设计、实际产能未验证，不能把目录数量乘任意单价或工时写成已批准预算。

<a id="ic-starter-forward-gate"></a>
## 7. IC-STARTER-GATE：同一初态的前向可行性

承接[共享前向 Gate](industrial-native-content-delivery.design.md#starter-fresh-forward-gate)与[当前来源适用性](industrial-native-content-delivery.design.md#starter-canonical-source-boundary)。先静态核输入、账本和原转换；实现该评估者时再以固定 source/build、实际入口/profile、同一合法初态与 grant 跑以下合同测试。当前源码的单步 quote 或结构测试不能填写为完整 Gate 通过。使用原[自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md#st-03)选择正式入口验证时，另记录真实操作与领域结果。

| 方法 | 独立输入与操作 | 预期与检查点 |
| --- | --- | --- |
| FG01 正向组合 | 原合法 starter、相同 owner/site/profile、建造与排产均已获准；分别取尚未建 Smelter、已就绪 Smelter 两种当前状态 | 按入口真实批量可到达 production_only；输出包含候选绑定、首成果与下一候选，已有工厂不重建。比较完整 live 状态及存储写入观察：无资源、队列、grant、receipt、milestone 或时间修改。 |
| FG02 当前输入与依赖 | 读取 owner 建造料、Smelter 输入和 Assembler 输入三个账本及实际 route/path；保持 canonical direct profile | 不要求 source/refinement/handoff/运输 receipt；Assembler 预置铁不由新产铁证明。换为确有上述依赖但缺 authority 的 profile 时，必须 no_safe，不能复制 direct 的适用判定。 |
| FG03 累计预算 | 在隔离且合法的相同规则初态把可用电力设为100，选择“建厂10 + iron 12 batch需96”；分别再用足够电力但剩余授权额度不足的输入 | 组合在剩余90E处拒绝排产，不能把两个独立阶段均能承担单项费用当整链可达；授权总量同样沿原grant逐步核算，查询不扣额度。 |
| FG04 单变量反例 | 分别缺建造料、site燃料、合法site/location、有效grant、当前profile或输出接收证据；其余与正向相同 | 每次返回实际最早且可行动缺口及真实恢复/复查点；world或其它site有同名材料不能抵销。未知字段不补零或强制标N/A。 |
| FG05 非当前转换 | 仅有目标设计、不能有界证明的完成条件、外部未来补给、未支持入口，或已提交但原结果未知 | no_safe并区分target/unknown；不注入ready对象、不自动跑无关job或再次提交。已有原件先按当前合法读权核对。 |
| FG06 绑定、漂移与恢复 | 重复同一只读请求；再独立改变owner、batches、ledger、grant用量/期限、profile或相关authority；真实提交前另制造有效状态变化 | 相同输入结果一致；影响因果的变化使旧候选失效，提交仍fresh重验；重连/replay不复制效果，原请求未知不换identity。 |
| FG07 完成边界与双入口 | 对同一可见窗口分别读Viewer与pure API；对照尚未开始、运行中、原结算与milestone、入口不支持所选参数 | 候选不等于accepted或produced；production_only仍undelivered，无需求减少/终端奖励；语义一致且能力差异明确。没有同候选实际运行则该层未验证。 |

核对预期时分别读取原建造规格、construction power、canonical recipe、grant语义和真实结算条件，不从被测 Gate 的输出反推预期。当前12批正向的已知电力为132−10−96=26；它只完成首产物，剩余26足以按原条件另评估Assembler/gear并非本Gate已交付这些结果。静态算术、单步合同、完整组合和实际入口四层分别报告。

<a id="ic-first-capability-continuation"></a>
## 8. IC-CONTINUATION：成果、受限选择与后继用途

承接[可选持续目标](gameplay-decision-delegation-return.design.md#first-capability-continuation)与[自然停点/回访](gameplay-decision-delegation-return.design.md#des-adopt-004)。这组方法不增加首产物门槛。静态核算沿原规则；合同/正式运行使用真实支持批量参数的入口和有限、已获准的环境。真人理解沿[首局真人方案](../../testing/manual/first-session-human-playtest.manual.md#human-study-assessment)独立观察，不给试玩者答案或世界写权限。

| 方法 | 输入、操作与独立预期 |
| --- | --- |
| CN01 实际成果 | 合法初态先建Smelter，再完成一个1-batch iron：电力132→122→114，site输入60/20→56/19，产物3 iron/1 slag；读原匹配job、owner/profile与milestone，不从余额反推完成。玩家可在此停下。 |
| CN02 重复与技术stable | 同一factory继续两个完全同canonical snapshot的1-batch job，各有独立实际结算；累计9 iron/3 slag、电力98、输入48/17。原repeat_count反映三次完成；一个3-batch job、重复读取同receipt、不同snapshot或无匹配原件不能凑数。只报告技术候选，不代签持续能力或信任。 |
| CN03 分账后继 | 按原合法条件建Assembler并完成4-batch gear，电力98→88→72；owner另付8 frame/10 iron/8 copper，Assembler site消耗自身8 iron并产4 gear，Smelter的9 iron/3 slag原样保留。分别核建厂/gear结果与用途，不产生不存在的运输或生产归因。 |
| CN04 可理解的真实限制 | 同状态请求12-batch iron只读quote：输入48/17足够，96E相对72E缺24；拒绝不产生job、receipt或扣费。不能展示为已开始工厂后来停机，也不能自动补电。 |
| CN05 明确减量并恢复推进 | 原大批未提交或已由原结果证明无效果、当前授权允许新选择时，明确选1 batch并fresh quote/提交；匹配结算后电力64、输入44/16、Smelter累计12 iron/4 slag。确认差异是合法新计划，不是悄悄改变旧操作或抹除旧义务。 |
| CN06 授权/未知反例 | 保持大批原请求unknown、撤销新写授权、改变owner或限定目的不允许减量，分别尝试CN05；对应情况下不新增效果，旧请求按当前合法读取/原协议恢复。查询权限也丢失时保留不可读状态，不以旧持有者身份继续。 |
| CN07 入口能力 | 分别核 native 可选batch与当前Viewer固定12批的真实操作面。Viewer不可因native方法通过就被记为支持小批；不支持的层保留缺口。原固定12批仍能完成其production_only首产物，不能为运行小批算例修改其默认语义。 |
| CN08 离场与回访 | 在首产物后、低电quote后和真实提交结果未知时分别离场/回访；另取无新事件、原窗口缺口两种情况。读取原成果/未决、当前权限与余额，提供真实下一步；不重播结算、不补离线收益或恢复电力。未提交quote不恢复为失败job；未知原操作先核对结果。 |
| CN09 玩家分项观察 | 请玩家说明产线投入/用途、低电原因、减量与扩展/停止的选择后果、下次继续理由，再核实际选择和原结果。分开判定运行可达、阻塞理解、恢复选择、至少一个中循环方向、信任与继续意愿；提示后回答、预约返回或技术stable不计自然留存。未发生相应观察如实未验证。 |

CN01–05的数量/电力轨迹是独立规则预期；当前实际准入若多出有效成本、时序、原件或容量约束，应先查原authority并修订受影响样例，不能屏蔽拒绝让算例通过。阻塞可在隔离测试中自然选择合法低电条件，不改玩家世界制造事故。每层记录候选、真实入口、授权、原件/账本与未覆盖项；方法写全不代表已执行这些生产、恢复或真人场景。
