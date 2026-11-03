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
