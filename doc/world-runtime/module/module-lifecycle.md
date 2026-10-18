# oasis7 Runtime：模块治理与生命周期（设计分册）

审计轮次: 4

本分册为 `doc/world-runtime/prd.md` 的详细展开。

## 模块治理与兼容性（草案）
- **版本与兼容**：`interface_version` 由内核维护；模块声明兼容范围，若不兼容则拒绝加载。
- **治理闭环**：模块变更走 `propose → shadow → approve → apply`，升级/回滚均形成审计事件。
- **沙箱限制**：内存上限、指令燃料（gas）、调用频率、输出/事件大小上限。
- **能力/政策**：模块不能直接 I/O，只能产出 `EffectIntent`，由 capability/policy 决定是否执行。
- **确定性约束**：禁止读取真实时间/随机数；非确定性来源必须通过 receipt 写回事件流。

## 模块注册表与存储（草案）

> 目标：用**内容寻址**与**审计元数据**管理 WASM 模块，支持可回放、可治理的动态装载。

**现有存储布局（兼容示意）**
- `module_registry.json`：模块索引（`module_id/version` → 完整 manifest 与注册记录）；内容哈希不是 manifest 主键。
- `modules/<wasm_hash>.wasm`：WASM 工件（只读、内容地址）
- `modules/<wasm_hash>.meta.json`：旧版单 manifest 元信息；不能表示共享 H 的多个不同 manifest。新保存/Observer 合同见[共享内容与退休闭包](#module-retirement-and-shared-content)，此旧布局不再是新实现应复制的完整方案。

**module_registry.json（示意结构）**
```
{
  "version": 1,
  "updated_at": 123,
  "records": [
    {
      "wasm_hash": "...",
      "module_id": "m.weather",
      "name": "Weather",
      "version": "0.1.0",
      "interface_version": "wasm-1",
      "kind": "Reducer",
      "registered_at": 120,
      "registered_by": "agent:alpha",
      "audit_ref": "event:1234"
    }
  ]
}
```

**modules/<wasm_hash>.meta.json（示意结构）**
```
{
  "module_id": "m.weather",
  "name": "Weather",
  "version": "0.1.0",
  "interface_version": "wasm-1",
  "kind": "Reducer",
  "wasm_hash": "...",
  "exports": ["reduce"],
  "subscriptions": ["WorldEvent/WeatherTick"],
  "required_caps": ["cap.weather.query"],
  "limits": { "max_mem_bytes": 1048576, "max_gas": 100000, "max_call_rate": 1 }
}
```

**ModuleRecord（索引条目）**
```rust
struct ModuleRecord {
    wasm_hash: String,
    module_id: String,
    name: String,
    version: String,
    interface_version: String,
    kind: ModuleKind,
    registered_at: i64,
    registered_by: String,   // agent_id / system
    audit_ref: String,       // 对应 RegisterModule 事件 id
}
```

**加载/缓存策略**
- **按哈希装载**：模块加载必须提供 `wasm_hash`，不允许同名替换。
- **LRU 缓存**：内存中缓存已编译模块（带 `max_cached_modules` 上限）。
- **冷启动**：按需从 `modules/` 读取工件；找不到则拒绝加载并记录事件。

## 模块注册 Happy Path（草案）

```
ArtifactWrite(wasm_hash) 
  -> ProposeModuleChangeSet(register+activate)
    -> ShadowReport(pass)
      -> Approve
        -> Apply
          -> RegisterModule event
          -> ActivateModule event
          -> module_registry.json 更新
```

## 模块注册 Failure Path（草案）

```
ArtifactWrite(wasm_hash)
  -> ProposeModuleChangeSet(register+activate)
    -> ShadowReport(failed)
      -> Reject
        -> 无 Apply（不写入模块事件/注册表）
```

```
ArtifactWrite(wasm_hash)
  -> ProposeModuleChangeSet(register+activate)
    -> ShadowReport(pass)
      -> Approve
        -> Apply
          -> ModuleCallFailed(reject_reason)
          -> 无 Register/Activate（不更新注册表）
```

## 模块治理流程接入（草案）

**流程（概要）**
1. Agent 编译模块 → 计算 `wasm_hash` → 将工件写入 `modules/<wasm_hash>.wasm`。
2. 生成 `ModuleManifest` 与变更计划（Register/Activate/Upgrade）。
3. `propose → shadow → approve → apply`：治理闭环审查模块变更。
4. `apply` 成功后写入 `RegisterModule/ActivateModule/UpgradeModule` 事件，并更新模块注册表。

**Shadow 校验（示意）**
- 工件存在性与哈希一致性校验（`wasm_hash`）。
- 接口版本与 ABI 兼容性校验（`interface_version`）。
- `required_caps` 与 `Policy` 规则校验。
- `limits` 合法性校验（不超过系统上限）。

**Apply 行为（示意）**
- 生成模块生命周期事件并追加到事件流。
- 更新 `module_registry.json` 与内存缓存索引。
- 若任何校验失败，拒绝 apply 并记录 `ModuleCallFailed`（带拒绝原因）。

**ModuleChangeSet（示意）**
```rust
struct ModuleChangeSet {
    register: Vec<ModuleManifest>,
    activate: Vec<ModuleActivation>,
    deactivate: Vec<ModuleDeactivation>,
    upgrade: Vec<ModuleUpgrade>,
}

struct ModuleActivation { module_id: String, version: String }
struct ModuleDeactivation { module_id: String, reason: String }
struct ModuleUpgrade { module_id: String, from_version: String, to_version: String, wasm_hash: String }
```

**Apply 事件序列（示意）**
1. `RegisterModule`（若有 register）
2. `UpgradeModule`（若有 upgrade）
3. `ActivateModule`（若有 activate）
4. `DeactivateModule`（若有 deactivate）
> 顺序固定以保证回放确定性；同类事件按 `module_id` 字典序处理。

**ModuleChangeSet 应用算法（当前 governed proposal 路径）**
```
fn apply_governed_proposal(changes: ModuleChangeSet) -> Result<()> {
  let mut prepared = PreparedGovernanceProposalApply::against(&world);
  prepared.validate_changes(changes)?;
  prepared.shadow_check(changes)?;

  // 1) register
  for m in sort_by_module_id(changes.register) {
    prepared.write_event(RegisterModule { ..m });
    prepared.registry.insert(m);
  }

  // 2) upgrade
  for u in sort_by_module_id(changes.upgrade) {
    prepared.write_event(UpgradeModule { ..u });
    prepared.registry.update(u);
  }

  // 3) activate
  for a in sort_by_module_id(changes.activate) {
    prepared.write_event(ActivateModule { ..a });
    prepared.registry.activate(a);
  }

  // 4) deactivate
  for d in sort_by_module_id(changes.deactivate) {
    prepared.write_event(DeactivateModule { ..d });
    prepared.registry.deactivate(d);
  }

  prepared.prepare_manifest_governance_journal_and_consensus()?;
  prepared.install_infallible(&mut world); // one publication seam
  Ok(())
}
```

当前 typed prepared publication 覆盖 governed proposal apply：在 immutable base 上预演 registry、artifact、tick schedule、prepared-subscription cache invalidation、manifest、proposal status、event id/era、bounded journal/backpressure 与最终 tick consensus；排序后的 lifecycle events 之后固定追加 `ManifestUpdated`、`Governance::Applied`，全部成功后才通过单一不可失败 seam 安装。任一预演或 post-prepare failure 都不发布中间态，register/upgrade/activate/deactivate 的 authority-drift 回归与混合成功批次的连续 event、registry/active、Tick schedule、manifest/governance 顺序及最终 consensus 回归共同约束该合同。它不等同于完整 lifecycle transaction；非 proposal 入口、persisted instance state 对齐、恢复/回放、durable external effect 与 receipt/outbox 仍是 `partial` / `target`。

**ModuleChangeSet 校验规则（示意）**
- `module_id` 在 `register/activate/deactivate/upgrade` 内不得重复冲突。
- `register` 与 `upgrade` 不能同时针对同一 `module_id`。
- `activate` 与 `deactivate` 若同时出现，按顺序执行，但必须显式声明（避免隐式切换）。
- 此处 `ModuleChangeSet.upgrade` 是全局版本注册/激活操作，`from_version` 校验当前全局激活版本；`to_version` 必须单调递增。对持久实例执行正常 upgrade 时，仅将“目标必须首次注册”和“源版本取全局 active”分别替换为注册复用与实例源版本判断；其余既有版本合法性约束保留，见[实例迁移](#instance-upgrade-version-reuse)。
- `register` 后默认不自动激活，需显式 `activate`（保证可审计变更意图）。
- 所有变更均需通过 shadow 校验（hash/ABI/limits/caps）。

## ModuleChangeSet 在 Manifest/Patch 中的编码（草案）

**Manifest 扩展**
```rust
struct Manifest {
    reducers: Vec<ReducerSpec>,
    modules: Vec<ModuleManifest>,
    module_changes: Option<ModuleChangeSet>,
    effects: Vec<EffectSpec>,
    caps: Vec<CapabilityGrant>,
    policies: Vec<PolicyRule>,
    routing: RoutingSpec,
    defaults: Defaults,
}
```

**Patch 路径约定（示意）**
- `/modules`：替换模块清单（`set`）
- `/module_changes`：提交模块变更计划（`set`）

**Patch 示例（注册 + 激活）**
```
{
  "base_manifest_hash": "...",
  "ops": [
    { "op": "set", "path": "/modules", "value": [ ... ] },
    { "op": "set", "path": "/module_changes", "value": {
        "register": [ { "module_id": "m.weather", "version": "0.1.0", ... } ],
        "activate": [ { "module_id": "m.weather", "version": "0.1.0" } ],
        "deactivate": [],
        "upgrade": []
    } }
  ]
}
```

**Patch 示例（升级）**
```
{
  "ops": [
    { "op": "set", "path": "/modules", "value": [ ... ] },
    { "op": "set", "path": "/module_changes", "value": {
        "register": [],
        "activate": [],
        "deactivate": [],
        "upgrade": [
          { "module_id": "m.weather", "from_version": "0.1.0", "to_version": "0.2.0", "wasm_hash": "..." }
        ]
    } }
  ]
}
```

## ModuleChangeSet 生命周期（草案）

**提案阶段**
- `module_changes` 仅存在于提案的 manifest 中，用于描述预期模块变更。
- Shadow 阶段完成校验后生成 `ShadowReport`（包含错误/警告/通过项）。

**Apply 阶段**
- apply 时按事件序列写入生命周期事件，并更新注册表索引。
- apply 完成后，活动 manifest 中的 `module_changes` 应清空为 `null`（避免重复应用）。

**回放阶段**
- 注册表由事件流重建；`module_changes` 仅用于审计历史，不作为运行时指令。

## 多补丁冲突处理（草案）

> 目标：在 merge/patch 叠加时，确保模块变更的确定性与可审计性。

**冲突判定**
- 同一 `module_id` 在不同 patch 中出现 `register/upgrade/activate/deactivate` 任一重叠，即视为冲突。
- `register` 与 `upgrade` 对同一 `module_id` 视为硬冲突，必须人工选择。
- `activate` 与 `deactivate` 对同一 `module_id` 视为软冲突，允许选择保留其一。
- `upgrade` 的 `from_version` 不一致视为冲突。

**合并策略**
- 默认 **拒绝自动合并**，返回 `PatchMergeResult.conflicts` 供治理人工裁决。
- 若治理提供显式决策（保留 patch A 或 B），则按决策应用并记录审计事件。
- 合并后的 `ModuleChangeSet` 需重新通过 shadow 校验。

> V1 约定：治理“补丁”采用**完整 manifest 替换**语义（shadow 仅计算候选 manifest 哈希）。

## Manifest 版本与迁移策略（草案）

- `manifest_version`：每份 manifest 显式携带版本号（如 `v1` / `v2`）。
- **向后兼容**：新字段默认值由加载器填充（如 `module_changes = None`）。
- **向前拒绝**：内核遇到高于自身支持的版本时拒绝加载并记录审计事件。
- **迁移路径**：提供 `migrate_manifest(from, to)` 辅助函数；迁移需可确定性重放。
- **Patch 约束**：`ManifestPatch` 必须基于同一 `base_manifest_hash`，跨版本 patch 直接拒绝。

**加载/恢复流程（示意）**
```
load_manifest()
  -> if version == supported: use
  -> else if version < supported: migrate_manifest(version, supported) -> new_manifest
  -> else: reject + audit
```

**base_manifest_hash 行为**
- 迁移后生成新的 `manifest_hash`，用于后续 patch 基线。
- 迁移前后的哈希需记录在审计事件中，保证可追溯。

**迁移审计事件（草案）**
```
ManifestMigrated {
  from_version,
  to_version,
  from_hash,
  to_hash,
  reason,
}
```

## 稳定实例、交易与升级合同

- 活动模块以持久 `instance_id` 区分，同一 module artifact 可在不同 owner/target 下形成独立实例；确定性执行使用稳定实例顺序。
- `ModuleInstallTarget` 只允许 `SelfAgent` 与
  `LocationInfrastructure { location_id }`。target-aware install 及其
  finality 变体携带该字段；legacy install 归一化为 `SelfAgent`。location
  目标必须非空、存在且与 installer 当前 location 一致，失败路径不写状态。
- action、`ModuleInstalled` domain event、pending/release 状态与 installed
  instance 都持久化 `install_target`；历史缺失字段以 `SelfAgent` 为 serde
  默认值。snapshot/replay 不重新选择目标，基础设施实例继续使用已记录的
  infrastructure tick origin。
- deploy/register、install、activate、deactivate、upgrade、list/bid/purchase/delist/destroy 均通过 validated action -> domain event -> state apply 进入权威状态并可重放，不允许直接改 registry 或 instance cache。
- upgrade 保持 module identity，校验 owner、interface version、required exports、subscriptions、capability slots 与 limits；不兼容升级被拒绝且不产生部分事件。
- artifact 市场只允许当前 owner 上架、下架或销毁；结算时校验正价格、价格资源类型与余额，成交后转移 ownership 并清理相关 offer/bid。active instance 仍引用 artifact 时拒绝销毁。
- 费用事件只在成功动作路径产生；旧事件新增字段使用明确兼容默认值，replay 不重新撮合、不重新报价，也不把历史订单视为保证成交。

<a id="module-retirement-and-shared-content"></a>
## 退休、共享内容与持久消费者的一致设计

本节承接 [runtime 保存与恢复合同](../prd.md#module-store-persistence-and-recovery-contract)，补齐 #4417 的退休、升级与共享内容分项。基准 `5e930face77dd284bbba6186fd997e7e63fa9164` 仍在退休时移除 bytes、在保存/恢复时遍历全部 registry；`ModuleStore` 仍只按 H 写 metadata，Observer 逐条比较完整 manifest。这些是待修的不同路径，不从本文推导为已经可用，也不重做已有 direct/trusted command 隔离设计。

### 退休事实与恢复所需材料

1. 退休准入保持 owner、费用、reason、active instance 和 legacy active 引用检查。销毁后的运行状态移除当前 artifact membership/执行 bytes，并按现行合同清理 cache；registry 历史、实例身份和 schedules 保留。恢复不得因历史 registry 尚在就重新启用该模块。
2. 目标持久表示必须保留可验证、绑定该 artifact 和已接受销毁事件的退休事实。writer 与 snapshot/CAS 使用同一判定，将“已退休的历史引用”与“仍需执行的引用”区分。只有前者可以缺当前执行 bytes；仅凭 owner 不存在、bytes map 为空、文件缺失或可变缓存不能推断退休。旧的单条 raw destroyed event 保持 state-only 兼容行为；合法动作退休必须另有下文定义的 journaled fact，不能只增加 snapshot 字段而遗漏 journal writer/replay reader。
3. 在发布可确认的生命周期结果前，候选的 registry、退休事实、可执行 bytes、实例引用和持久闭包必须能由受支持 reader 一致重建；失败则不发布这一成功转换。canonical CAS 不能只序列化成功就接受不可重建的候选。已经持久提交或提交结果未知时，沿既有 generation/commit 核对恢复，不能倒写未提交或删掉旧事实。
4. 保留窗口内的旧 snapshot、历史版本执行及受支持 replay 仍需的工件属于历史闭包，不能与当前执行 bytes 一同 GC；具体存储保留仍由[存储治理](../runtime/runtime-storage-footprint-governance.prd.md)拥有。当前不存在受支持的退休持久表示时，实施必须先补其 reader/writer 与明确兼容策略，不能通过全局忽略 missing bytes 放行。

<a id="module-artifact-retirement-journal"></a>
#### 可重放的动作退休记录（目标）

目标新增版本化 journal 记录 `ModuleArtifactRetirementCommitted`，用于重建成功动作的 sidecar 退休；名称及以下格式义务均为待实现设计，当前 `ModuleArtifactDestroyed` 不能单独承担这一含义。

- 记录至少携带格式版本、`wasm_hash`、该 artifact 本轮合法发布的事件引用和本次 `ModuleArtifactDestroyed` 的事件引用。事件引用使用所属 journal/提交内的既有稳定身份，完整记录受同一持久提交绑定；H 不能单独区分退休后再次发布的生命周期。
- 通过既有 owner、active 引用、费用及 reason 准入后，动作路径在同一 prepared publication 中按固定顺序准备 destroyed event 和退休记录，并将两者、原有业务状态、artifact membership/bytes 的移除与 cache 失效作为一个提交单元。journal/consensus/持久闭包任一步准备失败，均不发布半个单元；不得先宣告销毁成功，再异步补退休记录。退休记录不额外扣费，也不产生第二次玩家业务回执。
- `replay_from` 从已验证 checkpoint 按已提交 journal 顺序读取整个单元：核对发布引用、本次销毁事件及 H 一致后，重建退休事实并移除该生命周期的当前 membership/执行 bytes，执行既有 cache 失效，保留 registry 历史和实例。它不重新运行动作、当前 owner/报价准入或收费；同一记录重复恢复保持幂等，较旧生命周期的记录不能删除后来显式重新发布的 H。
- 销毁后 checkpoint 必须包含已验证的退休事实及其绑定，供 `World::from_snapshot`、目录 reader 与 canonical CAS 在不依赖较新缓存的情况下恢复。销毁前 checkpoint 则保留当时的工件闭包，由成功动作的完整 journal tail 重建同一退休结果；两个入口必须与直接捕获销毁后状态等价，不能互相替代验收。
- 新格式的 reader 只接受完整、相互匹配的已提交单元。未提交或截断 tail、错事件引用/生命周期、冲突重复和未知格式不能产生已退休状态；对声称完整的新格式恢复应明确拒绝。旧格式中的单条 raw destroyed event 仍按原 state-only 合同解释，不凭其存在升级为新格式的动作退休事实，也不以“兼容”忽略未退休 missing bytes。

退休判定属于所捕获版本的生命周期状态，不是对 H 的永久全局黑名单。后续显式、合法的同 hash 重新发布仍遵守现有准入与收费，只有新的已提交生命周期转换才能改变当前可用性；不能借缓存或旧 registry 自动复活，也不能重写退休前后已经固定的历史 snapshot。

### 两种身份与共享 bytes

`wasm_hash=H` 标识一份内容；registry 的 `module_id/version` 及完整 manifest 标识一条不可覆盖的注册记录。两个 manifest 可以指向 H，不能把两个 manifest 当成同一对象。一次捕获的 immutable generation 固定完整 registry/manifest 集及 H 的内容引用，读取顺序不影响恢复结果。

新 writer 以 generation 内受哈希绑定的完整 registry manifest 为元数据真值，WASM bytes 继续按 H 去重；若需要独立 metadata 文件，则按完整 manifest 的内容摘要引用并绑定到同一 generation，不能继续用单个 H 槽覆盖多个 manifest。不能从 module/version 裸字符串拼出未校验路径。新格式必须有明确识别方式；旧格式仍走其既有精确校验，不将不兼容目录静默当新格式读取。

Observer 和普通 reader 先核对所选 generation、全部 manifest 身份与其内容引用，再按同一退休判定取得必需 bytes。inline bytes 只改变取材位置，不绕过元数据/内容校验；all-inactive 也不能省略历史 registry。合法退休只放宽对应历史工件的当前 bytes 要求，不放宽活跃或未退休工件的 hash、identity、权限/兼容检查。Observer 保持只读、no-follow 与现有预算，不能现场补 meta、覆盖源目录或借较新 cache 修补旧 checkpoint。

<a id="instance-upgrade-version-reuse"></a>
### 实例升级与注册复用

正常实例 upgrade 分开准备“确保目标版本已合法注册”和“迁移该实例”。前者在目标不存在时复用现有注册准入；已存在时比较完整 manifest，一致才复用、不重复注册，不一致拒绝。后者从目标实例 ID 读取其当前 module/version/hash、owner、target、state 及兼容要求，`from_version` 与该实例匹配；A 改变全局 active 不能让仍在 v1 的 B 变成错误源版本。

既有目标版本合法性及 owner/artifact/冻结/interface/export/subscription/capability/limits/finality 校验不因复用省略，目标等于本实例当前版本仍拒绝；正常 upgrade 的版本约束以实例源版本判断，rollback 保持独立入口，不新增版本排序算法。`activate` 对全局视图的作用仍受既有规则约束，不能偷偷迁移其他实例。目标注册（若需要）、该实例升级、费用、既有调度变更和业务事件通过现有 prepared publication 一起提交。任何失败不留下半注册、半迁移或升级扣费；已有 proposal/shadow/approval 与 failure audit 的边界保留。

成功结果保留原实例 ID、owner、target 和独立 state，以真实 upgrade 事件记录新版本绑定。不得改用 rollback 审计、额外冻结正常 v1，或通过 install 新实例冒充迁移。snapshot/replay 保留 A/B 各自绑定；恢复后 B 仍可升级到同一 v2。通用 schema 迁移与完整 SR2 交易目标保持下文的独立范围。

<a id="gameplay-mode-key-design"></a>
### Gameplay mode key 的单一派生

承接 [mode 判等合同](../prd.md#gameplay-mode-identity-contract)：在 Gameplay layer 共用 `mode.trim()` 派生逻辑，给 manifest 去重、变更后 `(mode, kind)` 槽检查及 readiness 两边的 mode 比较使用。保留原始 manifest 和其哈希，不在 register/load 时原地修剪字段；查询返回的规范 mode 与原 manifest 展示内容应区分。snapshot/replay 重建同一派生视图；冲突不得以排序后取首项来隐藏。此接线修正不增加新的 wire identity、字符归一化或原始 manifest 迁移。

<a id="module-lifecycle-recovery-validation"></a>
### 独立验证与兼容边界

以下为待执行回归，优先扩展现有 `runtime/tests/persistence.rs`、`runtime/tests/module_action_loop_release_controls_tests.rs`、Observer load 与 chain runtime recovery tests；存在这些测试文件不表示本矩阵已通过。

| 场景 | 输入与检查 | 必须保留的负例/边界 |
| --- | --- | --- |
| 退休后 snapshot | 正常安装 inactive A，并保留 B 的 bytes；合法销毁 A 后直接 snapshot/from_snapshot，历史 A 可查、不可执行、B 正常。 | active 引用销毁拒绝；未退休缺 bytes 拒绝；空 registry fixture 不代签。 |
| 退休后目录与 CAS | 对同一销毁后状态分别 save/load、提交 canonical CAS 后 restart；核对 registry、退休事实、身份/状态及可继续操作的 B。 | 不用销毁前 baseline+tail 或较新 cache 代替；失败准备零部分发布。 |
| 退休前 checkpoint + 成功动作 tail | 安装 inactive A/B 后捕获 checkpoint；合法销毁 A，保存包含 destroyed event 与绑定退休记录的完整提交 tail；新 World 从 checkpoint 经 `replay_from` 恢复，再做 snapshot/from_snapshot、目录与 CAS restart，与直接销毁后状态比较。 | A 历史可查且不可执行，B 正常；费用只出现一次。缺失/截断单元、错发布或销毁引用、冲突重复、未知格式拒绝；同一记录重复恢复及同 H 后续发布不得误删新生命周期。 |
| 原始销毁兼容 | 通过原 raw destroy event 路径重放并比较 sidecar。 | 保留该入口既有 state-only 行为，不擅自删除持久 bytes 或把事件存在等同合法退休事实。 |
| 同 H 后续重新发布 | 捕获退休后的旧 snapshot，再显式合法重新发布同 H 并捕获新 snapshot；分别独立恢复并继续运行。 | 旧 snapshot 的退休状态不被新 cache/registry 改写；只有新已提交生命周期允许当前工件重新可用。 |
| 多实例正常升级 | A/B 在 v1，A 升 v2，snapshot/replay 后 B 再升同 v2，保留各自 ID/owner/target/state 及一次升级费。 | 同实例相同版本、不同 manifest、错 owner/from、冻结、不兼容、无效 finality 拒绝；无 rollback 事件或额外冻结。 |
| 共享 H 的 Observer | 同模块两版本、不同模块分别共享 H；经标准 writer 保存再走真实 Observer，单 active/全 inactive，支持的 inline/non-inline 分别覆盖。 | 错 manifest/hash、坏 bytes、损坏或不支持旧格式拒绝；源目录无新增或改写。 |
| mode 一致性 | 单条 ` sandbox ` 可被 canonical 查询发现；同 manifest 重复与同 kind 跨 manifest 冲突拒绝；不同 kind 覆盖合法，恢复后结果相同。 | 所有生命周期入口一致；raw manifest/hash 不变；不增加大小写归一化。 |

<a id="revoked-grant-discovery"></a>
### 撤销历史与当前能力目录

目录遵守 [WASM capability 合同](../wasm/wasm-interface.md#3-capability权限和-kernel-边界)，保留持久 grant 历史，由已验证的当前 authority/revocation 状态排除明确 revoked/superseded 的条目，再投影其余有效声明与授权。合法撤销 A 不应阻断独立有效 B；损坏签名、错误 authority/proof 或不可解释冲突仍拒绝，不能改成吞掉所有验证错误。执行时仍重验 live revocation，旧目录不能重新授权 A。

针对 #4502，待执行回归必须先成功注册 A/B，再合法撤销 A，然后真实调用目录派生及 B 的调用上下文生成：A 不可发现/执行，B 仍可发现；受支持 snapshot round-trip 和 journal replay 后同义。先撤销 A 再尝试注册 A 的 fixture 不覆盖此场景。该投影修复不等于生产 Agent E2E、command 实例隔离或全体能力迁移验收。

文档 PR 只证明这些设计边界已写明；退休 P1 先行，其余分项可独立实现验收。旧格式有缺失或不可证明退休事实时，应明确拒绝或采用另有验证的迁移，不把“缺 bytes”当成默认兼容值。

## SR2 目标合同：运行时生命周期的十二视图

本节 owner 为 runtime_engineer；审读基线为 `9c41d57b4436f71f0cf9b481043d882cfdc551ed`，2026-09-26 文档作者审读；独立 review 尚未完成。原文的当前义务和草案资格保持有效。以下为实际目标设计，未实现，不修改当前 wasm-1、市场规则、费率或上述 governed proposal prepared seam 的 partial 边界。本次文档变更采纳的是目标文档合同，不是运行能力或 proof 状态。

## 1. 问题、目标与非目标
跨版本 pending、迁移与外部效果必须共享 canonical authority，避免本地 accepted、加载成功或 provider ACK 被误算成世界效果。目标是可验证的一次成功执行与确定恢复；不承诺 provider 本身 exactly-once，也不把 manifest FORMAT migration 当作状态迁移。

## 2. 上游约束与相关角色
产品拥有 pending/服务等级语义；P2P 拥有 canonical branch、证书算法和 finality；WASM 拥有字段编码、迁移 ABI；QA 拥有组合验收；runtime 拥有决策、事务与 replay。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [保存/实例合同](../prd.md#module-store-persistence-and-recovery-contract) | 退休可恢复、多实例升级、共享内容各自成立 | [生命周期闭包](#module-retirement-and-shared-content) | Runtime/WASM store/Observer 与 chain recovery | 文档不证明格式迁移或实现完成 |
| [mode 判等合同](../prd.md#gameplay-mode-identity-contract) | 校验、active slots、readiness 与恢复同义 | [mode key](#gameplay-mode-key-design) | Gameplay layer/runtime | 不改原 manifest/hash 或新增 wire |
| [req-dwe-003](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-003) | exclusive winner / no collateral termination | [sr2-pending-contract](module-lifecycle.md#sr2-pending-contract) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | current Agent chat superseded 不证明 general lineage |
| [sr2-migration-contract](../wasm/wasm-interface.md#sr2-migration-contract) | all objects atomic, old bytes replay retained | [sr2-migration-publication](module-lifecycle.md#sr2-migration-publication) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | 完整 migration 未执行 |
| [sr2-release-acceptance](online-module-release-legality-closure-2026-03-08.prd.md#sr2-release-acceptance) | canonical inclusive version boundary and historical schedule | [sr2-activation-contract](module-lifecycle.md#sr2-activation-contract) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | prototype finality 非 BFT |
| [4-technical-specifications](online-module-release-legality-closure-2026-03-08.prd.md#4-technical-specifications) | provider proof versus world finality and idempotency | [sr2-external-contract](module-lifecycle.md#sr2-external-contract) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | 无 provider exactly-once / production full proof |

## 3. 当前状态、目标状态与差距
当前上述 typed prepared proposal 仅保证其限定入口的不可失败安装和固定事件序列；稳定 instance/install_target/市场状态与历史 serde defaults 保持原合同。目标 PendingIntentV1、IntentDecisionV1、VersionActivationV1、ExternalEffectReceiptV1、ExternalReceiptVerificationV1、SchemaMigrationV1、ModuleStateMigratedV1 尚无完整实现证明；本节不可使 absent Rust types 变成 current。

## 4. 边界与结构
WASM 输出仅是 intent。Runtime 从不可变世界基线选择 activation、验证资源和权利、准备 registry/state/resources/journal/receipt；P2P 验证包含证明；持久层发布 committed transaction；consumer 读取权威 decision。所有原始请求、历史 artifact、schema、schedule、activation 与 receipt 字节是 replay owner 的不可变输入，缓存不是 authority。

## 5. 关键运行流程
<a id="sr2-pending-contract"></a>
### PendingIntentV1 与 IntentDecisionV1：目标单一字段 authority
共享类型与 canonical profile 引用 [interface types](../wasm/wasm-interface.md#sr2-types)。字段均 required，只有带 `?` 的字段可省略；禁止 null、unknown property/version/enum。
`PendingIntentV1={schema_version:"PendingIntentV1",scope:WorldScopeV1,intent_id:Id,request_hash:Hash32,payload_hash:Hash32,subject_id:Id,target_hash:Hash32,lineage_id:Id,member_generation:U64,relation:original|linked_replacement|withdrawal|independent,exclusive_set_id?:Id,predecessor_intent_id?:Id,predecessor_request_hash?:Hash32,compatibility_declaration_hash:Hash32,accepted_position:JournalPositionV1,expiry_boundary?:U64,disposition:pending}`。
`IntentDecisionV1={schema_version:"IntentDecisionV1",scope:WorldScopeV1,intent_id:Id,request_hash:Hash32,lineage_id:Id,member_generation:U64,disposition:committed_effect|rejected|expired|replan_required|terminated_without_effect,reason_code?:Id,decision_position:JournalPositionV1,execution_receipt_ref?:ContentRefV1,winner_intent_id?:Id,winner_receipt_ref?:ContentRefV1}`。
Pending authenticated body 从不附加选定 version、winner 或 receipt；终态在 durable decision index 中另存。Admission 校验 world/subject/target、显式授权 exclusive set、exact predecessor ID/hash 与 checked successor member_generation；dangling/cycle/gap/overflow/conflicting bytes 拒绝。相同 world/subject/intent/request_hash 重试返回原状态；相同 key 改字节冲突。Generation 不是 priority、capability generation 或 key epoch。
PendingRequestCoreV1 是 PendingIntentV1 删除 request_hash/accepted_position/disposition 后全部其余 present fields 的精确投影，schema_version 保留 PendingIntentV1；不是新增 current wire。member_generation 必须先由 runtime assignment/validation，再计算 accepted request_hash=H("oasis7/intent-request/v1",PendingRequestCoreV1)，intent_id 保留；这个 admitted commitment 不替代 caller authentication，不让 caller 自授 generation/authority。accepted_position 是 admission journal provenance，不进入 request hash。
IntentDecisionCoreV1 是 IntentDecisionV1 全部 present fields 的 canonical 投影，schema_version 保留；没有 selfdigest/signature 字段。execution_receipt_ref/winner_receipt_ref 的 ContentRef 只引用 block-independent ModuleExecutionReceiptCoreV1 canonical bytes，digest=SHA256(CanonicalCBOR(receipt_core))、byte_length=这些 exact bytes 长度，locator 排除；不能引用同一 containing block 的 finalized wrapper bytes。Receiver fetch/verify core bytes 后单独计算 domain-separated receipt_id，读取 detached finalized wrapper 并验证 inclusion/receipt_id/execution_id/activation 匹配。decision commitment=H("oasis7/intent-decision/v1",IntentDecisionCoreV1)，因此同 block winner/loser decision 不通过 finality wrapper 形成递归哈希。Raw content digest 与 receipt_id 是不同值，不得互换。
canonical order 的第一个成功有效 receipt 事务同时发布 winner 与 explicit exclusive losers；独立 intents 不受影响。committed_effect 必须有 execution_receipt_ref；其他 dispositions 禁止该 ref 且必须 reason_code；exclusive loser 必须 winner ID/ref。reject/expire/replan 只结束自身。无效果 withdrawal 为 terminated_without_effect，不是 winner；其 predecessor linkage 保留在原 pending 请求。Finality outage 只能保留 pending/local audit，不能伪造 finalized rejection。恢复重新检查 expiry、权限、资源、target 与新 governing manifest；不延期、不继承旧 quote，不改 payload。

## 6. 接口与数据合同
<a id="sr2-activation-contract"></a>
### VersionActivationV1：目标单一字段 authority
`VersionActivationV1={schema_version:"VersionActivationV1",scope:WorldScopeV1,activation_id:Hash32,proposal_id:Id,prior_activation_id?:Hash32,base_manifest_hash:Hash32,new_manifest_hash:Hash32,from_execution_version:Id,to_execution_version:Id,effective_canonical_height:U64,metering_schedule_version:Id,metering_schedule_hash:Hash32,artifact_identities:[{module_id:Id,module_version:Id,wasm_hash:Hash32,artifact_identity_hash:Hash32}],compatibility_declaration_hash:Hash32,migrations:[SchemaMigrationV1],governance_receipt_ref:ContentRefV1,finality_binding:{canonical_block_hash:Hash32,canonical_block_height:U64,certificate:ContentRefV1}}`。
Core 是上述对象删除 activation_id/finality_binding 后的精确字段集；`activation_id=H("oasis7/version-activation/v1",core)`，证书 detached 验证 exact core 和边界，不能递归 hash containing block。artifacts 按完整 module/version/hash tuple 排序去重；migrations 按 object_kind/namespace/name/instance_id/entity_id 排序去重。first finality-verified canonical inclusion 的 height 在 inclusive boundary 上选择新版本；submission time、candidate block、安装和 compatibility 声明不能选择版本。Missing/conflicting boundary、未知版本、不可达旧 artifact 或证书不符均 fail closed。

<a id="sr2-external-contract"></a>
### ExternalEffectReceiptV1 与 ExternalReceiptVerificationV1：目标单一字段 authority
`ExternalEffectReceiptV1={schema_version:"ExternalEffectReceiptV1",scope:WorldScopeV1,receipt_id:Hash32,effect_intent_id:Id,request_hash:Hash32,parent_execution_receipt_id:Hash32,provider_id:Id,provider_key_epoch:U64,status:succeeded|failed,result_payload_hash:Hash32,request_payload_hash:Hash32,response_payload_hash:Hash32,provider_proof:ContentRefV1,canonical_inclusion?:{canonical_block_hash:Hash32,canonical_block_height:U64,journal_position:JournalPositionV1,receipts_root:Hash32,authority_snapshot_hash:Hash32,finality_proof:ContentRefV1}}`。
`ExternalReceiptVerificationV1={receipt_id:Hash32,verdict:unverified|verified_canonical|reorged|suspended,reason_code?:Id}` 是有界验证投影，继承 exact receipt scope，不重写 immutable provider result。
receipt core 删除 receipt_id/canonical_inclusion；`receipt_id=H("oasis7/external-effect-receipt/v1",core)`。Raw provider receipt 无 inclusion，不算 committed。Provider succeeded/failed 和验证状态是两条轴；failed 不制造 module committed_success。验证次序见 [release trust](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust)。重复 exact bytes 返回原 decision；同 ID 不同 bytes、unknown intent、错 world/request/provider/key、proof/hash/root/branch 不符拒绝；outbox 重试保持 effect_intent_id，provider 不幂等时不能保证现实世界 exactly-once。当前 local EffectReceipt 和 receipt_leaf_hash 不证明此新 trust wire。

## 7. 状态、事务与持久化
<a id="sr2-migration-publication"></a>
### SchemaMigrationV1 / ModuleStateMigratedV1 的原子发布
字段 authority 为 [interface migration](../wasm/wasm-interface.md#sr2-migration-contract)，本节拥有 publication/replay 语义。每个 object/namespace/name/instance/entity 匹配 exact from/to version+schema hash，PositiveU64 不同且精确声明，numeric ordering 不授予迁移权。Preflight immutable base 的旧 artifact/state、权限、manifest 与 bound；无 I/O sandbox 生成 bounded canonical output，只可写目标 namespace。Event adapter 不修改原 caller/time/cause/kernel identity。
所有对象先 staged，包括 registry、instance bytes、active manifest、schedule、cache invalidation、state/resources、journal/event era、migration event 与 receipt；任一步失败保留全部原 bytes/root/fees/artifacts/events。成功单一不可失败安装；durable commit 后才公开 world effect，crash after reservation/before commit 回到 no effect 或完整 transaction，after durable commit 返回原 receipt，禁止重跑 committed WASM。
ModuleStateMigratedCoreV1 是 ModuleStateMigratedV1 exact field projection，只删除 canonical_block_hash/canonical_block_height，schema_version 仍为 ModuleStateMigratedV1；没有新 event wire 或 own ID 字段；`H("oasis7/module-state-migrated/v1",core)` commits exact schema migration、scope、from/to state hash、base/governing manifest、activation、caused_by、journal position；detached canonical wrapper 不递归输入 containing block。Intent request/decision 分别使用 `oasis7/intent-request/v1`、`oasis7/intent-decision/v1` domain，删除 own digest 与签名/locators；Hash32/ContentRef 的 canonical 编码按 interface 唯一 profile。

## 8. 部署、安全与运行约束
Production 不能从本地 unsigned/self-signed receipt、CI artifact 或 health green 推断 finality。Unknown proof scheme、旧 key 超 grace window、无法读取 proof 均阻断；P2P 验证 snapshot/branch/certificate，runtime 不自造 BFT。旧 bytes、artifact/schedule retention 的 GC authority 是 runtime persistence；未定义完整 retention budget 之前不得清除 replay 所需对象。

## 9. 质量与容量
环境为同一候选/world/window 的 required deterministic fault fixtures；输入为两对象 migration 和每个 prepare 步注入失败；指标是 before/after bytes、roots、journal、资源、receipt 完全相等或一个完整 commit，不能是部分。预算继承原内存/fuel/output/call-rate 上限，不增费率；full 增加实际多节点恢复和 consumer/proof readback，未运行。

## 10. 兼容、迁移与回滚
原 wasm-1 optional/default fixtures、SelfAgent 默认与 infrastructure tick origin 保持。Replay 在已记录 activation boundary 以前使用原 artifact/schema/schedule；边界后读取 exact migration event，不重写旧事件或按当前价格重算历史 charges。缺失历史依赖停止恢复；治理 rollback 是新 activation，不撤销已经 committed 的现实效果。ManifestMigrated 格式迁移仍与 component/event migration 分离。

## 11. 验证设计与可追溯性
所有场景要求 candidate source/integration commit readback + tested tree readback，scope/window 精确相等；located tests 未在本次运行。完整输入、断言、tier 分配由 [GWSC runtime matrix](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios) 接收，不能用 future Issue 替代。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [保存/实例合同](../prd.md#module-store-persistence-and-recovery-contract) | [生命周期闭包](#module-retirement-and-shared-content) | 退休、升级、共享 H 分别验证 | [本页独立矩阵](#module-lifecycle-recovery-validation)；使用真实安装、两实例及标准 writer/Observer/CAS，不以空 registry 或普通 inline round-trip 替代 | 实施 PR 的实际命令、候选、状态/费用/目录对照 | 本文未运行；不代签完整 SR2、跨主机或新 wire |
| [mode 判等合同](../prd.md#gameplay-mode-identity-contract) | [mode key](#gameplay-mode-key-design) | lifecycle/readiness/restore 使用相同派生 key | [mode 一致性矩阵](#module-lifecycle-recovery-validation)；分别覆盖 trim、重复、同槽冲突、不同 kind 与恢复 | 实施 PR 的原 manifest/hash 与查询/拒绝结果 | 当前仅目标设计，无业务回归结果 |
| [req-dwe-003](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-003) | [sr2-pending-contract](module-lifecycle.md#sr2-pending-contract) | exclusive winner / no collateral termination | SR2-RT-02/03; LIN-01..04; required both canonical orders/same-block tie/duplicate/restart; full actual consumer readback；[located partial source](../../../crates/oasis7_runtime/src/tests/module_action_loop_release_controls_tests.rs) | candidate-bound artifact + GWSC matrix | current Agent chat superseded 不证明 general lineage |
| [sr2-migration-contract](../wasm/wasm-interface.md#sr2-migration-contract) | [sr2-migration-publication](module-lifecycle.md#sr2-migration-publication) | all objects atomic, old bytes replay retained | SR2-RT-07/08; MIG-01..03; required bad second object/output/context/post-prepare injections; module_lifecycle.rs shadow_failure_blocks_apply/replay_preserves_module_events 是 partial locator；[located partial source](../../../crates/oasis7/tests/module_lifecycle.rs) | Task evidence state/journal/charge/receipt equality | 完整 migration 未执行 |
| [sr2-release-acceptance](online-module-release-legality-closure-2026-03-08.prd.md#sr2-release-acceptance) | [sr2-activation-contract](module-lifecycle.md#sr2-activation-contract) | canonical inclusive version boundary and historical schedule | SR2-RT-04/05; ACT-01/MET-01; before/at/after boundary, old-submit/new-finalize, missing activation/artifact; full canonical proof replay；[located partial source](../../../testing-manual.md) | candidate-bound activation and replay artifact | prototype finality 非 BFT |
| [4-technical-specifications](online-module-release-legality-closure-2026-03-08.prd.md#4-technical-specifications) | [sr2-external-contract](module-lifecycle.md#sr2-external-contract) | provider proof versus world finality and idempotency | SR2-RT-06; RCP-01/02; required forged/conflicting/orphan/unavailable proof + restart duplicate; effect_publication_transaction_regressions.rs 是 partial locator；[located partial source](../../../crates/oasis7/tests/module_lifecycle.rs) | Task proof bytes/trust readback | 无 provider exactly-once / production full proof |

## 12. 决策、长期风险与未决问题
选择 immutable pending + durable decisions，避免修改 authenticated request；选择 core/detached proof 防 hash recursion。旧分析 ManifestActivationV1/generation/committed_rejection 等候选名称在当前作者报告中保留为历史替代，被 R0-R5 exact contract 取代。解除 production proof 缺口需 runtime/WASM/P2P 实现与 QA 同候选 full evidence，触发条件是具体实现合同或 proof receiver 改变；本次不激活能力。
