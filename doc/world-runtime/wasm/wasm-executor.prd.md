# oasis7 Runtime：WASM 执行器接入（设计分册）

- 专业 owner：`wasm_platform_engineer`；状态：active professional authority（目标合同不等于已实现）。
- 内容审读基线：`eng-cc/oasis7`，源提交 `9c41d57b4436f71f0cf9b481043d882cfdc551ed`；内容整理日期：2026-09-26。
- 独立审读与实际实现/测试候选证据由该 task evidence 记录；此处不预报审读通过、runtime capability、发布或 full proof。下方既有条款仍有效；仅显式日期/历史基线条款按其证据范围解释。

<a id="sr2-acceptance"></a>
## SR2 具体目标验收与专业承接

<a id="sr2-domain-case-1"></a>

### RCP-01 / RCP-02 专业接受条件

输入：wrong request/target/block/digest、不可获取 proof、local signer/provider/CI 伪装。断言：不计 success；external trust/finality 检查失败；新 effects=0。目标字段、算法和恢复条件由 [RCP-01 / RCP-02精确设计](wasm-interface.md#sr2-execution-contract) 承接；跨域 proof/publication obligation 由 [RCP-01 / RCP-02外部接收器](wasm-executor.design.md#sr2-host-publication) 承接。required codec/deterministic fixture 与 full actual finality/candidate artifacts 是不同层，本次未运行。

<a id="sr2-domain-case-2"></a>

### MIG-01 / MIG-02 / MIG-03 专业接受条件

输入：wrong from/to/hash、oversize/noncanonical output、namespace/caller forgery、旧 bytes跨迁移 restart。断言：拒绝完整 staged publication；old bytes保留；重放 recorded hashes一致。目标字段、算法和恢复条件由 [MIG-01 / MIG-02 / MIG-03精确设计](wasm-interface.md#sr2-execution-contract) 承接；跨域 proof/publication obligation 由 [MIG-01 / MIG-02 / MIG-03外部接收器](../module/module-lifecycle.md#sr2-migration-publication) 承接。required codec/deterministic fixture 与 full actual finality/candidate artifacts 是不同层，本次未运行。

<a id="sr2-domain-case-3"></a>

### MET-01 / MET-02 专业接受条件

输入：历史 commit后改价、overflow/fuel/balance不足、unordered访问。断言：历史费用不变；提交前拒绝；free coefficient不抹 actual access。目标字段、算法和恢复条件由 [MET-01 / MET-02精确设计](wasm-interface.md#sr2-execution-contract) 承接；跨域 proof/publication obligation 由 [MET-01 / MET-02外部接收器](wasm-interface.md#sr2-metering-contract) 承接。required codec/deterministic fixture 与 full actual finality/candidate artifacts 是不同层，本次未运行。

<a id="sr2-domain-case-4"></a>

### ENC-01 专业接受条件

输入：UTF8>256 bytes、U64 max/max+1/>2^53/leading0/exponent、GitHash混用/tree swap、scope冲突。断言：strict receiver拒绝；max lossless roundtrip；CBOR/JSON shape区分。目标字段、算法和恢复条件由 [ENC-01精确设计](wasm-interface.md#sr2-execution-contract) 承接；跨域 proof/publication obligation 由 [ENC-01外部接收器](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix) 承接。required codec/deterministic fixture 与 full actual finality/candidate artifacts 是不同层，本次未运行。

<a id="sr2-domain-case-5"></a>

### HASH-01 专业接受条件

输入：detached inclusion变化、circular inclusion进入core、matching digest错误proof body、locator改变。断言：同core IDs稳定；fake inclusion拒绝；same bytes locator不改变content identity。目标字段、算法和恢复条件由 [HASH-01精确设计](wasm-interface.md#sr2-execution-contract) 承接；跨域 proof/publication obligation 由 [HASH-01外部接收器](wasm-executor.design.md#sr2-dc5-semantic-validator) 承接。required codec/deterministic fixture 与 full actual finality/candidate artifacts 是不同层，本次未运行。


既有所有成功标准、验收、limits、公式、historical identities和能力限制保留；下表逐项承接到 [精确target设计](wasm-interface.md#sr2-execution-contract)，而非以任务链接代替设计。本次文档变更只承担文档合同采纳；现行实现partial与未来所需runtime/proof验证保持独立。typed字段共用 [interface types](wasm-interface.md#sr2-types)；build/signature/SDK/metrics局部结果均不证明canonical世界效果。

| 原 obligation identity（下方完整原文） | 具体承接结果 / 适用条件 | 精确设计 receiver | 验证范围 / evidence |
| --- | --- | --- | --- |
| [E2原文](#sr2-obligation-e2) | ### 实现要点（E2） | [E2本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E3原文](#sr2-obligation-e3) | ### 实现要点（E3） | [E3本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E4原文](#sr2-obligation-e4) | ### 实现要点（E4） | [E4本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E5原文](#sr2-obligation-e5) | ### 实现要点（E5） | [E5本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E6原文](#sr2-obligation-e6) | ### 实现要点（E6） | [E6本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E7原文](#sr2-obligation-e7) | ### 实现要点（E7） | [E7本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E8原文](#sr2-obligation-e8) | ### 实现要点（E8） | [E8本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E10原文](#sr2-obligation-e10) | ### 实现要点（E10） | [E10本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E11原文](#sr2-obligation-e11) | ### 实现要点（E11） | [E11本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E12原文](#sr2-obligation-e12) | ### 实现要点（E12） | [E12本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E1原文](#sr2-obligation-e1) | E1：选择 WASM 引擎并完成配置结构体与沙箱实现骨架。 | [E1本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E9原文](#sr2-obligation-e9) | E9**：模块调用入口按 ModuleKind 选择并补充测试。 | [E9本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E13原文](#sr2-obligation-e13) | E13**：将磁盘编译缓存从“原始 wasm 回盘”修正为“序列化 compiled artifact 回盘”，补齐 round-trip 与损坏恢复回归。 | [E13本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [E14原文](#sr2-obligation-e14) | E14**：将 agent-os 对齐增强与 sandbox 安全硬化的永久契约并入本稳定权威，退役重复专题文档。 | [E14本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |
| [PRD-ENGINEERING-006原文](#sr2-obligation-prd-engineering-006) | &#124; PRD-ENGINEERING-006 &#124; 文档内既有任务条目 &#124; test_tier_required &#124; ./scripts/doc-governance-check.sh + 引用可达性扫描 &#124; 迁移文档命名一致性与可追溯性 &#124; | [PRD-ENGINEERING-006本域合同](wasm-interface.md#sr2-execution-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../crates/oasis7_wasm_executor/src/tests.rs)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |

目标 acceptance 另外要求：有效 ModuleExecutionReceiptV1 仅 committed_success；canonical no-effect由runtime IntentDecisionV1区分；world branch、member_generation、decimal U64与UTF8byte bound全链一致；receipt core不包含递归blockproof。migration/state/schedule/replay精确字段由interface独占，运行发布点由lifecycle/executor独占。观测不可进入计费/世界hash；Docker构建proof与external release finality独立；SDK旧wasm-1 optional/default不变。完整DC5 attachment由GWSC/schema接收，SC9全部8cells与provider/headed proof不裁剪。


- 对应设计文档: `doc/world-runtime/wasm/wasm-executor.design.md`
- 稳定证据入口: `doc/world-runtime/wasm/evidence.md`

审计轮次: 4


本分册描述将真实 WASM 执行器接入 `ModuleSandbox` 的最小方案，并作为已完成 agent-os 对齐增强与 sandbox 安全硬化的稳定专业权威。模块工件持久化生命周期仍由 `doc/world-runtime/prd.md` 拥有。

## 1. Executive Summary

### Capability 验收逐项承接

AUTHZ-01..18 的原文与原 required/full tier 继续有效；其唯一规则 authority 是 [interface授权验收表](wasm-interface.md#37-authorization-acceptance-criteria)。下表按原表的精确 row identity 分配真实receiver；已有ABI/envelope测试是partial，不能代替subject/issuer/nonce/finality/staged publication新negative fixtures。

| 上游精确 row / 义务 | 本设计接收条款 | required/full验证场景与断言 | 当前事实 / evidence边界 |
| --- | --- | --- | --- |
| [AUTHZ-01](wasm-interface.md#37-authorization-acceptance-criteria) — A valid Agent/Module/System subject and finalized trusted issuer produce a stable grant id; tampered issuer, signature, nonce or grant body is rejected. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-01` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-02](wasm-interface.md#37-authorization-acceptance-criteria) — Scope matching is conjunctive and exact across module id/version, namespace, object, operation and entity/resource selectors; omitted or widened selectors fail. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-02` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-03](wasm-interface.md#37-authorization-acceptance-criteria) — Parent delegation can only attenuate scope, expiry and depth; parent or child revocation invalidates the correct descendants without mutating historical receipts. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-03` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-04](wasm-interface.md#37-authorization-acceptance-criteria) — Expiry, revocation epoch/list, trust-root rotation and stale registry state fail closed at execution time. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-04` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-05](wasm-interface.md#37-authorization-acceptance-criteria) — Dynamic catalog entries are filtered by the current subject and bind module registry, policy and revocation hashes; catalog exposure alone grants no authority. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-05` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-06](wasm-interface.md#37-authorization-acceptance-criteria) — Provider responses must bind subject, exact selected schema, catalog snapshot and fresh nonce; forged/unknown/stale responses are rejected before sandbox or journal mutation. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-06` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-07](wasm-interface.md#37-authorization-acceptance-criteria) — A live recheck catches grant revocation, module deactivation, policy change and entity/resource ownership change between discovery and execution. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-07` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-08](wasm-interface.md#37-authorization-acceptance-criteria) — Governance issuance requires finalized authority; pending proposals and consensus receipts without governance grant semantics cannot authorize module effects. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-08` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-09](wasm-interface.md#37-authorization-acceptance-criteria) — Snapshot/restart/replay preserves grant, parent, revocation, trust-root and nonce state and reproduces the same accept/deny decision and hashes at the same historical head. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-09` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_full`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-10](wasm-interface.md#37-authorization-acceptance-criteria) — Legacy migration is scope-preserving; `allow_all` remains native-only, new module commands require v2 grants, and the cutoff rejects unmigrated aliases. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-10` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_full`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-11](wasm-interface.md#37-authorization-acceptance-criteria) — Every issuance, delegation, revocation, catalog, check, denial and command submission has deterministic audit bindings without leaking payload secrets. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-11` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_full`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-12](wasm-interface.md#37-authorization-acceptance-criteria) — Any missing policy, issuer, revocation, journal or audit authority fails closed with no partial state/effect/charge; accepted commands enter the same staged transaction and receipt path. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-12` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_full`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-13](wasm-interface.md#37-authorization-acceptance-criteria) — Presenter/provider, subject and audience/world are distinct; a provider cannot substitute identity, audience, grant or scope, and cross-world/branch replay is rejected. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-13` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-14](wasm-interface.md#37-authorization-acceptance-criteria) — Issuance is deterministic canonical signing bound to issuer key epoch and finalized authority rotation; high-impact grants require a verified finality block hash. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-14` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-15](wasm-interface.md#37-authorization-acceptance-criteria) — The persistent nonce tuple `(subject, issuer/key_epoch, grant, audience, branch/finality epoch, nonce)` is idempotent for the same request hash and rejects a different hash without mutation. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-15` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-16](wasm-interface.md#37-authorization-acceptance-criteria) — Nonce reservation, budget before/after, world effects, module state, events and authorization receipt commit atomically; crash/recovery resolves reserved records deterministically. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-16` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_full`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-17](wasm-interface.md#37-authorization-acceptance-criteria) — Partition, pending/unverified finality and reorg/orphaned branch evidence are preview-only or suspended; no unverified grant can authorize a command or high-impact effect. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-17` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_full`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
| [AUTHZ-18](wasm-interface.md#37-authorization-acceptance-criteria) — `allow_all`, `add_capability`, provenance, process-local caches, provider responses and consensus receipts cannot bypass live v2 authorization. | [host authorization/publication](wasm-executor.design.md#sr2-host-publication)；[types / scope](wasm-interface.md#sr2-types)；原授权条款仍为authority | `AUTHZ-18` target fixture：按本行原义务构造正常+失配/撤销/缺proof输入，验证原断言与denied no-mutation；原tier `test_tier_required`不变；samecandidate/worldbranch/full真实proof | [partial envelope tests](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) located/unrun；本次文档变更仅作文档采纳；authority+nonce/staged/crash证明未执行 |
- 在现有 `ModuleSandbox` 抽象之上提供真实 WASM 执行实现（首选 Wasmtime）。
- 与既有 ABI/序列化约定对齐，保证输入/输出可验证、可回放。
- 提供确定性与资源限制（内存、燃料、超时、输出大小）并可审计。

## 2. User Experience & Functionality
### In Scope（V1）
- 以 `ModuleSandbox` 为适配层的执行器实现（不改动 world 内核调用流程）。
- 基本资源限制：内存上限、燃料/指令预算、超时、输出大小。
- 最小编译缓存：按 `wasm_hash` 缓存已编译模块，并可把 Wasmtime 序列化产物持久化到磁盘。
- 可配置的执行器参数（燃料、超时、并发上限、缓存容量）。
- 过渡期占位实现：未接入引擎时返回 `SandboxUnavailable`。
- 基础依赖通过 Cargo feature `wasmtime` 引入。
- 引擎骨架以 `Engine::default()` 初始化，后续再接入燃料/超时配置。

### Out of Scope（V1 不做）
- 多线程并行执行与跨模块共享状态。
- 复杂 I/O host functions（保持纯函数模型）。
- JIT 运行时热更新或远程分发。


## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（文档迁移任务）。
- Evaluation Strategy: 通过文档治理校验、引用扫描与任务日志检查验证迁移质量。

## 4. Technical Specifications
### 关键接口
- `ModuleSandbox`：保持现有 `call(request) -> ModuleOutput` 入口不变。
- `WasmExecutorConfig`（新增）：执行器配置（燃料、超时、内存上限、缓存上限）。
- `WasmExecutor`（新增实现）：封装底层引擎并实现 `ModuleSandbox`。

### 执行流程（概念）
1. 校验 `ModuleCallRequest`（limits 与运行时最大值）。
2. 按 `wasm_hash` 获取/编译模块（命中缓存或新编译）。
3. 绑定 host functions（仅暴露 ABI 必需的接口）。
4. 按 [WASM ABI](wasm-interface.md) 调用物理导出：`Reducer -> reduce`、`Pure -> call`，传入序列化输入；`compute` 不是 ABI 导出名。
5. 读取并反序列化输出，执行 `ModuleOutput` 校验。
6. 超时/超限返回 `ModuleCallFailure`，写入 `ModuleCallFailed` 事件。

### 资源限制与确定性
- **燃料/指令预算**：优先使用引擎原生 fuel/epoch 机制。
- **内存限制**：WASM memory pages + 运行时限制双重校验。
- **超时**：引擎 epoch 或外部 watchdog 触发超时。
- **确定性**：禁用非确定性 host function（时间、随机、I/O）。

<a id="sr2-obligation-e2"></a>

### 实现要点（E2）
- Wasmtime 引擎启用 fuel + epoch interruption 以支持超时/燃料限制。
- 执行器在调用前预检查请求 limits（fuel/memory/output），并映射到 ModuleCallErrorCode。
- 输出校验失败路径单元测试覆盖 OutputTooLarge / Timeout 场景。

<a id="sr2-obligation-e3"></a>

### 实现要点（E3）
- 编译缓存以 `wasm_hash` 为键，LRU 策略，容量由 `max_cache_entries` 控制。
- 缓存通过 `Arc<Mutex<...>>` 共享，允许多执行器克隆共享已编译模块。
- 编译过程与缓存锁分离，避免长时间持锁。
- 若配置 `compiled_cache_dir`，磁盘层必须持久化 Wasmtime `Module::serialize()` 产物，而不是原始 `.wasm` 字节；缓存文件需带自定义 magic/version/checksum wrapper，并在反序列化前先做 wrapper 校验与 precompiled marker 检测，失败按 cache miss + 删除处理。
- 磁盘 cache 目录必须按当前 engine 的 precompile compatibility key 与宿主 `arch/os` 隔离，避免不同 Wasmtime 兼容域或宿主目标复用旧 `.cwasm`。

<a id="sr2-obligation-e4"></a>

### 实现要点（E4）
- Wasmtime 执行器使用 `memory`/`alloc`/`reduce|call` 导出进行最小调用（`reduce/call(i32, i32) -> (i32, i32)`，入口取决于 ModuleKind）。
- `ModuleCallRequest` 增加 `wasm_bytes`，由 `World::execute_module_call` 注入真实工件。
- 集成测试通过 `--features wasmtime` 验证真实 wasm 调用与回放事件一致性。

<a id="sr2-obligation-e5"></a>

### 实现要点（E5）
- 输出采用 Canonical CBOR 解码为 `ModuleOutput`。
- 集成测试的 wasm 工件输出切换为 CBOR 编码。

<a id="sr2-obligation-e6"></a>

### 实现要点（E6）
- 事件/动作输入改为 Canonical CBOR 编码，满足 `wasm-1` ABI 的确定性要求。
- 新增模块输入 CBOR 编码的路由测试。

<a id="sr2-obligation-e7"></a>

### 实现要点（E7）
- 模块输入封装为 `ModuleCallInput { ctx, event|action }`，携带 `ModuleContext` 元信息。
- `ModuleContext` 包含 `v/module_id/trace_id/time/origin/limits` 等字段。
- 新增输入 envelope 编码测试，校验 ctx 与 event/action bytes。

<a id="sr2-obligation-e8"></a>

### 实现要点（E8）
- `ModuleContext.world_config_hash` 使用当前 manifest 的哈希（`current_manifest_hash`）。
- 输入 envelope 测试校验 `world_config_hash` 一致性。

<a id="sr2-obligation-e10"></a>

### 实现要点（E10）
- reducer 调用输入携带 `state`（空字节串代表无历史状态）。
- 模块返回 `new_state` 时记录 `ModuleStateUpdated` 并更新状态，保证回放一致。
- pure 模块返回 `new_state` 视为 InvalidOutput。

<a id="sr2-obligation-e11"></a>

### 实现要点（E11）
- 将 `crates/oasis7/Cargo.toml` 的 `wasmtime` 依赖从 `18` 升级到 `41`，并刷新 `Cargo.lock`。
- 保持现有 `ModuleSandbox` 的 Wasmtime API 调用路径不变（`Config/Engine/Store/Linker/TypedFunc`），验证升级后可直接兼容。
- 升级后通过 `--features wasmtime` 执行 `cargo check` 与 `wasm_executor` 相关测试，确保执行器闭环可用。

<a id="sr2-obligation-e12"></a>

### 实现要点（E12）
- `WasmExecutor` 初始化失败必须返回结构化错误，不允许以 `panic` 终止宿主。
- `oasis7_wasm_sdk::wire` 的 CBOR 编解码失败必须向调用者显式暴露，由 builtin 模块明确选择 fallback，而不是由 SDK 静默吞错。
- Node/runtime 入口在构造执行器失败时需保留可观测错误文本，测试需覆盖磁盘缓存初始化失败路径。

### ABI / capability 对齐契约

- `ModuleManifest.abi_contract` 的版本、输入/输出 schema、`cap_slots` 与 `policy_hooks` 是默认兼容的可选扩展；旧 manifest 缺省这些字段仍可读取。
- effect 可以携带可选 `cap_slot`；存在 slot 时必须解析到 manifest 中唯一的 `cap_ref`，与显式 `cap_ref` 冲突或未声明时拒绝。
- pure policy hook 只能作为 effect 提交前的确定性判定器，不得递归产生世界副作用或取得绕过治理的写权限。
- `ModuleContext` 可携带 stage、manifest hash、journal height、module version/kind/role 等可选元信息；缺省值必须兼容旧模块。
- 这些扩展不替代 `wasm-1` ABI，不得被描述为新主版本或强制所有历史模块立即实现。

### Sandbox 与工件完整性契约

- 请求 `max_gas = 0` 使用执行器配置的 `max_fuel`，不得形成无上限执行。
- 每次调用设置 epoch deadline 并由 watchdog 触发抢占；fuel 耗尽与 epoch interruption 映射为结构化 timeout/failure。
- store memory limiter 约束 `memory.grow`；linker 默认不暴露非 ABI 必需的时间、随机或 I/O capability。
- 从持久化存储加载工件时必须重算 SHA-256 并与记录的 `wasm_hash` 比对，失败拒绝加载；storage 布局、retention 和 lifecycle 不由本文重定义。
- 编译缓存只保存带 wrapper/version/checksum 的 serialized compiled artifact；校验失败按 cache miss 安全恢复，并按 engine/OS/arch 兼容域隔离。
- 引擎初始化、缓存目录和 codec 失败返回结构化错误，不允许以 panic 终止宿主。

## 5. Risks & Roadmap
<a id="sr2-obligation-e1"></a>

- **E1**：选择 WASM 引擎并完成配置结构体与沙箱实现骨架。
- **E2**：接入燃料/超时/内存限制，输出校验与错误码映射。
- **E3**：实现编译缓存与并发安全策略。
- **E4**：补充集成测试（真实 wasm、超限失败、确定性回放）。
- **E5**：切换 ModuleOutput 编码为 Canonical CBOR，并完善 ABI 说明与测试。
- **E6**：模块输入切换为 Canonical CBOR 编码并补充测试。
- **E7**：模块输入封装 ModuleContext + event/action envelope 并补充测试。
- **E8**：补充 world_config_hash 并测试。
<a id="sr2-obligation-e9"></a>

- **E9**：模块调用入口按 ModuleKind 选择并补充测试。
- **E10**：模块状态输入/更新接入并补齐回放一致性测试。
- **E11**：升级 Wasmtime 版本（18 -> 41）并完成兼容性回归验证。
- **E12**：清理执行器初始化 `panic` 与 SDK wire 静默吞错路径，补足失败路径结构化错误回归。
<a id="sr2-obligation-e13"></a>

- **E13**：将磁盘编译缓存从“原始 wasm 回盘”修正为“序列化 compiled artifact 回盘”，补齐 round-trip 与损坏恢复回归。
<a id="sr2-obligation-e14"></a>

- **E14**：将 agent-os 对齐增强与 sandbox 安全硬化的永久契约并入本稳定权威，退役重复专题文档。

### Technical Risks
- 引擎版本升级导致行为变化（需锁定版本/回放验证）。
- 资源限制不一致（引擎与内核限制口径差异）。
- ABI 变更导致兼容性破坏（需版本化接口）。

## 6. Validation & Decision Record

执行器 ABI、资源限制、工件完整性和损坏恢复的稳定验证面见
[`evidence.md`](evidence.md)。任务状态、实现批次和候选级结论由 GitHub
task issue / Project 与 Git history 追溯，不能由本文代替。

- Test Plan & Traceability:
<a id="sr2-obligation-prd-engineering-006"></a>

| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-ENGINEERING-006 | 文档内既有任务条目 | `test_tier_required` | `./scripts/doc-governance-check.sh` + 引用可达性扫描 | 迁移文档命名一致性与可追溯性 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-DOC-MIG-20260303 | 逐篇阅读后人工重写为 `.prd` 命名 | 仅批量重命名 | 保证语义保真与审计可追溯。 |
| DEC-WR-WASM-EXEC-001 | 将 agent-os 对齐与 sandbox 安全契约吸收到 executor 权威 | 长期保留两个完成专题作为并列权威 | 两者共同约束 executor/ABI 执行边界；集中后更能保持 compatibility、limits、capability 与 cache 语义一致。 |

## 原文约束点映射（内容保真）
- 原“目标” -> 第 1 章 Executive Summary。
- 原“范围” -> 第 2 章 User Experience & Functionality。
- 原“接口 / 数据” -> 第 4 章 Technical Specifications。
- 原“里程碑/风险” -> 第 5 章 Risks & Roadmap。

## 7. 执行计量与 artifact 边界

- executor 只装载 hash 已验证的 artifact，并按 manifest limits/capabilities 执行；module storage、ownership、market 和 lifecycle 状态仍由 runtime module authority 拥有。
- 每次调用的 compute 计量由输入/输出的 1 KiB 向上取整单位、effect 权重与 emit 数确定，electricity 计量由基础调用、effect、emit 与新状态共同确定；实际资源类型、扣减和审计事件由 runtime 统一应用。
- 余额不足必须产生结构化 policy rejection，且不扣费、不应用输出/state/effect/emit。成功调用才形成计费事件；replay 读取事件，不重新执行计量或价格判断。
- source package 编译不是 executor 生产职责；生产只接受 canonical binary + receipt，dev/test source compile guardrail 由 deterministic-build pipeline 维护。
