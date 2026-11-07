# 内置 WASM 内容目录与交接核查手册

<a id="scope"></a>
## 1. 适用范围与结果口径

本手册验证[配对设计](builtin-wasm-content-delivery.design.md)中的内容身份、参数、组合、交接和生产关系。静态命令只读 Git 对象／文本，不运行 Cargo、WASM、游戏、模型、构建或发布。运行层方法另列前置和判据，只有实际执行并保留对应候选证据后才报告通过。

从仓库根目录执行静态脚本。设计的源基线为 `a23bc41943893da72dbbd78df65214a8a0ae964c`；核新候选时先固定新的 commit，再明确与旧基线的差异，不能只替换输出里的版本号。结果写回原 PR／制作记录，区分静态检查、测试定义审读、实际运行和玩家验收；不把计划写回长期设计作为通过记录。

<a id="bc-catalog"></a>
## 2. BC-CATALOG：源集合、身份与内容单元

以下脚本核直属 Cargo.toml 的 38 项集合、canonical map 的 37 个唯一 ID、各组 ID 清单及源常量；独立 provider 必须从 builtin 计数中分出。它只验证源身份，没有加载 artifact。

```bash
python3 - <<'PY'
import collections
import re
import subprocess

revision = "a23bc41943893da72dbbd78df65214a8a0ae964c"
def read(path):
    return subprocess.check_output(["git", "show", f"{revision}:{path}"], text=True)

prefix = "crates/oasis7_builtin_wasm_modules/"
paths = subprocess.check_output(
    ["git", "ls-tree", "-r", "--name-only", revision, prefix], text=True
).splitlines()
manifests = {p for p in paths if re.fullmatch(re.escape(prefix) + r"[^/]+/Cargo\.toml", p)}
artifact_dir = "crates/oasis7_runtime/src/world/artifacts/"
rows = [line.split() for line in read(artifact_dir + "builtin_module_manifest_map.txt").splitlines()
        if line.strip() and not line.lstrip().startswith("#")]
assert all(len(row) == 2 for row in rows), "map row shape changed"
mapping = dict(rows)
assert len(mapping) == len(rows) == 37, "duplicate or changed canonical IDs"
assert len(set(mapping.values())) == 37, "shared source mapping needs explicit review"
assert len(manifests) == 38, "source package scope changed"
provider = prefix + "local_test_provider/Cargo.toml"
assert manifests - set(mapping.values()) == {provider}
assert set(mapping.values()) <= manifests
groups = collections.Counter(module.split(".")[0] for module in mapping)
assert groups == {"m1": 10, "m4": 22, "m5": 5}, groups
for group in ("m1", "m4", "m5"):
    ids = [line.strip() for line in read(artifact_dir + group + "_builtin_module_ids.txt").splitlines()
           if line.strip() and not line.lstrip().startswith("#")]
    assert len(ids) == len(set(ids))
    assert set(ids) == {m for m in mapping if m.startswith(group + ".")}
for module_id, manifest in mapping.items():
    source = manifest.removesuffix("Cargo.toml") + "src/lib.rs"
    declared = re.search(r'const MODULE_ID: &str = "([^"]+)";', read(source))
    assert declared and declared.group(1) == module_id, (module_id, source)
    print(module_id, source)
provider_source = read(provider.removesuffix("Cargo.toml") + "src/lib.rs")
assert 'const MODULE_ID: &str = "module.runtime.local-test-provider";' in provider_source
print("revision", revision)
print("canonical_groups", dict(sorted(groups.items())))
print("source_packages", len(manifests), "canonical_ids", len(mapping), "independent_provider", 1)
PY
```

判据：每个 canonical ID 均能落到设计的 M1／M4／M5 内容表与实际源；provider 只算独立样本。集合变化时报告新增／删除和真实用途，先补相应内容解释，不能更新数字后就宣布完整。再检查保留的[四场景目录](../../world-simulator/scenario/asteroid-scenario-content-delivery.design.md#scenario-content-catalog)与[native 目录](industrial-native-content-delivery.design.md#native-content-catalog)没有被重新定义；其他 scenario 的工程测试用途不升级为正式关卡。

<a id="bc-content"></a>
## 3. BC-CONTENT：参数、默认值和内容语义

### 3.1 参数读审

下面输出 M4 源常量及共享 template include，供逐项对照设计表；同时断言 8 个 Product 的基线限制、tradable 和 quality 标签，以及三类实际模板的依赖数量。正则只服务此固定源格式，不是 Rust 解释器或 production source hasher；格式变更失败时回到源审查。

```bash
python3 - <<'PY'
import collections
import re
import subprocess

revision = "a23bc41943893da72dbbd78df65214a8a0ae964c"
def read(path):
    return subprocess.check_output(["git", "show", f"{revision}:{path}"], text=True)

map_path = "crates/oasis7_runtime/src/world/artifacts/builtin_module_manifest_map.txt"
mapping = dict(line.split() for line in read(map_path).splitlines()
               if line.strip() and not line.lstrip().startswith("#"))
expected_products = {
    "iron_ingot": (500, "true", ["industrial", "refined"]),
    "alloy_plate": (400, "true", ["industrial", "hardened"]),
    "control_chip": (200, "true", ["industrial", "precision"]),
    "motor_mk1": (128, "true", ["industrial", "precision", "hardened"]),
    "logistics_drone": (32, "true", ["baseline", "field_ready", "fleet_grade"]),
    "sensor_pack": (96, "true", ["precision", "survey"]),
    "module_rack": (48, "true", ["industrial", "governance_ready"]),
    "factory_core": (24, "false", ["critical", "governance"]),
}
templates = collections.Counter()
seen_products = set()
for module_id, manifest in mapping.items():
    if not module_id.startswith("m4."):
        continue
    source = read(manifest.removesuffix("Cargo.toml") + "src/lib.rs")
    constants = dict(re.findall(r"const (\w+):[^=]+?=\s*(.*?);", source, re.S))
    includes = re.findall(r'include!\("([^"]+)"\)', source)
    assert len(includes) == 1, (module_id, includes)
    templates[includes[0].rsplit("/", 1)[-1]] += 1
    print(module_id, {key: " ".join(value.split()) for key, value in constants.items()
                      if key != "MODULE_ID"}, "include", includes[0])
    if module_id.startswith("m4.product."):
        kind = constants["PRODUCT_ID"].strip('"')
        actual = (int(constants["STACK_LIMIT"]), constants["TRADABLE"],
                  re.findall(r'"([^"]+)"', constants["QUALITY_LEVELS"]))
        assert actual == expected_products[kind], (module_id, actual)
        seen_products.add(kind)
    if module_id.startswith("m4.recipe."):
        assert constants["RECIPE_ID"].strip('"') == module_id.removeprefix("m4.")
        assert constants["DURATION_TICKS"] == "1"
assert seen_products == set(expected_products)
assert templates == {"m4_factory_module_template.rs": 3,
                     "m4_recipe_module_template.rs": 11,
                     "m4_product_module_template.rs": 8}, templates
print("revision", revision, "template_dependents", dict(sorted(templates.items())))
PY
```

读 [`bootstrap_economy.rs`](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs) 的三个 `m4_default_*_profiles` 函数，对照设计 §3.4 的 23／10／11 项及 key，尤其检查：material 的物流属性不冒充 Product validator；recipe stage 与 product unlock_stage 分开；sink 是按主产出单位合并到实际消耗，不是初态库存。确认默认安装只插入缺失 profile，不覆盖已有值。

### 3.2 必须能解释的正负例

| 内容场景 | 核查来源与预期判断 | 不能由此得出的结论 |
| --- | --- | --- |
| drone Product 请求 kind=logistics_drone、amount=32 / 33 / 0，另给错误 kind | [product template](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_product_module_template.rs)与 drone 源；32 满足该模板的三项谓词，33、0 和错误 kind 拒绝 | 静态谓词判断不证明任意世界生产可完成；material profile 的 200 不覆盖 validator 的 32。 |
| factory_core Product | 源中 limit 24、tradable=false，quality 为 critical／governance | 不能把 governance 标签当成治理授权；也不能由一条 ProductValidated 证明实际交易入口可用。 |
| 两架 drone 的成本 | 原 Recipe 常量按接受批次缩放；有效 ProductProfile 的 sink 额外带来 hardware_part 4 | 不混入无该 profile 的 native 配置，不把 recipe bootstrap stage 当成越过 product scale_out。 |
| Recipe 的两批与一批 | [recipe template](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_recipe_module_template.rs)中每批材料／产物／power 缩放，duration 取常量 1；可用投入不足时 accepted_batches 可减少 | 不把 requested_batches 当已接受批数，不把 duration 乘批次，不产生随机品质。 |
| M1 radiation 与 cargo | [radiation 源](../../../crates/oasis7_builtin_wasm_modules/m1_power_radiation_harvest/src/lib.rs)、[cargo 源](../../../crates/oasis7_builtin_wasm_modules/m1_storage_cargo/src/lib.rs)和 bootstrap 订阅 | 前者当前有效动作输入更新私有 state，后者处理扩容事件；不是每 tick 世界余额赠送或完整实体背包。 |
| M5 普通票局／危机 | 对照对应源中的 event arm、run_tick、directive 与 [consumer](../../../crates/oasis7_runtime/src/world/gameplay_loop.rs) | GovernanceFinalize 不等于执行宪制变更；CrisisSpawn 的类型名不等于完整恢复项目。 |

如进行真实 Runtime／WASM 验证，先按[测试手册](../../../testing-manual.md#local-check-entry)选择原专业层与适用环境。原落点包括 [economy_bootstrap.rs](../../../crates/oasis7_runtime/src/tests/economy_bootstrap.rs)、[economy_module_validation_tests.rs](../../../crates/oasis7_runtime/src/tests/economy_module_validation_tests.rs)、[agent_default_modules.rs](../../../crates/oasis7_runtime/src/tests/agent_default_modules.rs)及 [gameplay_protocol_regressions.rs](../../../crates/oasis7_runtime/src/tests/gameplay_protocol_regressions.rs)。测试源码只给方法和扩展落点；运行记录必须注明 exact candidate、实际 active 模块／profile、输入、领域结果及未覆盖项。本手册不要求为目录修改启动这些运行。

<a id="bc-composition"></a>
## 4. BC-COMPOSITION：组合和采用证据

先检索真实调用者，保留完整结果并区分 helper、fixture 和生产入口：

```bash
git grep -n -E 'install_m1_power_bootstrap_modules|install_m1_agent_default_modules|install_m1_scenario_bootstrap_modules|install_m4_economy_bootstrap_modules|install_m5_gameplay_bootstrap_modules|install_gameplay_scenario_bootstrap_modules' a23bc41943893da72dbbd78df65214a8a0ae964c -- crates
```

按 [`bootstrap_power.rs`](../../../crates/oasis7_runtime/src/world/bootstrap_power.rs)、[`bootstrap_economy.rs`](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs)、[`bootstrap_gameplay.rs`](../../../crates/oasis7_runtime/src/world/bootstrap_gameplay.rs)顺序读真实调用和失败返回；检查设计的 2／4／6／22／5／33／29 分别是何种集合和条件，不能相加为世界拥有的功能数。读原测试的具体场景以确认方法位置：

| 组合场景 | 原方法／执行落点 | 判据和边界 |
| --- | --- | --- |
| M1 空白适用初态，toggle true 与 false；重复调用；已注册未 active | [power_bootstrap.rs](../../../crates/oasis7_runtime/src/tests/power_bootstrap.rs) 的 `install_scenario_bootstrap_modules_supports_default_package_toggle`、`install_scenario_bootstrap_modules_is_idempotent`、`install_power_bootstrap_modules_reactivates_registered_version` | true 的安装集合是 6，false 是 2；重复不制造另一组内容，false 不承担卸载。实际读 registry／manifest／events，不能只看 Ok。 |
| M1 storage 历史 state／版本 artifact 不适用 | 同 bootstrap 的验证函数及 [release manifest 回归](../../../crates/oasis7_runtime/src/tests/power_bootstrap_release_manifest_full.rs)、[movement settlement 回归](../../../crates/oasis7_runtime/src/tests/power_movement_settlement.rs) | 记录实际拒绝原因与保留状态；event era、旧 state、选定工件分别核，不绕过迁移后声称成功。 |
| M4 缺省 profile 世界与已有同 key profile 世界 | [economy_bootstrap.rs](../../../crates/oasis7_runtime/src/tests/economy_bootstrap.rs) 的 `install_m4_economy_bootstrap_modules_injects_layered_profiles`、`install_m4_economy_bootstrap_modules_is_idempotent`；已有 profile／中途失败按本行补专门样本 | 缺失项补入，既有值保留；核失败前后实际 profile／artifact／registry，不能预设整套事务回滚。 |
| 全套 M1→M4→M5 组合 | [gameplay_bootstrap.rs](../../../crates/oasis7_runtime/src/tests/gameplay_bootstrap.rs) 的 `install_gameplay_scenario_bootstrap_modules_stacks_base_and_gameplay` | 原默认组合在适用初态覆盖 33；若验证 false 分支则另核 29，不能把原 true 测试当 false 通过。 |
| 无 active Gameplay Tick、仅 Council、五项完整、停用其中一项 | [gameplay_loop.rs](../../../crates/oasis7_runtime/src/world/gameplay_loop.rs) 的 `has_active_gameplay_tick_modules`／`process_gameplay_cycles_with_modules`；真实组合回归在 [gameplay_protocol_regressions.rs](../../../crates/oasis7_runtime/src/tests/gameplay_protocol_regressions.rs) 补样本 | 无 active Gameplay Tick 才走整套 core fallback；其余核实际 directive 与领域事件，不假定缺失领域独立 fallback。尚无对应样本或未运行时记录未验证。 |

准备交付为“正式采用”时，再独立提供实际入口、world、seed／初始化来源、active manifest／实例、有效 profiles、artifact 身份以及一次真实消费的结果。若只有 helper／测试，结论停在“支持该组合的代码与方法已定位”。这是采用证据缺失，不是批准从测试入口直接生产激活。

<a id="bc-handoff"></a>
## 5. BC-HANDOFF：接收一项内容

1. 选择一项真实 ID，例如 `m4.product.finished.logistics_drone`；从设计表打开 source、Cargo.toml 和 product template，对照源 commit、输入 kind、上限、tradable、quality 及实际消费者。
2. 明确此次交付阶段：内容源审查、构建材料交接、治理后世界采用，或玩家入口验收。按[设计交接表](builtin-wasm-content-delivery.design.md#builtin-delivery-handoff)核该阶段实际材料，后续阶段缺失要明确保留，不能将阶段字段改成“通过”填空。
3. 对构建材料，沿[版本化输入闭包](../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md#build-source-input-rule)和[工件版本绑定](../../p2p/consensus/builtin-wasm-identity-consensus.design.md#builtin-artifact-version-binding)核已有 receipt／hash／identity 的一致性。没有材料就报告未取得；不在内容检查中生成假 receipt、替换 manifest 或触发构建。
4. 对来源与编辑权利，按[原来源手册](content-production-and-rights.manual.md#cp-rights)核实际适用输入、许可、notice 和继续编辑条件。内容作者、Git 历史、根 MIT 声明、所用工具分别是不同材料。
5. 对世界采用，按 BC-COMPOSITION 核实际事实；对玩家入口，按原产品与专业验收取证。接收结果列明已取得材料和欠缺层次，不用一个“包通过”覆盖所有层。

判据：接收方能取得当前声明阶段所需源／版本／关系并继续工作；没有的 artifact、授权、active 状态或运行证据保持未取得，不被目录本身补齐。现有 native 首局和四场景的交接继续采用原手册，不重建重复包。

<a id="bc-change"></a>
## 6. BC-CHANGE：单项、模板与默认值的变更推演

在静态 review 中使用现有 drone 关系作以下推演，**不实际修改数值或发布内容**：

| 推演 | 必须列出的影响集合 | 通过判据 |
| --- | --- | --- |
| 获准改变 drone Product 的上限或 quality 标签 | 本源、共享模板对参数的解释、实际 active validator 选择、可信预检与完成验证、玩家说明；MaterialProfile 默认 200 和 ProductProfile sink 2 单列 | 说明改动哪一层；不能推定其他层自动改变或已有任务被追溯重判。 |
| 获准改变 product template 的接受条件 | 用 BC-CONTENT 的真实 include 集合找 8 个依赖包；单列各自 ID、参数差异、source closure／artifact／identity 及消费者 | 8 个范围可解释，不从一个 drone 例子外推所有谓词；recipe／factory 无实际依赖时不假称一并改变。 |
| 获准改变默认 profile | `m4_default_*_profiles` 对应 key、缺省世界和已有同 key 世界、stage／sink／物流字段的真实消费者 | 分开预测两类世界；没有显式迁移时不宣称已有世界被更新，不生成库存。 |

真实实施时保留已批准规则、变更前后源、受影响依赖、所选 candidate、实际适用验证与未覆盖边界；创作者的编辑／发布／采用仍使用原发布与治理流程。出现 state、权限、兼容或新消费者变化时回到原 owner 的设计，不能以本目录更新代替。

<a id="bc-production"></a>
## 7. BC-PRODUCTION：质量、成本和批次接收

1. 选择至少能代表本次实际范围的工作样本。单 Product 参数检查不能代表 M5 有状态规则；全五项组合的样本不能代表部分集合或旧 state 迁移。
2. 每个样本记录真实单元、初次／重复制作、作者／集成／验证投入、工具直接费用、返工、等待和维护范围。算法和去重沿[CP-COST](content-production-and-rights.manual.md#cp-cost)，没有费率或测量就保持缺失，不填零。
3. 依据设计[生产质量表](builtin-wasm-content-delivery.design.md#builtin-production)检查角色是否能审查该规则和消费者；单项目录完整、shared template 完整、世界组合可用分别给结论。
4. 扩大数量或变更模板／state／profile 时重新判断样本适用性。保留需要的来源、集成和兼容工作，不能将 38 个源包乘一个平均人时作为全游戏工期。

判据：可计算范围和未测风险明确；可用容量及依赖有真实输入，所声明交付质量有对应材料。此处不要求新增全面预算表，也不以没有预算批准否认已经完成的有界内容设计。
