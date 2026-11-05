# 小行星碎片场景内容制作与验证手册

<a id="scope"></a>
## 1. 适用范围与执行层级

本手册承接[四场景内容设计](asteroid-scenario-content-delivery.design.md#scope)，供内容作者、Simulator/Runtime 消费者和 QA 核对实际输入、制作交接及适用验证。默认从仓库根目录操作，并固定所读 source revision；四份 JSON 和当前方法可用于静态审读，代码命令只在实际生成或行为验证任务需要时执行。

只修目录/说明时执行只读 JSON、文本/链接检查及既有文档检查，不启动 Cargo、生成器、游戏、UI、模型或素材制作。实际场景/生成/消费者变化时，按受影响范围选择下列运行方法；结果记录在原 PR/QA 或制作记录，不把本手册存在记作 PASS。正式网络、世界激活、玩家流程与 Agent 自测继续沿原授权和专业入口。

<a id="asc-catalog"></a>
## 2. ASC-CATALOG：四份实际源与内容目录

在仓库根运行以下只读目录提取。它解析现有 JSON、核对文件名与 ID，并输出实际字段和文件 SHA-256；不生成世界、不加载 Rust 默认值，也不验证运行时 schema 或玩法。

```bash
python3 - <<'PY'
from pathlib import Path
import hashlib
import json

root = Path("crates/oasis7_simulator/scenarios")
ids = (
    "asteroid_fragment_bootstrap",
    "asteroid_fragment_detail_bootstrap",
    "asteroid_fragment_twin_region_bootstrap",
    "asteroid_fragment_triad_region_bootstrap",
)
for scenario_id in ids:
    path = root / f"{scenario_id}.json"
    raw = path.read_bytes()
    spec = json.loads(raw)
    if spec.get("id") != scenario_id:
        raise SystemExit(f"scenario id mismatch: {path}")
    record = {
        "path": str(path),
        "sha256": hashlib.sha256(raw).hexdigest(),
        "id": spec["id"],
        "name": spec.get("name"),
        "seed": spec.get("seed"),
        "asteroid_fragment": spec.get("asteroid_fragment", "not explicit"),
        "origin": spec.get("origin", "not explicit"),
        "location_generator": spec.get("location_generator", "not explicit"),
        "agents": spec.get("agents", "not explicit"),
        "power_plants": spec.get("power_plants", "not explicit"),
        "module_visual_entities": spec.get("module_visual_entities", "not explicit"),
    }
    print(json.dumps(record, ensure_ascii=False, sort_keys=True))
PY
```

将输出逐项与[场景目录](asteroid-scenario-content-delivery.design.md#scenario-content-catalog)比较，再读 [scenario.rs](../../../crates/oasis7_simulator/src/scenario.rs)、[init.rs](../../../crates/oasis7_simulator/src/init.rs) 和 [types.rs](../../../crates/oasis7_simulator/src/types.rs)补齐未显式字段。判据是四个 ID/源对应，seed/offset/chunks/spacing、origin、普通地点、Agent 和库存说明一致，设施/视觉实体没有被想当然补入。SHA-256 只标识这一文件，不证明程序编译包含它或任何世界已消费它。

`region-*` 应按普通地点解释，不能补成阵营/资源授权；detail 的零 Agent 只用于对应 Simulator 初态。发现文档与源不符先核原 authority：目录错修目录，规则或实现冲突交原 owner；不通过修改源数据去迎合目录。

<a id="asc-effective-input"></a>
## 3. ASC-EFFECTIVE-INPUT：有效配置与初始模型

同一 seed 不足以固定世界。按[初始化合同](chunked-fragment-generation.prd.md#chunk-init-contract)逐项核对：

| 检查对象 | 静态输入与操作 | 实际生成/验收时追加的材料与判据 |
| --- | --- | --- |
| 世界尺寸与生成默认值 | 读 [world_geometry.rs](../../../crates/oasis7_agent_api/src/world_geometry.rs)与[WorldConfig 默认值](../../../crates/oasis7_simulator/src/world_model_config.rs)，确认实际调用者使用何配置 | 保存完整有效 config 及程序版本；索引 chunks 与已生成 chunks 分别计数。 |
| seed 与普通地点 | 读 `WorldScenarioSpec::into_init_config`；确认 override 在生成地点前完成，普通地点使用同一有效 seed | 比较 region ID、位置及普通 profile；显式 `WorldInitConfig.locations` 按原合同保留坐标，不误称重新按 seed 生成。 |
| fragment 输入 | 读 seed/offset、spacing override 与 bootstrap sanitization | 记录有效 fragment 根、完整配置及实际调用顺序；不能以 spec `config` 代替未显式默认值。 |
| 实际生成范围 | 追 `build_world_model` 的 seed positions → bootstrap → Agent positions | 逐项列出实际生成 chunk 及来源，核每个显式 bootstrap 被处理；bootstrap 列表不能当总数量。 |
| 内容保留 | 读初始 `model.locations/agents/power_plants` | 按四场景逐项核 origin、region/Agent、资源初值、fragment/profile/budget；detail 核四个指定 chunks、无 origin、Agent=0。 |

可复用 [init.rs 测试源](../../../crates/oasis7_simulator/src/tests/init.rs)中的 `scenario_templates_build_models`、`scenarios_are_stable`、`scenario_asteroid_fragment_min_spacing_overrides_world_config` 和四个场景专用测试：

- `asteroid_fragment_bootstrap_seeds_fragments_and_resources`
- `asteroid_fragment_twin_region_bootstrap_seeds_fragments_and_regions`
- `asteroid_fragment_detail_bootstrap_seeds_dense_fragments_for_viewer`
- `asteroid_fragment_triad_region_bootstrap_seeds_fragments_and_regions`

四场景专用断言检查其必要实体、显式 bootstrap 的 Generated/Exhausted 状态及 chunk budget；detail 还检查零 Agent、无 origin 和存在 frag。它们不证明每个 chunk 各有固定数量/种类，也不统计不可用开局。`scenario_templates_build_models` 只断言模型地点非空；`scenarios_are_stable` 不是全模型逐字节的固定 golden snapshot。实际模型没有取到时只记录源码审读结果，不填运行通过。

<a id="asc-material"></a>
## 4. ASC-MATERIAL：具体材质与组成

1. 读 [types.rs](../../../crates/oasis7_simulator/src/types.rs) 的 `MaterialKind/LocationProfile` 与 [fragment_physics.rs](../../../crates/oasis7_simulator/src/fragment_physics.rs) 的 `material_compounds`；逐行核对设计五类组成方向，精确数值以源为准。
2. 读 [world_model_config.rs](../../../crates/oasis7_simulator/src/world_model_config.rs) 的默认分布与权重；核四 JSON 没有选择另一个 distribution profile。若实际调用方改变 WorldConfig，以其有效配置为准，不能继续引用默认 uniform 代表该候选。
3. 实际候选检查时，选真实 frag，保存其 ID、位置、`LocationProfile` 和 `fragment_profile`；从生成调用追合成与截断后的 profile。普通 region 只有默认 `LocationProfile` 时，不补造 block/compound 内容。

通过判据为解释与所选实际 profile 同源，不把采样权重当实测比例，不用材质名推出安全路线、工业材料或配方收益。现有 `chunk_generated_fragments_include_fragment_profile` 位于 [init 测试源](../../../crates/oasis7_simulator/src/tests/init.rs)，只覆盖该 fixture 的 profile/预算相关断言；其他材质分布、物理边界、视觉品质按实际变化另选原测试或专业验收，不由该名称代签。

<a id="asc-quantity"></a>
## 5. ASC-QUANTITY：资源作用域与数量

在所选真实初始模型或 snapshot 中定位一个 frag 及其所属 chunk，分别读取 `fragment_budget.total_by_element_g`、`remaining_by_element_g`、所属 `chunk_resource_budgets` 的同名字段、地点和 Agent 的 `ResourceStock`。记录这些值对应的对象、作用域和 snapshot；沿[原预算合同](chunked-fragment-generation.prd.md#chunk-budget-contract)核对，而不是重新生成后拿初始 total 代替当前 remaining。

纯静态审读追 [generate_chunk_fragments](../../../crates/oasis7_simulator/src/init.rs)的 profile 截断→budget→fragment 写入→chunk 累计，确认设计没有把普通 region 当资源预算。实际资源变更验证可按原 `consume_fragment_resource_keeps_fragment_and_chunk_conservation` 测试源及 [UWS 资源预算方法](../../../testing-manual.md#uws-v-resource-budget)选择范围；两者定义存在不表示本轮执行。

通过判据是当前来源/数量可追到正确对象，其他库存没有替代当前预算。边界中心归属疑点仍沿原专业设计处理；缺预算、错 chunk、已消耗后用重生成初值、或把 Metal 直接记成工业 `iron_ore` 均不能判同源通过。自然资源到工业输入的实际交付继续使用[工业初态手册](../../game/gameplay/industrial-native-content-delivery.manual.md#ic-initial-state)。

<a id="asc-spawn"></a>
## 6. ASC-SPAWN：出生候选、坐标与失败

读 [init.rs](../../../crates/oasis7_simulator/src/init.rs) 与 [frag_spawn.rs](../../../crates/oasis7_simulator/src/frag_spawn.rs)，核所选场景的 Agent 数、有效 seed、候选集合排序、选中 `location_id`、frag 半径和最终坐标。实际验证同时保存地点中心和 Agent 坐标，不能只记录“在某 frag”。detail 的零 Agent 是有效内容，不给它补一个出生点再称原样通过。

[init_agent_frag_spawn.rs](../../../crates/oasis7_simulator/src/tests/init_agent_frag_spawn.rs) 的 `agents_prefer_fragment_spawn_locations_when_fragments_exist` 仅检查 frag 前缀；`fragment_spawn_positions_stand_off_above_surface` 检查 x/y 相同、z 更高、间距 `2000..5100 cm` 和空间内。这些不能证明精确 5000cm、20m/中心两个 fallback 全分支、一般碰撞避让或全部 seed 确定性。`init_is_deterministic_with_seed` 和 `init_requires_spawn_location` 在 [init 测试源](../../../crates/oasis7_simulator/src/tests/init.rs)，各自只覆盖原 fixture 的模型重复构建与无候选错误。

需要扩展站位行为声明时按原[出生验证映射](agent-frag-initial-spawn-position.design.md#fsp-verification)准备匹配输入和独立观察，不把 planned 分支说成现有测试。初始化失败则保留具体错误并修正输入/实现；不能忽略错误交付部分模型。Viewer 是否保留偏移及玩家能否辨认分别交原 Viewer/QA 验证。

<a id="asc-import"></a>
## 7. ASC-IMPORT：编辑、导出与导入方法

### 7.1 Simulator 内容预检

内置编辑修改原 JSON 后重编译消费者；外部试作使用独立 JSON，沿相同 spec 结构，不修改现存世界。选择其中一种实际入口，不能同时传场景名和 `--scenario-file`：

```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_simulator --bin oasis7_init_demo -- asteroid_fragment_detail_bootstrap
env -u RUSTC_WRAPPER cargo run -p oasis7_simulator --bin oasis7_init_demo -- --scenario-file "$ASC_SCENARIO_FILE"
```

第二条运行前将 `ASC_SCENARIO_FILE` 设为实际待验 JSON 的路径并记录文件摘要。demo 能报告统计和地点/Agent 库存，`--summary-only` 只保留摘要；它不打印完整位置、profile、预算或实际 chunk 列表。要判初态保留，仍使用 §3/§6 的模型与测试入口。不可读/解析失败、参数冲突或未知场景应保留实际错误，不能静默替换成其他场景。

### 7.2 既有 generated-world 工件

当前 [artifact CLI](../../../crates/oasis7_ops_tools/src/bin/oasis7_generate_world_artifact.rs)只接受注册场景。实际制作任务先固定待构建版本、实际 public manifest 及独立的新输出位置；`ASC_OUTPUT_ROOT` 指向本次输出根，`ASC_PUBLIC_MANIFEST` 指向实际可读的非空 JSON 数组文件。不要覆盖正在消费或身份不明的产物。命令形式为：

```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_ops_tools --bin oasis7_generate_world_artifact -- \
  --scenario asteroid_fragment_bootstrap \
  --world-dir "$ASC_OUTPUT_ROOT/generated-scenario-world" \
  --provenance-out "$ASC_OUTPUT_ROOT/world-generation-provenance.json" \
  --public-manifest "$ASC_PUBLIC_MANIFEST"
```

生成器使用默认 WorldConfig 和内置 spec，不接受 demo 的外部文件参数。保存实际命令/退出码、生成程序版本、spec 与有效配置、manifest、输出 snapshot/journal 和 provenance；具体文件集合沿 `save_to_dir` 的原格式，不手工删减日志。

原 [Viewer CLI](../../../crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs)的 `--generated-world-dir` 读取上述父目录，检查 `generated-scenario-world/snapshot.json`、`journal.json` 与 provenance 存在，并拒绝同时给 named scenario。实际启用前先核该任务采用的是本地消费者还是 world-service，按 §8 对账；本段不授权启动正式世界。若制作的是 governed 组合，直接复用 [UWS-V-ARTIFACT](../../../testing-manual.md#uws-v-artifact) 的现有 build/validate 方法与其真实前置，不用 sidecar 替代 audited world。

<a id="asc-consumer"></a>
## 8. ASC-CONSUMER：目标消费者的内容对账

先沿[实际分流](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/support.rs)确定 world-service、generated sidecar、named scenario 或本地默认入口，再读 [bootstrap.rs](../../../crates/oasis7/src/runtime_host/bootstrap.rs)。源模型和目标 snapshot 要属于同一候选和明确时间点；记录转换后的内容，不要求不同消费者强行相同。

| 核查项 | 当前应如何解释 | 目标环境追加判据 |
| --- | --- | --- |
| 四场景 origin/region/frag | Simulator 按原内容保留；named Runtime 不保留完整 seed model；sidecar 额外保留 seed model 用于映射 | 逐个核实际 snapshot 中的 ID、profile、Agent 引用及缺失原因，不以模型非空代替。 |
| detail Agent=0 | `bootstrap_runtime_world_from_model` 会补 `runtime-agent-0/1` | 若目标是纯碎片零 Agent，普通这两条 Runtime 路径不能填原样通过；Simulator 结果与 Runtime 结果分开。 |
| Agent Electricity/Data | Runtime 对每个 Agent 至少设置 `32/8` | 记录场景原值和消费值；不要将覆盖后的资源算作场景自然资源或已生产成果。 |
| 本地默认 starter | 程序内 seed model 与固定工业准备，非四 JSON 的别名 | 核实际程序输入与 seed Agent；工业资格沿原工业手册。 |
| world-service/formal 工件 | 权威读取/外部准备入口 | 缺真实工件或授权材料时保留未核实，不用本地 seed 常量代填。 |

现有 `generated_world_sidecar_initializes_runtime_live_snapshot_map` 位于 [snapshot_generated_world.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/snapshot_generated_world.rs)，检查保留地点/profile、Agent 地点引用与初始化 chunk config，并明确 live refill preview 不启用。它不覆盖四个场景全部内容、零 Agent 保留或 provenance 绑定。本地默认测试沿 [UWS-V-LOCAL-SPAWN](../../../testing-manual.md#uws-v-local-spawn)；正式统一消费者仍沿 [UWS-V-CONSUMERS](../../../testing-manual.md#uws-v-consumers)，缺正式选择不以局部测试代签。

<a id="asc-provenance"></a>
## 9. ASC-PROVENANCE：来源声明分层

1. **存在与可取得**：确认所消费路径、文件版本和原始材料可读；现有 CLI/bootstrap 对存在性的检查只支持这一前置，不能写成内容已校验。
2. **实际内容核对**：读取 provenance 六字段，比较实际 spec/seed、manifest 字节摘要和条目数，再读 sidecar 的 config/model/chunk runtime。缺默认值记录或内容不一致时明确报告；当前 loader 不会因为这些差异自动拒绝。
3. **同次内容绑定**：按[原来源目标](scenario-files.prd.md#generated-world-provenance-contract)检查生产端是否将 provenance 与实际 snapshot/journal 内容绑定、消费者是否重算核验。当前六字段缺这项绑定；事后单独计算一个未知产物的摘要不能补成来源证明。
4. **独立重建**：需要可复现声明时固定程序、全部有效输入与触发顺序，比较两次初始模型、预算和 journal；仅重新加载同一快照不算独立重建。历史恢复沿原 replay 合同，不能用此步骤重算现有世界 remaining。

将 seed=9/provenance seed=42 的既有 fixture 识别为“映射测试未验证来源”，不要改写成一致样本。非法/缺失 provenance、同 seed 不同配置/sidecar、manifest 不匹配等完整负例仍按原合同由实现候选补齐；本手册不宣称已有全部拒绝测试。候选材料不足时给出对应未验证层级，已确认不匹配不能记为通过。

<a id="asc-validation"></a>
## 10. ASC-VALIDATION：按变化选择现有验证

执行者先确认具名测试仍存在、发现实际命中且使用正确 package/features；0 tests、编译失败、未执行与通过分别记录。以下是命令形式及证明边界，不是本轮运行清单：

| 变化与目标 | 原有命令入口 | 可证明范围 |
| --- | --- | --- |
| 场景 ID/装配 | `env -u RUSTC_WRAPPER cargo test -p oasis7_simulator --lib scenario_specs_match_ids`；其他 filter 依 §3 选择 | 对应单元断言；不会验证内容意义或玩家可达性。 |
| 四场景初态或 spacing | 同上 package/`--lib`，分别使用 §3 具名 filter | 实际 fixture 的初态与 override；不将总测试数当分布覆盖。 |
| 出生/资源作用域 | 同上 package/`--lib`，按 §4–§6 的实际具名 filter | 对应 profile、守恒、站位/失败断言；未涵盖部分单列。 |
| demo 加载 | `env -u RUSTC_WRAPPER cargo test -p oasis7_simulator --features test_tier_full --test oasis7_init_demo oasis7_init_demo_runs_asteroid_fragment` | 四个 asteroid summary tests；实际只核退出及标签/摘要字段，不核完整模型。 |
| 外部 JSON demo 路径 | 同一 CLI 测试参数，将 filter 换为 `oasis7_init_demo_runs_from_scenario_file` | 当前 minimal 文件的成功读取；不代表完整非法/未知字段负例。 |
| sidecar 实际映射 | `env -u RUSTC_WRAPPER cargo test -p oasis7_viewer_runtime --lib --features test_tier_required generated_world_sidecar_initializes_runtime_live_snapshot_map` | §8 的单 fixture 映射及禁用 refill；不证明 provenance 或正式资源。 |

CLI 测试源为 [oasis7_init_demo.rs](../../../crates/oasis7_simulator/tests/oasis7_init_demo.rs)，四个 asteroid 与外部文件用例受 `test_tier_full` 控制；不能省略 features 后把零发现计为通过。生成与消费格式、回放、共用默认值变更还须按原专业影响选择相关验证；本文不扩大为每次目录修改的全库回归。

<a id="asc-change"></a>
## 11. ASC-CHANGE：一次真实变化的交接

以设计中的 twin seed/spacing 样例或本次实际改动为对象：

1. 保存原/新 source revision 与实际 diff，确定是说明、场景输入、共用默认值还是消费者变化。
2. 先核原 authority 和变化理由，再列会受影响的普通地点、触发顺序、fragment/profile/budget、Agent、sidecar/Runtime 和用户表达；引用存在不代表文件必须修改。
3. 沿 §7 选择真实编辑/导入入口，记录需重编译/重建的程序或产物。旧世界与旧证据保持原身份，不直接用新源覆盖其历史结论。
4. 制作方交输入及产物，消费方交实际加载与内容保留结果；QA 按 §3–§10 判断受影响正例、失败/恢复与未覆盖项。source 收到、字段通过、映射通过分别记录。
5. 共同依赖冲突由指定维护者整合后核实际组合；若缺来源绑定、正式选择或目标环境，记录具体阻塞及原 owner，不靠换入口或删掉反例宣布完成。

纸面依赖核对只能称方法审读。只有真实输入变化、实际重建/消费及观察结果齐备时，才把它计为规格变更传播或内容生产证据。

<a id="asc-production"></a>
## 12. ASC-PRODUCTION：制作范围与成本记录

按[设计的复杂度类别](asteroid-scenario-content-delivery.design.md#production-scope-and-cost)选择与实际变化相符的样本，记录选用原因、目标入口/品质、实际数量和未覆盖差异。使用原制作 PR 或已有记录，分别填写首次工具准备、内容编辑、重建/导入、审读、目标环境验证及返工的人时；等待依赖另记日历时间，避免重复计费。

已有样本足以覆盖同类型重复制作时可以复用，说明适用的源/工具/消费者与品质范围。只有一次精调、静态目录输出或生成成功时，不给出批量产能结论；跨算法、正式世界、UI 或玩法变化需另核复杂度。估算标明依据与区间，缺实际样本保留未评估，不把四个 JSON 数量换算成人力、工期或全游戏完成度。
