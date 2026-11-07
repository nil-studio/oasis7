# Agent 机体、接口与默认能力核验手册

- 要求：[原专业目标与 AC-ADM-01～09](agent-default-modules.prd.md#agent-default-modules-acceptance)。
- 设计：[默认模块、机体接口与实体 cargo](agent-default-modules.design.md#1-设计定位)。
- 产品关联：[默认机体与能力框架组合验收](../../product/agents-world-simulation/prd.md#ac-agent-default-001)。
- 记录位置：实际 Task/PR 的命令输出、来源与前后态对照；本文件只保存方法，不保存可变进度或宣称运行通过。

<a id="adm-manual-scope"></a>
## 1. 适用范围、候选与来源前置

ADM-M-01 是只读源核对；ADM-M-02 给出现有自动回归入口；ADM-M-03～07 分别核对扩容、实体存取、挂载、七能力及恢复。只有文档/设计时可以执行只读核对与参数手算，不能记为游戏行为验收。未来目标场景必须先定位实现候选的真实消费者及受控环境，再按本手册执行；缺入口、授权、来源或参数时准确记录该项尚不可执行，不用内部 helper 补齐正式链路。

开始前在原 Task/PR 记录 source/build、实际模块版本与工件、启用 features、机体/能力配置、核验范围和环境。涉及世界操作时另保存真实 world/Agent/主体/owner、原请求与观察位置；不把示意 ID 或测试 hash 填成生产证据。文档编辑与游戏调用分开记录，列出测试名不表示运行了测试。

| 必备输入 | 核对方法 | 未满足时的边界 |
| --- | --- | --- |
| 同一候选与配置 | 从实际树读取 [bootstrap](../../../crates/oasis7_runtime/src/world/bootstrap_power.rs)、[值模型](../../../crates/oasis7_world_state/src/agent_state.rs)、实际 manifest/instance/配置 | 不以另一 worktree 或文档目标替代运行版本；缺工件时不宣称模块可运行 |
| 接口/模块/矿物实体来源 | 正式场景须从该世界的合法入账结果与当前容器核对种类、数量、尺寸、来源和归属；隔离回归明确标注 fixture 建立位置 | 当前测试中的 `add_agent_cargo_entity` 无入账事件，不证明来源机制、制造、交易或初赠 |
| 目标消费者 | 查实际 catalog、API/command 声明、路由、主体与授权重验，以及权威结果读取；存/取/列举必须分别定位 | `store_entity` 等是 PRD 能力名，不是当前可直接执行的命令；无消费者就记录缺口 |
| 有限参数 | 用设计已选择的 `adm-capabilities-v1` 与三种机体组合核对实际配置 hash、模块/状态版本及有限参数；测试专用覆盖值单独标记 | 设计初值尚未平衡或实测；独立 fixture 值不替换生产配置，旧工件不当作新算法消费者 |
| 故障与恢复入口 | 使用候选已有受控测试 seam 或获准隔离环境，固定操作前 checkpoint、完整 tail 与操作后 snapshot | 不破坏公开环境；缺故障 seam 或持久恢复消费者则保留未验证，不能改称不适用 |

正式接口道具获得机制仍是原 ADM-5 后续范围。测试 fixture 可以证明有前置库存时的消费/恢复，不得被写成玩家已能取得该库存。当前默认 Agent 的 cargo 为空，不能把测试主动加的物品描述成出厂赠送。

<a id="adm-manual-baseline"></a>
## 2. ADM-M-01：源码、安装集合与真实能力核对

按下表逐行读当前候选的直接来源，写下实际差异；固定历史树可用 `git show <实际 commit>:<路径>`，路径必须是该树真实文件。核验者先从源码或当前配置得出预期，再与设计比较。

| 读取来源 | 独立核对点 |
| --- | --- |
| [bootstrap_power.rs](../../../crates/oasis7_runtime/src/world/bootstrap_power.rs) | 三个 helper 分别安装 2、4、power-first 可选 4；false 不卸载已有包；已注册同版本不重复 register，停用只恢复 activate；两次 helper 不是跨包原子事务 |
| [agent_state.rs](../../../crates/oasis7_world_state/src/agent_state.rs) | 当前 standard 7 槽、六个绑定字符串与一个空 Universal 槽，`cargo_entries` 空；不存在由字符串自证 active/owner 的字段 |
| [body.rs](../../../crates/oasis7_runtime/src/world/body.rs) 与 [core reducer](../../../crates/oasis7_world_state/src/state/core_policy_transition.rs) | helper 的直接写入与饱和合并；扩容 trim、128 上限、库存类型/数量、成功事件的容量/层级/槽 ID；reducer 的消费与新槽准备、拒绝的 `last_active` |
| [body core](../../../crates/oasis7_builtin_wasm_modules/m1_body_core/src/lib.rs) | BodyAction 属性变化与 Electricity 10 的规则成本；不把这个成本或动作当接口扩容 |
| [radiation](../../../crates/oasis7_builtin_wasm_modules/m1_power_radiation_harvest/src/lib.rs) 与 [storage](../../../crates/oasis7_builtin_wasm_modules/m1_power_storage/src/lib.rs) | 当前按 action 调用补能、位置轴向和超过 800,000 cm 时增加 bonus；有限电池上限 12、首次注册 6、普通私有移动每取整公里 3。新逻辑 tick/热/距离衰减算法只在明确新候选中验证，不把旧输出代入新算例 |
| [cargo](../../../crates/oasis7_builtin_wasm_modules/m1_storage_cargo/src/lib.rs) | 只消费扩容/拒绝事件，私有计数/层级变化；没有完整货物列表、容量、存取 effects 或 command 声明 |
| [sensor](../../../crates/oasis7_builtin_wasm_modules/m1_sensor_basic/src/lib.rs)、[mobility](../../../crates/oasis7_builtin_wasm_modules/m1_mobility_basic/src/lib.rs)、[memory](../../../crates/oasis7_builtin_wasm_modules/m1_memory_core/src/lib.rs) | 当前分别为附近 Agent observation、零距离/距离费用规则、最多 256 条 `time/kind/agent_id` 标签；完整目标的其他义务不得由同名推出 |
| [实例路由](../../../crates/oasis7_runtime/src/world/module_runtime.rs) 与 [capability catalog](../../../crates/oasis7_runtime/src/world/capability_catalog.rs) | 精确 active 实例/target 与声明/当前 grant；槽位标签、全局 active、有效目录和最终调用分开判断 |
| [测试注册](../../../crates/oasis7_runtime/src/tests/mod.rs) | `power_bootstrap`、`agent_default_modules` 在 `test_tier_full` 下才注册；测试名称与实际执行层对应 |

把七类目标逐项列为“实际消费者已定位/只见局部实现/尚缺目标消费者”，并写直接来源。不要只用一次全文检索或同名文件作结论。检查旧文档或当前 UI 若将“已安装默认包”直接写成“全部能力已可用”，定位具体误读并修正其消费者。

## 3. ADM-M-02：现有自动回归入口及证明边界

在候选仓库根、按原任务获准的本地构建环境执行以下 canonical 命令。命令沿默认 Wasmtime feature；不得禁用 feature 后用零匹配或跳过结果冒充通过。保留实际测试计数、失败与过滤情况；本手册列出命令本身不构成运行记录。

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --features test_tier_required --lib tests::body::expand_body_interface -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --features test_tier_required --lib tests::body::expanded_body_state_persists_after_restore -- --exact --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --features test_tier_required --lib tests::core_policy_publication_transaction_regressions::all_eleven_raw_events_use_typed_postprepare_and_retry_replay -- --exact --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --features test_tier_full --lib tests::power_bootstrap:: -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --features test_tier_full --lib tests::agent_default_modules:: -- --nocapture
```

| 测试源 / 精确用例 | 实际前置与能证明的局部结果 | 不能代签 |
| --- | --- | --- |
| [body.rs](../../../crates/oasis7_runtime/src/tests/body.rs)：`expand_body_interface_consumes_item_and_adds_slot`、`expand_body_interface_rejects_when_item_missing`、`expand_body_interface_rejects_missing_agent` | 直接注册 Agent、helper 注入接口实体后正常消费；缺件与缺 Agent 分别拒绝 | 公开鉴权、正式获得途径、满载/溢出及全部错误状态 |
| 同源 `expanded_body_state_persists_after_restore` | 扩容成功后捕获 snapshot，再 from_snapshot，保留 8 槽/层级 1/物品耗尽 | 操作前 checkpoint 加 tail、原始来源入账或跨故障域持久化 |
| [core publication](../../../crates/oasis7_runtime/src/tests/core_policy_publication_transaction_regressions.rs)：`all_eleven_raw_events_use_typed_postprepare_and_retry_replay` | 包含扩容/拒绝 raw event fixture；post-prepare 失败前后状态不变，成功重试并从已含源库存的前态 snapshot 加 tail 比较状态/root | 完整 action、模块后处理、公开请求去重或无道具世界的来源回放 |
| [power_bootstrap.rs](../../../crates/oasis7_runtime/src/tests/power_bootstrap.rs)：两个 `install_*_registers_and_activates`、三个 `install_*_is_idempotent`、`install_power_bootstrap_modules_reactivates_registered_version`、`install_scenario_bootstrap_modules_supports_default_package_toggle` | full 层当前安装集合、重复调用和 power 停用恢复、toggle | 默认四模块逐个停用恢复、power-first 事件顺序/中途失败、完整能力与槽位绑定仍另验 |
| [agent_default_modules.rs](../../../crates/oasis7_runtime/src/tests/agent_default_modules.rs)：`default_sensor_module_emits_observation`、`default_mobility_module_rejects_zero_distance_move`、`default_memory_module_records_domain_events`、`default_storage_cargo_module_tracks_expand_events` | full 层上述各局部行为与 cargo 私有统计 | 完整感知对象、移动模式、长期记忆、实体存取或来源 |
| 同源 `scenario_modules_limit_mobility_before_sensor_when_power_low`、`scenario_modules_replay_keeps_state_consistent`、`scenario_modules_with_transfer_and_body_keep_wasm_closed_loop_consistent` | 低电拒绝移动后仍可查询感知；组合 fixture；restore 用动作后的 snapshot | 原 PRD 全部建议降级顺序、完整前态 replay、正式玩家闭环 |

文档变更只需执行适用文档检查和人工关系核对。若交付开始改变正式 Agent 游戏行为，按[开发自测范围](../../engineering/workflow/source-of-truth.md#agent-gameplay-selftest)另组织适用自主试玩；普通局部测试不替代该验收，也不因本手册存在启动付费模型或公开世界操作。

<a id="adm-manual-expansion"></a>
## 4. ADM-M-03：扩容正常、拒绝与原结果读取

第一轮可直接核现有 body fixture。目标公开路径另需真实鉴权/读取消费者；缺少时只保留内部动作结论。

1. 固定同一 Agent 的 standard 前态：容量 7、层级 0、七个唯一槽 ID，slot-7 为空；核对来源中一个接口条目数量 2、尺寸 1。隔离 fixture 明确在 checkpoint 中保存注入后的前态，不伪称有入账事件。
2. 用该候选真实支持的入口提交一次扩容，保存原输入、主体、原请求与 action/event。不得为了通过正式路径而直接补 map 或以测试 API 自授权限。
3. 从权威状态独立核对容量 8、层级 1、一个新增未锁空 Universal 槽、原六个绑定未变、接口条目数量 1；cargo 容量不增加、有效能力目录不因空槽自动增加。core 结果是 oracle，WASM 消费计数只能旁证。
4. 从相同前态分别执行下表负例；每次保存前后 cargo/槽位以及真实拒绝。拒绝 audit/`last_active` 和独立模块执行费用按原合同记录，不把它们与业务库存“不变”混淆。

| 输入或变化 | 独立预期 |
| --- | --- |
| 缺 Agent | `ActionRejected` 的 AgentNotFound；不存在被补注册或新建的机体 |
| 空/全空白 ID、缺件、数量耗尽或同 ID 错类别 | 无消耗、无新槽；空白/缺件沿当前拒绝语义，不用 Mineral/ModuleItem 替代接口实体 |
| 容量 128；另在目标入口测试容量/槽数/层级畸形或整数边界 | 满额拒绝；畸形前态不饱和成成功，保留原可读事实并阻止受影响变更。后者需目标验证，不能声称当前代码全部覆盖 |
| 预览后最后一个物品被另一合法请求消费 | 本次重验拒绝；两次请求不能各获得一个槽位 |
| 提交前授权撤销、owner/目标绑定变化或 cargo 权限失效 | 真实外部入口重验并拒绝，不把旧目录/旧预览当授权；内部裸 action 不能代该案例 |
| 同一原请求响应丢失后重连 | 核对原提交，不用新 ActionId 重发；只出现一次消费和扩容 |

再用数量 1 的合法前态执行成功，核对条目耗尽后移除、占用减少 1 且不额外加槽。把合法新请求与同一原请求读取区分；前者若无新库存应拒绝，不能因 payload 相同而误判为幂等成功。

<a id="adm-manual-cargo"></a>
## 5. ADM-M-04：实体存取、容量与并发

这组方法针对完整目标候选。先定位真实源实体、目的 cargo、命令声明和权威结果读取；当前只有 `cargo_entries` helper 或扩容统计时，记录“完整存/取/列表消费者未具备”。下列数量是独立算例和受控 fixture 参数，不是游戏平衡参数、初赠或正式来源规则。

设置隔离前态：采用 standard/full 的容量 `C=64`，合法电池投影已追平且处于D0～D3、各依赖可用；cargo 内 Mineral A 为 `28×2`、ModuleItem B 为 `1×3`、InterfaceModuleItem C 为 `2×1`，所以 `U=61`。合法源容器另持有同批 A 数量3和C数量1；合法目的容器能接收B，三容器权威位置相同、当前授权允许交接。ID、归属、来源与配置在fixture中明确，生产验证使用真实已存在实体，不能以算例创建资产。

由核验者先手算以下预期，再依次通过实际存/取消费者操作；每步查同一提交下源、cargo、目的与消耗事实，不能只看列表 UI。

| 顺序 / 操作 | 源或目的预期 | cargo 预期 |
| --- | --- | --- |
| 列举初始库存 | 无变更 | A=28、B=1、C=2；`U=61`、容量64 |
| 存入 A 的 1 个单位 | 源 A 从 3 到 2 | A=29；`U=63` |
| 存入 C 的 1 个单位 | 源 C 从 1 到 0 | C=3；`U=64`，恰满成功 |
| 再存入 A 的 1 个单位 | 源 A 仍为 2，拒绝容量不足 | `U=64`、所有条目不变；不能部分接纳或截量 |
| 取出 B 的 1 个单位到指定目的容器 | 目的 B 增加 1 | B 耗尽移除；`U=61` |
| 用该 cargo 的 C 扩容一次 | 消耗事实增加 1，其他源/目的不变 | C=2；`U=60`、容量仍64，槽数仅增加1 |

最后独立核算：A的源加cargo总量仍31；B的cargo加目的总量仍1；C的源加cargo加本次已消费量为3。原数值资源账本不得无规则地增加同等物料。列举的分页并集应与完整权威库存一致，不重复、不漏项；同一观察版本的排序一致。

从初始 fixture 分别追加以下失败场景，不能将上一次失败污染后态当下一次的合法起点：

- 数量 0/负数、尺寸 0/负数、`q×s` 或合并/总和越出整数范围：精确拒绝，两端无部分数量变化。大整数用有界字段 fixture 检查，不创建相同数量的真实对象或超量内存。
- 相同 ID 但种类/尺寸/来源不同，或试图用 metadata 改 owner/尺寸：拒绝异义合并/越权输入；普通说明 metadata 不改变权威数量或占用。
- 源数量不足、目的不存在/无容量/无权、超出允许交接范围、对象已挂载/消费：分别拒绝，不隐式丢弃、出售或远程移转。
- 两个请求争用源的最后 1 个单位，或两个各需 2 单位空间而仅剩 2：至多一个成功；失败者重验后读到真实剩余，不复制预览额度。
- 预览后容量配置或授权变化：提交按当前合法配置/权限判断，若拒绝则不扣源；不沿用旧预览绕过校验。
- 源扣除准备后、cargo 发布前的受控失败：所有相关业务库存与结果保持前态；提交结果不确定时按 ADM-M-07 查原请求，不立即再次存入。

记录实际入口/schema、source/cargo/destination 三方同版本状态、手算、拒绝原因和事件。未有正式来源只可形成隔离 fixture 的容量/事务证据，不能把这些步骤写成玩家获得方式。

<a id="adm-manual-binding"></a>
## 6. ADM-M-05：机体、挂载与安装生命周期

目标绑定验证使用合法配置和精确模块/实例；仅有旧字符串占位时先报告缺绑定消费者。每项都从固定前态独立执行，并核对 inventory、槽位、实例、registry/active 和模块私有状态。

| 场景 | 核验步骤与独立预期 |
| --- | --- |
| standard、light、heavy 配置 | 按设计读取 standard/full 的 7 槽/六附件/一空槽/cargo64，heavy/full 的 10 槽/六附件/四空槽/cargo128，light/without-memory 的 5 槽/五附件/无空槽/cargo32；前两者分别验证七能力，light 必须明确缺 memory，不能作为完整默认包通过 |
| 五槽冲突与后续合法扩展 | 向 light 放六独立附件时整体拒绝，槽数、绑定、实体不变；在明确隔离 fixture 或真实合法来源下先消费接口得到第六空槽，再合法挂载 memory，才可据实际绑定声明当前七项齐备。不回写初始五槽，也不把消费规则当道具获得方法 |
| 类型、锁定与冲突 | 合法空兼容槽绑定成功；错类型、已锁、已占用、重复槽 ID、错 Agent/owner、错 instance/工件分别拒绝，不能只改槽或只扣物 |
| 默认绑定与实体挂载 | 默认配置记录不自动生成可取出的 ModuleItem；已有实体挂载时比较 cargo 减少与精确绑定同一提交，卸载只在合法同一实体可返还且容量足够时成功 |
| 停用与恢复 | 通过现有治理停用后核对能力不可执行而槽位/原实体/历史保留，再按合法恢复入口重验并恢复；不重赠物品、不清空状态、不重复注册 |
| 升级、回退、降容量 | 绑定指向正确实例与版本；不兼容升级/不够容纳当前库存的降容量拒绝，前态保留。恢复旧模块不会撤销已完成扩容或复制实体 |
| 配置切换 | 对现有standard请求改为heavy/light均拒绝：本轮没有跨frame入口，不能由改名免费加槽、加cargo或删能力。另验同frame合法绑定/版本迁移：精确兼容时保留原槽、扩展层级、库存和实例状态；旧standard库存U65切入目标cargo64时拒绝，不裁掉1单位。单纯重装不重置容量、充电或制造返还模块实体 |
| scenario 顺序及部分成功 | 从空 registry 验 power 先完成；在获准 fixture 使后续四模块加载/安装失败，核对 power 成功事实留存；重试前读回并完成剩余集合，不称跨包原子回滚 |
| 完整包的 body core 接点 | 场景装配者沿真实ModuleChangeSet/原治理独立注册激活匹配候选的body core，核Body/reduce/pre_action body_action路由及基础绑定不占槽；再查六附件与宿主适配。body core工件/路由失败时保留已安装部分并报告完整包未完成，不把原2+4 helper扩大为7项成功 |
| default toggle 与重复调用 | false 的新世界只有 power；已经安装四模块的世界传 false 不自动卸载；成功后重复同配置无额外 register/消费。四默认模块分别停用再恢复须单独验证，power 用例不能代签 |

当前通用 SelfAgent install 没有 slot 参数，因此通用安装测试只能证明原生命周期；完成上述目标必须定位实际绑定适配及真实状态，不把同名 `installed_module` 字段当结果。

<a id="adm-manual-capabilities"></a>
## 7. ADM-M-06：七能力与降级组合

先固定七类模块各自版本、实际实例/target、依赖、有限参数和真实调用入口。当前 full 测试证明范围按 ADM-M-02 记录；下面按原 PRD 定义完整目标的独立 oracle，未具备消费者的行保留未验证。

| 能力 / 刺激 | 操作与独立预期 |
| --- | --- |
| body 属性 | 合法属性更新、越界/过快变化、资源不足分别调用；核对 Kernel 属性与费用。与扩容并排查槽位：属性更新不增加槽位，扩容不应用 BodyAction 的 Electricity 10 |
| 发电与储能 | 用配置允许的低/正常辐射、距离/热/效率边界和满电前态，在已知逻辑 tick 运行；从配置独立推导非负产量/容量，低辐射为 0，满载不溢出。分别读 audit emission、私有电池和世界 Electricity，不相互代签 |
| 感知 | 在已知范围内、边界、范围外布置合法 Agent/Location 和可见/不可见关键事件，核对授权过滤和结构化条目；位置缺失/模块停用、输出截断/陈旧分别可解释，不能把截断当“没有对象” |
| 移动 | 同位置、合法目标、越几何/速度边界、低能量、冷却中、合法/非法档位分别执行；从实际配置预先计算成本与可达性，再核规则/位置/电池/世界资源。只有实际移动结果成立才更新位置，降档须符合配置并可读 |
| 记忆 | 准备已知 observation、成功/拒绝/待决 action result 和关键事件序列，含重复输入；按候选配额计算应保留条目与摘要/索引来源。跨短期容量和写速阈值后查询长期索引，核对顺序、去重、淘汰、范围与缺失说明；不得把未终结行动改成成功 |
| cargo | 执行 ADM-M-04 的三类实体、存取/列表和满载拒绝；扩容私有计数不能代容量和真实库存 |
| 依赖与低电降级 | 逐步降低能源并记录感知频率、记忆写速、移动档位与待机实际变化；与 PRD 建议顺序及选定策略对照。当前低电拒绝移动仍允许 sensor 的测试只证明该局部策略，不能写为全部建议已落地 |

再停用一个实际依赖或让其在受控 fixture 超燃料/输出限制，确认受影响能力解释限制，仍合法的独立能力保持工作；原位置、实体、槽位和世界历史不被清空。恢复后查询原状态再尝试合法新请求，不能靠重启补电、补物或恢复旧授权。

### 7.1 目标发电与 tick 结算的独立算例

先查候选确实采用 `adm-capabilities-v1`，保存已确认 tick 起点、位置、B/H、last_harvest_tick 与配置 hash。下面每行从独立前态执行一个新逻辑 tick；正常 E=1000，坐标只沿 x 轴。核验者先根据距离平方、先冷却后取热系数及整数公式手算，再看原 audit 与电池。没有新消费者时可做此材料算术核对，但不能记为运行通过。

| 前态/输入 | 独立预期 |
| --- | --- |
| x=800000，B=6，H=0 | 边界属于 R2000；冷却后H0、T1000，P2、accepted2、curtailed0，B8；世界 Electricity 不因充电改变 |
| x=800001，B=6，H=0 | 进入 R1000 段，P1，B7 |
| x=1600000 / 1600001，B=6，H=0 | 外边界分别 P1 / P0，B7 / B6；低辐射不扣电 |
| x=0，B=6，H=6 | 先冷却为H5，T500，所以 P1，B7 |
| x=0，B=6，H=8 | 先冷却为H7，仍T500、P1，B7；不能按冷却前H8错误算成0 |
| x=0，B=6，H=9 | 先冷却为H8，T0，P0，B6；继续正常 tick 冷却后可按新热状态恢复充电 |
| x=0，B=11，H=0 | P2，accepted1，curtailed1，B12；重放本tick仍B12，不累积隐藏电量 |
| 单独命名并记录 hash 的 E500 公式 fixture；x=800001，B=6，H=0 | P=floor(1000×500×1000/1e9)=0；余数不留到下tick。生产三个组合仍E1000 |
| E1001、负B、未知配置或缺合法起点 | 目标配置/状态拒绝，不夹紧为“合法成功”，不回空/补成初始6 |

同一 tick 连续提交 3 个合法只读查询，last_harvest_tick 和B均不再增加；另保存该 tick 后 snapshot 再恢复，重复 tick/事件也不补电。把一个下一 tick 的位置变更放在已确认事件序列里，逐 tick 用实际起点计算，不以恢复后的最终位置给过去补电。只推进墙钟、没有新逻辑 tick 时应无充能。此组专门区别当前 action 补能旧路径。

同tick再经原治理重装、换合法实例或同frame迁移，核对稳定`(world,Agent,tick)`水位与原结果仍在；已充过的tick不能再加2，旧实例迟到响应也不能重充，新绑定从下一合法tick起参与。另令该tick结果尚未核定，确认不能越过它后为旧tick补发新信用。没有本轮跨frame入口，不能用改机体名称建立这组成功前态。

### 7.2 移动、近公里取整与冷却

每行使用同一新策略、当前依赖可用、power投影已追平、Kernel允许该距离、世界Electricity足够的独立前态；除专列D3负例外，正例明确处于D0，且cooldown允许。表中tick的充能已结算，不再因提交动作补电；B12本身不能证明已通过滞回回到D0。保存原from/to，分别核算新策略私有成本与原world费用，不能只把两个数写在同一“电力”字段。

| 前态/位移 | 预期档位、实际成功或拒绝 |
| --- | --- |
| t=20，B12，H0，ready；位移100000cm | economy，私有3、原世界费用1；成功B9/H1/next_move_tick21。同tick再次移动因冷却拒绝，位置及本次业务电池不变 |
| 同样前态，位移200000cm | cruise，私有6、原世界费用2；成功B6/H2/next_move_tick21 |
| 同样前态，位移300000cm | burst，私有12、原世界费用3；成功B0/H4/next_move_tick22，立即进入D4；t21即使其他条件恢复也仍冷却中 |
| 位移 `(100000,1,0)` cm | 新整数 D100001、k2、cruise、私有6；原规则 round 距离为100000、世界费用1。不能用新k替原世界费用的k |
| D0 / 300001，或H8，或t早于next_move_tick | 分别零距离/超范围/过热/冷却拒绝；无本次位置、电池、热与冷却成功效果 |
| B11 的burst，或D3状态请求200000cm | 分别能量缺口1/档位受限拒绝；不能自动改终点、拆请求或报成economy成功 |

对同一已成功原移动重复读结果/恢复 tail，私有成本、世界资源和热增量各只发生一次。拒绝后的修正操作是显式新请求；已有合法 tick 充电与按原合同独立结算的执行费用另列，不能借“不移动”把它们写成不存在。

### 7.3 记忆配额、摘要与检索

在获准隔离环境准备已知来源序列；source tick、序号、类别、结果和降级状态均从合法前态给出。用短记录保证单条低于512 bytes，排除不相关字节边界后独立计算容量。

1. 固定 D0，将来源1～256按每tick16条分布在tick0～15，均为 action_result/committed_success。短期应恰为1～256，长期为空；输入一次重复的256，条数、quota和游标不重复增长。
2. tick16依次提供257～273共17条。仅257～272进入短期，273作为quota skipped记录；短期是17～272共256条。被淘汰的1～16都落在 bucket0/action_result/committed_success，长期仅一行，count16、首末来源1/16，history_complete=false及未保留位置273可读。
3. tick17提供274，其原结果为pending。短期加入274并淘汰17；长期成功行变count17，不能把新pending混进成功计数。重复272、273或274均不重新接受或重新累计skipped。之后仅在新的权威终局来源到达时记录其真实结果，并保留原请求关联。
4. 独立D2前态在一个tick提供5条，前4条进入、第五条quota skipped；若先在D0合法接受10条、同tick后降到D2，原10条保留，后续不再接受。重新启动或按原顺序延迟消费不能重置该tick额度。已经处理tick6后，新增序号却声称source tick5的输入须停止/隔离，不再次发放tick5额度；单调来源证明不能由客户端自报时间替代。
5. 用可复核前史形成128个长期键，再淘汰一个进入新键的短期条目，索引仍128行、字典序最早键被移除、summary_evicted计数加1。查询被移除键应注明不完整，不能解释为世界从未发生该事件。
6. 分别输入超512-byte条目、未知source/冲突序号、损坏state和使总编码超限的候选：超条目明确skipped及缺失范围；无效来源停止该批；坏state/总输出超限保留原可信状态并阻止写入，不能回空声称成功。

通过设计指定的受授权宿主只读投影核对按bucket/kind/outcome检索的排序、32行分页、16KiB响应上限和来源定位；memory reducer保持0 effects/0 emits，查询不能修改来源游标或配额。短期分组按最后来源位置取最近32组后键序输出，超限明确truncated。用同一前态加tail与直接后态比较短期、长期、quota、cursor、skipped/evicted和history_complete，摘要必须字节一致。待决记忆摘要或遗失不代替原请求的权威恢复。

另按候选真实SDK encoder构造合法最大状态和最大受支持输入/响应，记录内层长度与完整CBOR长度：普通`Vec<u8>`不能按裸bytes计外层。设计保守上界是`164×2+128+16=472 KiB < 512 KiB`；其中非状态payload与ctx/封套须分别真实满足128/16KiB限额，不能只测全零字节的最短编码。实际输入/完整ModuleOutput达512KiB内且各下层真实上限均允许时方可执行，超限保留前态并给明确限制；另用更小world/executor/command/transport上限验证候选不适用或合法分页，不能暗调上限。现有8KiB memory output与玩家/Provider额度均不被新内部预算覆盖。

### 7.4 降级顺序、滞回与配置差异

先在独立合法前态核B=12/8/6/4/1，分别得到D0/D1/D2/D3/D4；比较新观察最小间隔1/2/2/2/停、记忆16/16/4/4/0、移动全/全/全/economy/停及cargo可写/待机只读。热与cooldown仍能独立阻止移动，module active不因D4自动变成未安装。

以下恢复轨迹用固定 x=0、E1000、H0、无动作耗电、初态B1/D4/恢复计数0；每tick产2，读实际持久状态，不用反复查询代替tick：

| 新tick序号 | B | tick结束的状态与恢复计数 |
| --- | --- | --- |
| 1 | 3 | D4，1 |
| 2 | 5 | D3，0 |
| 3 | 7 | D3，1 |
| 4 | 9 | D2，0 |
| 5 | 11 | D2，1 |
| 6 | 12 | D1，0 |
| 7 | 12 | D1，1 |
| 8 | 12 | D0，0 |

在第1tick后恢复snapshot，重复读取或同tick其他动作不使计数从1变2。另用合法耗能使电量低于本级回升门槛，恢复计数应清零；跨下降阈值时立即进入更严重级。停用发电但B尚足时仅停止补能；破坏储能投影则依赖新写入/新观察停止，既有库存与记忆仍按权限可读。light/without-memory从D4回D0后仍缺memory，不能由电量回升变成完整包。

对外可读性由实际消费者核对：让独立审读者使用同候选读面，回答“已注册什么、已挂载什么、现在能做什么、缺什么、结果在哪里、下一步如何恢复”。私有诊断可核技术身份，玩家表面应先解释其行动条件；没有实际读面时只做材料审读，不能代签 UI 或真实试玩。

<a id="adm-manual-recovery"></a>
## 8. ADM-M-07：中断、前后态恢复与兼容

每个目标操作至少保存：明确合法前态、原请求/动作、完整已提交结果与 tail、直接操作后 snapshot。原始来源必须在前态中可查；fixture 在 checkpoint 中已存在与“正式来源入账可重放”分别记录。

目标持久路径另保存稳定操作身份、base generation和完整staged候选，按[原generation/G1合同](../prd.md#runtime-generation-persistence-contract)在真实受控故障seam分别核对三态；没有相应消费者时记录未验证：

- `NotCommitted`：在确认未提交的准备/持久化失败cut，机体、两端库存、移动位置、B/H/cooldown、原世界费用、模块state与业务结果均无本次变更，才可丢弃候选；拒绝/故障审计沿原协议，不强行追加矛盾终态。
- `Committed`：令提交确认后镜像/GC等维护失败，仍须安装/恢复原候选；与无故障成功路径逐项一致，只发生一次消费、移动及扣费，不能回到旧余额后重做。
- `StatusUnknown`：在pointer已可见而规定持久确认尚未完成等cut，保存原候选及base，核对整个旧World的新写入、其他Agent写入和tick均隔离。分别提供后来证实committed、证实not committed、仍未知的材料；前两者按原候选安装或确认后丢弃并经过原服务闸门，第三者继续隔离。不能因为旧snapshot健康就丢候选或恢复写权。

上述是本地generation三态oracle，不能代签canonical authority证明、独立故障域持久条件或恢复serving；正式场景仍分别验证原authority/历史版本及当前服务闸门。

| 场景 | 具体方法与独立预期 |
| --- | --- |
| 未发布前失败 | 沿受控 prepare/publication seam 注入失败，比较两端库存、槽位、绑定、结果和适用发布状态；业务变更均为前态。已有 raw-event regression 只代其固定范围，不能扩大为整个 root transaction |
| 结果响应丢失 | 在提交后、消费者收到结果前中断读取，保存原请求；重连查询其持久判定。成功只消费一次；仍未知保持未知，不以相同 payload 创建新 ActionId |
| 操作后 snapshot | 用成功后的 snapshot 恢复，核对库存/槽位/绑定及模块状态与原后态相同。该路径对应现有 body/scenario restore，不代表前态 replay |
| 操作前 checkpoint 加 tail | 从确实含源库存的操作前 checkpoint 恢复，再读完整提交 tail；与直接后态比较实体总量/消耗、槽数/层级、绑定、资源和适用状态根；replay 不再调用 LLM、不重新执行来源或扣费 |
| 投影处理失败 | 核心扩容已提交但 cargo 私有统计尚未处理时中断；恢复按原事件/版本重建投影，核对核心只消费一次，不重发扩容来补计数 |
| 旧/派生电池投影落后 | 与完整目标的提交前失败分开：用已原子提交的t20 burst结果，其权威位置、B0/H4/next_move_tick22与原世界费均已成立，令派生读面仍显示旧B12并受控失败；旧投影虽可解码也须标stale/catch-up并阻止依赖该读面的新写。追平只反映原结果一次，不再次移动/扣电/加热。现有兼容post-event路径另按其原规则验证，不能用迟补账代签完整目标全部或无 |
| 缺工件、坏 memory bytes、未知格式、缺/冲突日志 | 保留可信历史与明确失败/只读范围，阻止受影响能力写入；不以 decode 默认空状态宣布恢复，不生成替代物品或隐式切新版本 |
| owner/授权在中断期间改变 | 恢复已提交历史而不把旧授权恢复为当前；未生效动作提交前重验，失效时拒绝/按原合同终止，不作为新 owner 的新选择 |
| 旧机体/cargo 迁移与模块回退 | 保留原槽、原数量和原历史；不从缺失 source 推定可交易来源，不重复 bootstrap。新字段/配置由确定性受支持迁移补充；无法表示新状态的旧 reader 不静默丢字段后开放写入 |
| 新策略切入与重复恢复 | 在已确认t0、原移动已结算、B5的前态采用新策略：B仍5，旧格式缺热时有明确H0迁移记录、last_harvest_tick=t0、next_move_tick=t0+2；从t0+1起才新算法补能。再次恢复同一新状态不重置热/冷却、不赋6、不重做迁移。旧memory标签仍可读，新索引标明切入前不覆盖 |

跨进程/跨节点、目录/CAS 或跨故障域验证只在实际受影响的持久消费者已具备时按其原手册执行；本地 World/from_snapshot 或内存 journal 结果不能代签这些结论。若该项属于交付范围且前置不足，应保留 blocked/inconclusive 与具体缺项。

## 9. 记录结论

在实际 Task/PR 逐项写：对应 AC、候选/配置/环境、来源前置、真实入口、执行步骤和命令、独立预期、实际状态/事件、失败或未执行范围。清楚区分文档方法、源码定义、本地 fixture、正式消费者与真实世界结果。

可以形成“默认安装与 fixture 扩容局部通过、完整 cargo/挂载/长期记忆仍缺实现”的结论；不能因为局部通过改变原 AC 或把后续来源标成已交付。复核应能沿同一原请求、同一实体来源和同一模块版本重现判断，不新建任务台账或通用放行凭证。
