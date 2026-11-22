# WASM 开放制度扩展合同（分册）

- 主接口文档：[`wasm-interface.md`](wasm-interface.md)
- 本分册承接原 `wasm-interface.md` 的“开放制度扩展合同（试点，非全面迁移）”正文，不改变既有契约、状态边界或验证义务。

## 开放制度扩展合同（试点，非全面迁移）
本节将“制度能力由 WASM 扩展”的合理部分落实为一个可审计的增量合同。它是
现有 `wasm-1` ABI 的可选扩展，不是立即把 `WorldState` 改成 ECS、删除既有
native action，或允许任意未治理工件进入生产执行路径。
> **状态边界：command 与 provider 各有基础切片，但尚未闭环。** 当前
> `ModuleAbiContract.declarations.commands` 已提供版本化 command 声明；manifest
> admission 会校验声明，active module 可投影为确定性的 command catalog，
> `World::execute_module_command` 会在触碰 sandbox 前校验 active 状态、声明匹配、
> schema/hash/payload 边界，并复用现有计量调用链。`CapabilityCatalogSnapshot`、
> subject-bound context、`AgentCommandResponse` 及 provider/loopback 校验也已进入
> 代码。这些是可审计的基础切片，不是完整协议：payload 语义 schema、trusted host
> typed-args 编码、instance target、component/event/migration、execution receipt、
> 完整 capability scope、生产 context 自动接线、真实 Agent E2E 和 staged atomic
> commit 仍是目标扩展。下面的完整结构和字段名仍是语义草图，不能直接当作现行 wire
> 定义；试点实现前还必须补齐 ABI 版本、编码规范、hash/receipt 绑定、迁移策略和
> 失败路径测试。当前生产路径仍同时承诺本文件前文定义的现有 `wasm-1` 行为与上述
> 已落地的 command/provider 基础切片。
试点只选择一个有界制度（例如 Alliance 或 EconomicContract，由跨角色治理
确定），以现有 native vertical slice 作为兼容基线；试点成功前不得把所有
经济、战争或治理能力迁出内核。模块治理、工件发布和实例生命周期继续遵循
`doc/world-runtime/module/module-lifecycle.md` 的 `propose -> shadow ->
approve -> apply` 闭环。
### 1. 命名空间、声明和版本
模块的开放对象**目标上**必须在 manifest 中声明。当前 command 已是
`ModuleAbiContract.declarations.commands` 中 serde-defaulted 的版本化字段，并有
声明 admission；但当前声明集合仍只覆盖 command，不包含 component、event 或
migration。扩展这些对象前必须定义旧 manifest 的读取方式、新旧 ABI 的
admission/migration 规则，并证明历史 `wasm-1` 模块仍走原有路径。
```rust
// Target sketch for the full declaration set; the current wire type only supports commands.
struct ModuleSchemaDeclarations {
    commands: Vec<CommandSchema>,
    components: Vec<ComponentSchema>,
    events: Vec<EventSchema>,
    migrations: Vec<SchemaMigration>,
}
// Target sketch only; canonical encoding and capability binding remain prerequisites.
struct SchemaRef {
    name: String,
    schema_version: u32,
    schema_hash: String, // SHA-256 of the canonical schema descriptor
    max_payload_bytes: u64,
}
// Target sketch only; migration_manifest_hash must be bound before pilot admission.
struct SchemaMigration {
    object_kind: String, // "component" or "event"
    namespace: String,
    name: String,
    from_schema_version: u32,
    to_schema_version: u32,
    from_schema_hash: String,
    to_schema_hash: String,
    migration_wasm_hash: String,
    migration_manifest_hash: String,
}
```
未来版本可在 `ModuleManifest.abi_contract` 下继续承载等价的 declarations；当前
`ModuleAbiContract.declarations.commands` 已定义并参与 admission，但当前 wire 尚未
定义 component、event 或 migration declarations。这些声明不能替代
`interface_version`、`input_schema`、`output_schema`、`required_caps` 或
`cap_slots`。ABI 版本、对象 schema 版本、manifest hash 和工件 `wasm_hash`
是四个不同的目标身份维度：任一声明、权限、limits 或 metering schedule 的
hash 影响规则，都必须在版本化 ABI 和 canonical hash 规范中先定义，再作为
试点 admission 条件；工件字节变化仍必须产生新的 `wasm_hash`，不得同名覆盖。
命名约束如下：
- `namespace` 是由治理绑定到 module identity 的小写、稳定标识（示例
  `bank.acme`）；`command`、`component`、`event` 是 namespace 内的本地名，
  三者的完整键分别为 `namespace:command`、`namespace:component`、
  `namespace:event`。
- `core.*`、`kernel.*` 和内核保留对象不可由模块注册或覆盖。命名空间不能
  通过上传新工件重新归属；归属变更必须走新的治理提案和 manifest hash。
- 同一完整键的 schema 版本必须显式递增。旧版本仍可用于历史事件和回放；
  未知的未来版本在边界处 fail closed，不能按“最接近字段”猜测解码。
- schema descriptor、声明列表、capability 绑定和 limits 在计算 manifest
  hash 前按完整键和版本排序；模块执行时的访问集合也必须按同一稳定顺序
  处理，不能依赖 map/hash iteration 顺序。
### 2. Command / component / event wire
当前已落地一个受界限的 module-command 入口，不要求为每一种银行、联盟或合约
操作新增闭合的 Rust `Action` 变体。`ModuleCommandEnvelope` 承载如下 command
字段；runtime 会在进入 sandbox 前校验 active module 的声明、namespace/name、
schema version/hash 和 payload 边界，再复用现有计量调用链。该入口仍只是基础切片；
provider/loopback 已有 response 校验，但尚未证明生产自动接线，也不等于完整的 caller capability、通用
action/router、component/event/migration wire、staged atomic commit 或 execution
receipt 协议。
```text
{
  "namespace": "bank.acme",
  "command": "open_account",
  "schema_version": 1,
  "schema_hash": "<sha256-hex>",
  "payload": <canonical-cbor bytes>
}
```
这里的 **Canonical-CBOR 保证只覆盖 envelope**。`payload` 是 opaque CBOR byte string；
`decode_canonical` 验证外层 map 的 byte-exact 编码，`schema_hash` 只与 active
manifest declaration 相等并检查大小上限。runtime 不递归验证 payload 的字段类型、
范围或内部 canonical 编码，不能把 canonical envelope 写成 canonical payload schema。
目标上，每条 command 必须有可寻址的语义 descriptor，并由内容地址绑定：
```text
CommandSchemaDescriptor = {
  namespace, command, schema_version,
  encoding: "canonical-cbor", request_shape, response_shape?,
  field_constraints, max_payload_bytes,
}
schema_hash = SHA-256(Canonical-CBOR(CommandSchemaDescriptor))
```
descriptor bytes 由受信 schema registry 按 `schema_hash` 提供；manifest declaration
和 catalog 只接受该内容地址，不能由 provider 临时改写。`request_shape` 至少表达
字段名、标量/容器类型、必选性、整数范围和嵌套 schema hash；response/error shape
也必须版本化。排序和 hash 域需在试点前冻结；当前 `ModuleCommandDeclaration` 的
`schema_hash` 仍只是声明级标识，不是 descriptor registry 或语义验证器。
目标上 Agent/provider 只返回 typed args。受信 host 按精确 `schema_hash` 取得
descriptor，校验字段/类型/范围/大小，以唯一 Canonical-CBOR encoder 生成 payload，
再构造并重验 `ModuleCommandEnvelope`；provider 不得决定 caller、target、grant 或
provenance。当前 response 仍携带 raw payload，typed-args encoder 和生产 registry
尚未实现；raw envelope 仅是兼容/试验路径，不能作为语义安全证明。
试点宿主从受信调用边界注入 caller、origin、logical time、trace/journal 位置；模块
不得在 payload 中伪造。当前 `ModuleContext` 已承载 host-injected caller、manifest
和 journal provenance，但没有完整 subject/instance/entity target；action 路由也
没有这套 admission，须在试点前补齐并绑定回放。
目标宿主还必须先确认模块处于 active 状态、完整命中 manifest 声明、`payload`
哈希/大小和 schema 版本有效，再进入 WASM；未知 namespace、command 或版本均
结构化拒绝。
目标 command 请求必须把实例寻址放在受信 envelope/context 外层，而不是由 payload 自报：
```text
ModuleCommandTarget = {
  module_id, module_version, instance_id, entity_id?,
}
ModuleInvocationProvenance = {
  caller, origin, target: ModuleCommandTarget, manifest_hash, wasm_hash,
  journal_position?,
}
```
catalog entry、capability scope、nonce 和 execution receipt 必须绑定同一精确 target；
singleton 也必须使用稳定 `instance_id`，不能退化为 module-global 状态。当前
lifecycle/tick/state 更新路径已有 instance-scoped 证据，但 direct/trusted command
的 state lookup、catalog target 和部分 provenance 仍按 `module_id`；在 target 字段
进入 admission、snapshot/replay 和 receipt 前，不能宣称 instance-complete。
未来扩展可采用逻辑 component 记录，但这不是当前 `WorldState` 已有的 component
存储，也不是本期承诺的完整 ECS 或 `WorldState` 重写。稳定 instance/entity
寻址、journal 所有权、snapshot/replay 绑定和读写 admission 都是试点前置条件：
```text
ComponentKey   = { namespace, component, schema_version, entity_id }
ComponentValue = { schema_version, data: canonical-cbor bytes }
```
目标规则是模块只能读写自己声明且被授予 capability 的 namespace；模块局部
singleton 使用稳定的 instance/entity key。核心位置、身份、所有权、时间、资源
守恒和其他 Kernel invariant 不作为可写 component 暴露。需要改变核心状态时，
模块只能产生经过 capability/policy 校验的 `EffectIntent`，由 Kernel 原子应用
或拒绝；这些 scope、原子边界和拒绝语义在当前 ABI 中尚未实现，不能当作现行
安全保证。
目标扩展中，模块发出的事件必须匹配 manifest 的 event declaration，且完整键为
`namespace:event`；目标宿主负责填充 event id、logical time、caused-by 和当前
manifest hash。payload 按声明的 schema 以 Canonical CBOR 校验。当前
`ModuleEmit` 仍是 JSON payload，legacy adapter、原始 bytes 保留、schema/hash
绑定和 replay 规则都尚未定义；它们是试点前置条件，不能宣称当前 event wire
已经满足该目标。目标版本仍不得原地重写旧事件。
<a id="wasm-capability-permission-kernel-boundary"></a>
### 3. Capability、权限和 Kernel 边界
本小节是目标扩展的权限合同和试点前置条件，不是当前 `CapabilityGrant` 或
`cap_slots` 已经提供的 scope 证明。当前 grant 不能表达下列完整的
namespace/object/entity 作用域；在 ABI、manifest hash 和执行前拒绝路径补齐前，
不能把这些规则作为现行 fail-closed 保证。
- **当前实现边界。** 现行 `CapabilityGrant` 只有全局 `name`、`effect_kinds`
  和可选 `expiry`；`WorldState` 的 capability map 以 grant name 索引，
  `ModuleAbiContract.cap_slots` 只把 slot 映射到这个名称。当前的
  `EffectOrigin`/`ModuleInvocationProvenance` 能记录调用来源类别和模块/Agent
  provenance，但不能证明某个 Agent 对某个 module version、namespace、command
  或 entity 的授权。`module_command_catalog` 也是 active module 声明的只读、
  subject-agnostic 投影；它可以驱动 provider 的 schema 发现，但不是 grant，
  不能仅凭 catalog 或 provider response 执行命令。`allow_all` 只保留为既有
  native effect 的兼容能力，不能成为新 module command 的默认权限。
- **目标声明边界。** command、component read/write、event emit 都必须在
  manifest 中声明所需 `cap_ref`；`cap_slot` 只能解析到 manifest 中唯一且已
  通过治理授予的 capability。模块不能自授予 capability、扩大 entity 范围、
  把 read 提升为 write，或把一个 namespace 的授权转给另一个 namespace。
- **目标最小权限。** capability 是绑定主体、模块身份、对象、操作和资源范围
  的最小治理授予；缺失、过期、冲突、撤销、快照过期或 policy deny，应在进入
  sandbox、计量、journal 或 effect dispatcher 前返回结构化
  `CapabilityDenied`/`PolicyDenied`。拒绝不得应用 state、effect、emit 或部分
  扣费。具体 grant wire、scope canonicalization、签名/receipt 和 replay binding
  必须先版本化，以下定义是目标合同而非当前 wire。
- 目标 host 不暴露真实时间、随机数、网络、文件或任意 I/O。所有外部影响
  必须表达成带 `cap_ref` 的 `EffectIntent`，由宿主 policy 决定是否落账。
- 目标扩展永远不能覆盖 Kernel invariant、绕过资源守恒、跳过 logical time、
  直接移动/写入核心 component，或把制度权限转化为共识/节点权限。制度语义
  仍由其领域治理和试点分册拥有，本合同只定义待实现、可验证的扩展边界。
#### 3.1 目标授权对象：subject、issuer、scope 和 grant identity
下面的结构保留为目标授权语义模型；`CapabilityGrantV2` Rust 类型已在
`oasis7_wasm_abi` 中实现，grant body 使用 Canonical CBOR 编码并以 SHA-256 计算 body hash；
runtime 基于已最终确认的 authority evidence 验证 issuer 与 issuance 的 Ed25519 签名。
类型和这些实现不表示下方每个 target variant 与规则均已支持；当前受支持的 target
及其边界以 runtime 实现和证据为准。
```text
CapabilitySubject =
  Agent { agent_id, owner_binding, generation }
  | Module { module_id, module_version, instance_id }
  | System { system_id, epoch }
CapabilityPresenter = {
  presenter_id,
  presenter_kind,              // provider | agent_client | module | viewer
  session_id?,
  attestation_ref?,
}
CapabilityAudience = {
  world_id,
  branch_id,
  finality_epoch,
  target_kind,                 // world | institution | module_instance
  target_id?,
}
CapabilityIssuer = {
  issuer_id,                 // governed authority, not the calling subject
  issuer_kind,               // governance | kernel_migration | system
  governance_epoch,
  finalized_receipt_id,
  key_id,
  issuer_key_epoch,
  authority_rotation_receipt_id?,
  signature,
}
CapabilityScope = {
  module_id,
  module_version,
  namespace,
  object_kind,                // command | component | event | effect
  object_name,
  operation,                  // execute | read | write | emit | invoke
  entity_selector?,           // exact ids or governed bounded set
  resource_selector?,         // exact resource/type/ledger range
  max_payload_bytes?,
  policy_class?,
}
CapabilityGrantV2 = {
  grant_id,                    // hash of canonical grant body
  grant_version,
  subject,
  audience,
  issuer,
  scope,
  issued_at_tick,
  expires_at_tick?,
  grant_nonce,
  parent_grant_id?,
  delegation_depth,
  revocation_epoch,
  status,
  canonical_body_hash,
  issuance_signature,
}
```
Normative target rules:
1. `subject` answers **who may call** and is not inferred from a payload or from a
   module's self-reported origin. An Agent generation changes on transfer, reset or
   re-claim so that a stale grant cannot follow the identity silently. A Module grant
   binds both immutable `module_id` and exact active `module_version`; an upgrade must
   obtain a new grant unless governance explicitly declares a compatible replacement.
2. `presenter` is the transport or actor presenting a request (for example a provider,
   Agent client, viewer, or module); it is not the `subject` and cannot substitute for
   it. A provider may present an Agent's response only when the host has a separately
   governed binding between presenter and subject. Provider identity, session and
   attestation are audit inputs, never an implicit capability.
3. `audience` binds the grant to one world/institution/module-instance and its branch and
   finality epoch. A grant issued for world A, another institution, or an orphaned branch
   cannot be replayed in world B or a new branch. Audience is part of the scope and of
   every discovery, nonce and receipt hash.
4. `issuer` answers **who was trusted to issue** the grant. Only a finalized governance
   receipt, a narrowly scoped Kernel migration authority, or an explicitly registered
   system authority can issue a root grant. The calling Agent, module, provider, viewer,
   local catalog, pending proposal, or unfinalized block cannot act as issuer. The
   issuer key, `issuer_key_epoch`, governance epoch and authority-rotation receipt are
   part of the signed grant body; a key outside the current trusted epoch fails closed.
   Authority rotation must itself be finalized and cannot silently extend grants issued
   under an older epoch.
5. `scope` is conjunctive, not additive: module identity, namespace, object, operation,
   and every supplied entity/resource selector must match. Omitted selectors mean
   **no authority** for a target grant unless a governed scope policy explicitly
   defines a bounded default; they never mean unrestricted access. Wildcards are
   disallowed for new grants except for a named, audited legacy compatibility class.
6. `grant_id` is the stable audit/replay identity. `grant_nonce` is unique per issuer
   and subject and prevents replay of an issuance or delegation request. A new scope,
   subject generation, expiry, issuer epoch, or parent creates a new grant id; grants
   are immutable and are superseded/revoked rather than edited in place. The canonical
   grant body is deterministically encoded, hashed, and signed by the trusted issuer;
   a database row, provider response, or local grant name is not an issuance proof.
7. `expires_at_tick` uses logical world time and is inclusive only through the exact
   declared boundary. Wall-clock time is never an authorization input. A grant that
   lacks expiry must be explicitly marked non-expiring by governance and remains
   revocable; absence of the field is not an accidental infinite lifetime.
The target issuance receipt for a root or delegated grant must include the canonical
body hash, grant id, subject, presenter (when one initiated the request), audience,
issuer id/key id/key epoch, authority rotation receipt, scope hash, grant/parent nonce,
governance proposal or policy id, `finality_status`, `finality_block_hash` when verified,
branch id, finality epoch, issued/expiry ticks, revocation epoch, and the signed bytes
hash. High-impact scopes (assets, ownership, governance, irreversible institution
changes, or cross-world effects) require a finalized governance certificate and a
non-empty finality block hash; a pending or merely locally observed signer result is
never sufficient. Lower-impact registered system issuance still requires the same
canonical signed body and trusted issuer/key epoch.
The two receipt surfaces are distinct but both are target contracts:
```text
CapabilityAuthorizationRootReceipt = {
  receipt_id,
  grant_id,
  canonical_grant_hash,
  subject,
  presenter?,
  audience,
  issuer_id,
  issuer_key_id,
  issuer_key_epoch,
  authority_rotation_receipt_id?,
  scope_hash,
  grant_nonce,
  parent_grant_id?,
  governance_proposal_id?,
  finality_status,              // pending | verified | reorged | suspended
  finality_block_hash?,
  branch_id,
  finality_epoch,
  revocation_epoch,
  issued_at_tick,
  expires_at_tick?,
  signed_receipt_hash,
}
CapabilityAuthorizationAuditReceipt = {
  receipt_id,
  root_receipt_id?,
  grant_id?,
  subject,
  presenter?,
  audience,
  scope_hash,
  module_id?,
  module_version?,
  manifest_hash?,
  catalog_snapshot_id?,
  response_nonce?,
  authorization_nonce_key_hash?,
  decision,                     // accepted | denied | idempotent | pending
  denial_code?,
  budget_before,
  budget_after,
  world_head_before,
  world_head_after?,
  branch_id,
  finality_epoch,
  finality_block_hash?,
  finality_status,
  state_hash_before,
  state_hash_after?,
  committed_effect_receipt_id?,
  canonical_request_hash,
  canonical_result_hash,
}
```
`CapabilityAuthorizationRootReceipt` proves how a grant entered the authorization
registry; `CapabilityAuthorizationAuditReceipt` proves how one presented request was
checked and, if accepted, atomically committed. Neither receipt is itself a new grant:
the runtime still rechecks the live grant, revocation epoch, branch/finality state and
nonce record for every new request. `budget_before`/`budget_after` are deterministic
world-resource values (including zero where the operation has no budget), never
wall-clock or provider-local estimates.
#### 3.2 Delegation attenuation and revocation
Delegation is an opt-in governance capability, not an automatic property of every
grant. A delegated grant must carry `parent_grant_id` and satisfy all of these
constraints:
- the parent is active, non-expired, non-revoked, and explicitly `delegable`;
- child subject, module identity, namespace, operation, entity/resource selector and
  policy class are a subset of the parent's scope; `read` cannot become `write`, and
  a bounded selector cannot become a wildcard;
- child expiry is no later than the parent expiry, delegation depth strictly decreases,
  and the child nonce is fresh under the issuer/parent pair;
- the child issuer is authorized by the parent grant's delegation rule and cannot
  change the governance epoch, issuer trust root, or grant's audit policy;
- revoking a parent immediately invalidates all descendants, while revoking a child
  does not rewrite or revoke its parent. A delegation chain is replayed and checked in
  canonical parent-to-child order.
The target revocation registry is versioned by `revocation_epoch`:
```text
CapabilityRevocationState = {
  epoch,
  revoked_grant_ids: sorted set<grant_id>,
  superseded_by: map<grant_id, grant_id>,
  trusted_issuer_key_epochs: map<issuer_id, key_epoch>,
  finalized_receipt_id,
}
```
An issuance, delegation, expiry transition, parent revocation, child revocation or
trust-root rotation is an auditable governance event. Revocation takes effect at its
finalized logical tick; a command already committed before that boundary remains a
historical result, but an uncommitted request or a retry must be checked against the
new epoch. If the runtime cannot prove that its revocation state is at least the grant's
required epoch, authorization is denied; stale caches never extend authority. A trust
root/key rotation likewise rejects grants whose issuer key epoch is no longer trusted,
unless a finalized rotation policy explicitly preserves a bounded historical read-only
use. It never preserves new writes or high-impact effects by default.
#### 3.3 Dynamic Agent catalog and provider response binding
The Agent/provider surface is a two-step contract. Discovery is an observation;
execution is a fresh authorization decision. **Current/partial:** snapshot/response
types, subject binding, provider/loopback validation and selected-entry mismatch
rejection exist; automatic production context injection and real Agent E2E are not
proven. **Target:** host catalog carries trusted descriptor and exact instance target,
and provider returns typed args for host encoding. Discovery never becomes authority.
```text
CapabilityCatalogSnapshot = {
  snapshot_id,                 // hash of canonical snapshot
  world_id,
  world_head,
  branch_id,
  finality_epoch, finality_block_hash,
  logical_tick,
  module_registry_hash,
  policy_hash, policy_epoch,
  revocation_epoch,
  subject, presenter, audience,
  entries: [
    { module_id, module_version, namespace, command, schema_version,
      schema_hash, max_payload_bytes, instance_target,
      eligible_grant_ids }
  ],
  valid_until_tick,
}
AgentCommandResponse = {
  response_nonce,
  subject, presenter, audience,
  catalog_snapshot_id,
  selected_entry,              // exact module/version/instance/namespace/command/schema
  typed_args,                  // target: host validates against schema_hash
  envelope?,                   // current compatibility lane: raw envelope
  provider_id?,
  trace_id,
}
```
`instance_target` is mandatory for a module-instance audience and carries stable `(world_id,
module_id, instance_id)`; a governed non-instance command uses an explicit variant, never global
fallback. Snapshot hash and bound nonce/context/receipt cover world, branch, finality block/epoch,
policy hash/epoch, revocation epoch, subject/audience, entries and validity.
Catalog entries intersect active declarations, the subject's grants and audience policy.
Verified revoked/superseded grants are excluded without hiding unrelated valid grants;
history remains persisted. Corrupt signature/authority/proof or unexplained conflicts
still fail closed; see [discovery validation](../module/module-lifecycle.md#revoked-grant-discovery).
Sorted entries bind `module_registry_hash`, policy hash, branch, finality and revocation
epochs. `eligible_grant_ids` is an audit hint, not a bearer token or new authority.
Providers may derive dynamic schemas and propose a response, but cannot add commands,
change subject/audience, substitute grants or issue scope. Selected entry, envelope
schema, subject, presenter binding, audience, snapshot id and response nonce must all
match the request; execution independently rechecks live authority below.
The current response may carry a raw `ModuleCommandEnvelope`; the trusted host treats it
as compatibility input and re-runs declaration/hash/size checks. Once `typed_args` exists,
only the host encoder produces payload bytes. Provider cannot choose another descriptor,
add undeclared fields, widen constraints, or use another instance. Missing descriptor,
stale registry, ambiguous field or encoding error is a structured denial; no JSON/CBOR
guessing fallback exists.
At execution time the host must re-read live state and validate, in this order:
1. canonical response/envelope encoding, fresh response nonce, subject generation,
   presenter-to-subject binding and exact audience;
2. catalog snapshot existence, `world_head`/policy hash compatibility and
   `valid_until_tick`; branch/finality status must be verified. A policy may permit a
   bounded head drift only when that rule is explicit and included in the signed
   snapshot; it may not permit an unverified branch or reorged finality evidence;
3. active module record, exact module/version, declaration, schema hash and payload
   limit;
4. issuer trust, grant signature, parent chain, scope intersection, expiry and current
   revocation epoch/list;
5. current policy, entity/resource ownership and one-use/replay nonce state;
6. only then enter metering, WASM sandbox, staged state/effect/event application and
   receipt generation.
Any mismatch returns a structured denial with the earliest failing reason. It must not
fall back to a static Action, a legacy global grant, the provider's claimed schema, or
the stale catalog. This preserves existing closed `ActionCatalogEntry` tools while
allowing a new dynamic module-command tool lane to be added incrementally.
The persistent authorization nonce key is the complete tuple below; a process-local
nonce cache is not an authority:
```text
AuthorizationNonceKey = {
  subject,
  issuer_id,
  issuer_key_epoch,
  grant_id,
  audience,
  branch_id,
  finality_epoch,
  nonce,
}
AuthorizationNonceRecord = {
  key,
  request_hash,
  outcome_hash,
  committed_receipt_id?,
  state,                         // reserved | committed | aborted
}
```
For one key, the same canonical `request_hash` is idempotent and returns the original
receipt (or the same pending status); a different hash is a deterministic
`NonceConflict` and cannot execute a second request. The nonce record, budget
reservation/debit, module state, world effects, emitted events and authorization
receipt must commit in one staged transaction. A failed or aborted transaction rolls
back all of them, including the nonce reservation and budget; a committed retry reads
the durable receipt instead of re-running WASM. Recovery resolves `reserved` records
from the journal before accepting new work.
#### 3.4 Governance, finality, persistence, replay and audit
- **Issuance/finality:** governance creates `CapabilityGrantIssued` or
  `CapabilityGrantDelegated` only after the applicable proposal, signer/quorum and
  finality receipt are valid, and the canonical grant body has a valid trusted issuer
  signature. A pending governance action can be displayed as a preview but cannot enter
  the active authorization registry. During a network partition, an unverified branch,
  or a reorg before finality, the grant is `pending`/`unverified` and may only be shown
  as a non-authorizing preview; it cannot be used for a command, budget debit or effect.
  If its finality block is orphaned, the grant becomes `reorged`/`suspended` and requires
  re-issuance or a fresh finalized proof on the canonical branch. Consensus finality
  confirms the governance event; it does not turn a module command into a consensus
  primitive or grant node/validator authority.
- **Persistence:** snapshots must include the active immutable grant records, grant
  parent links, issuer trust-root epoch, revocation epoch/list, supersession map and
  consumed issuance/delegation/execution nonces and durable budget state. The snapshot
  also records the `module_registry_hash`, policy hash, branch/finality epoch and the
  last finalized governance receipt and finality block hash used to construct its
  authorization view. A catalog snapshot may be retained as an audit object, but it is
  never the sole source of authority.
- **Replay:** replay reconstructs the grant registry and revocation state from
  governance events up to the historical world head, then validates each command
  against that historical state. It never consults today's grant list or current
  revocation epoch to reinterpret a committed historical receipt. A response/catalog
  hash is replay evidence, not a grant; an uncommitted response is discarded after
  recovery and must be re-discovered. A branch or finality proof that was pending,
  unverified, or orphaned is not promoted during replay. Historical committed receipts
  retain their original branch/finality block hash and status; they are not silently
  replayed as new canonical authority.
- **Audit events:** the target event set is
  `CapabilityGrantIssued`, `CapabilityGrantDelegated`, `CapabilityGrantRevoked`,
  `CapabilityTrustRootRotated`, `CapabilityCatalogPublished`,
  `CapabilityAuthorizationChecked`, `CapabilityAuthorizationDenied` and
  `ModuleCommandSubmitted`. Each event binds `grant_id` (when applicable), subject,
  presenter, audience/world/branch, issuer/key epoch, scope hash, catalog snapshot id,
  response nonce, nonce-key hash, module/manifest hash, logical tick, world head,
  finality epoch/block hash/status, budget before/after, and caused-by/trace id.
  The authorization/audit receipt additionally records the canonical grant/response
  hashes, decision, denial code or committed receipt id, and the state/effect/charge
  result. Denial events contain a structured reason and no payload secrets. An audit
  append is transaction-neutral; failure to append it cannot turn a denied request into
  an accepted one.
- **Fail closed:** unknown issuer, malformed signature, missing or stale revocation
  state, expired/revoked/superseded grant, invalid parent chain, subject mismatch,
  presenter mismatch, audience mismatch, scope widening, stale snapshot, nonce reuse
  or hash conflict, inactive module, schema/hash mismatch, unverified/reorged finality,
  unavailable policy, exceeded limit/budget or unavailable audit/journal authority all
  deny before any world mutation. The only permitted observable result is a bounded,
  structured denial/audit record; retries require a new response nonce and fresh
  discovery.
#### 3.5 Forbidden authorization shortcuts
The following are explicitly forbidden in the target implementation and are not
acceptable substitutes during migration:
- `CapabilityGrant::allow_all`, `World::add_capability` or any global
  grant-map mutation may not mint, widen, or refresh an actor-scoped v2 grant. These
  APIs remain legacy native-effect compatibility only until their declared migration
  cutoff.
- `EffectOrigin`, `ModuleInvocationProvenance`, a provider attestation, process identity,
  session cookie or caller-supplied `subject` is provenance, not proof of issuer,
  grant, scope, audience, finality or revocation status. Host provenance must be
  cross-checked against the persistent authorization registry.
- Process-local catalog, grant, revocation, budget or nonce caches may accelerate a
  lookup only when their digest and epoch are checked against durable world state. They
  cannot authorize during a partition, after expiry/revocation, or when the durable
  source is unavailable; stale/unknown cache state denies.
- A provider response, `ActionCatalogEntry`, static fallback action, receipt signature,
  consensus block, or module manifest alone cannot authorize a high-impact effect.
  Each still requires the live subject/issuer/scope/audience/nonce and finality checks.
- No implementation may reserve a nonce, debit budget, or append an authorization
  receipt separately from the staged world effect. Partial success is a denial/recovery
  case, not an accepted command.
#### 3.6 Migration from the current global `CapabilityGrant`
Migration must be explicit and one-way for new authority. A migration adapter may read
the current global map during a bounded compatibility window, but it must not silently
upgrade a broad grant into an actor-scoped grant:
1. For each legacy grant, governance emits a `CapabilityGrantMigrated` event containing
   the old grant name/hash, a synthetic `kernel_migration` issuer, the migration epoch,
   and an immutable `LegacyGlobal` subject. The old `effect_kinds` become exact legacy
   effect scopes; `expiry` maps to logical `expires_at_tick`.
2. `cap_slots` are resolved to the migrated grant id for existing native effect paths.
   A legacy `*`/`allow_all` grant remains restricted to the pre-existing native effect
   compatibility class. It cannot satisfy a new module command, component, event or
   Agent/provider request.
3. New module declarations must reference `CapabilityGrantV2` ids and exact scopes.
   An Agent or module that still presents only a legacy name receives a structured
   migration-required denial; there is no implicit subject or issuer inference.
4. Snapshot/replay supports both formats only until the declared migration cutoff.
   Before cutoff, old events remain readable through the adapter and new issuance is
   dual-recorded with the v2 grant id; at cutoff, the active legacy map is frozen and
   all remaining authority requires finalized v2 issuance. Historical legacy receipts
   remain queryable and are not rewritten.
5. Migration acceptance requires proving that no migrated grant gains a wider effect,
   namespace, entity, resource, expiry or delegation scope than its source grant, and
   that a revoke/expiry in either representation cannot leave a live alias.
#### 3.7 Authorization acceptance criteria
The following IDs are testable gates for an authorization implementation; they are
target acceptance criteria, not current test claims:
| ID | Acceptance criterion | Required tier |
| --- | --- | --- |
| AUTHZ-01 | A valid Agent/Module/System subject and finalized trusted issuer produce a stable grant id; tampered issuer, signature, nonce or grant body is rejected. | `test_tier_required` |
| AUTHZ-02 | Scope matching is conjunctive and exact across module id/version, namespace, object, operation and entity/resource selectors; omitted or widened selectors fail. | `test_tier_required` |
| AUTHZ-03 | Parent delegation can only attenuate scope, expiry and depth; parent or child revocation invalidates the correct descendants without mutating historical receipts. | `test_tier_required` |
| AUTHZ-04 | Expiry, revocation epoch/list, trust-root rotation and stale registry state fail closed at execution time. | `test_tier_required` |
| AUTHZ-05 | Dynamic catalog entries are filtered by the current subject and bind module registry, policy and revocation hashes; catalog exposure alone grants no authority. | `test_tier_required` |
| AUTHZ-06 | Provider responses must bind subject, exact selected schema, catalog snapshot and fresh nonce; forged/unknown/stale responses are rejected before sandbox or journal mutation. | `test_tier_required` |
| AUTHZ-07 | A live recheck catches grant revocation, module deactivation, policy change and entity/resource ownership change between discovery and execution. | `test_tier_required` |
| AUTHZ-08 | Governance issuance requires finalized authority; pending proposals and consensus receipts without governance grant semantics cannot authorize module effects. | `test_tier_required` |
| AUTHZ-09 | Snapshot/restart/replay preserves grant, parent, revocation, trust-root and nonce state and reproduces the same accept/deny decision and hashes at the same historical head. | `test_tier_full` |
| AUTHZ-10 | Legacy migration is scope-preserving; `allow_all` remains native-only, new module commands require v2 grants, and the cutoff rejects unmigrated aliases. | `test_tier_full` |
| AUTHZ-11 | Every issuance, delegation, revocation, catalog, check, denial and command submission has deterministic audit bindings without leaking payload secrets. | `test_tier_full` |
| AUTHZ-12 | Any missing policy, issuer, revocation, journal or audit authority fails closed with no partial state/effect/charge; accepted commands enter the same staged transaction and receipt path. | `test_tier_full` |
| AUTHZ-13 | Presenter/provider, subject and audience/world are distinct; a provider cannot substitute identity, audience, grant or scope, and cross-world/branch replay is rejected. | `test_tier_required` |
| AUTHZ-14 | Issuance is deterministic canonical signing bound to issuer key epoch and finalized authority rotation; high-impact grants require a verified finality block hash. | `test_tier_required` |
| AUTHZ-15 | The persistent nonce tuple `(subject, issuer/key_epoch, grant, audience, branch/finality epoch, nonce)` is idempotent for the same request hash and rejects a different hash without mutation. | `test_tier_required` |
| AUTHZ-16 | Nonce reservation, budget before/after, world effects, module state, events and authorization receipt commit atomically; crash/recovery resolves reserved records deterministically. | `test_tier_full` |
| AUTHZ-17 | Partition, pending/unverified finality and reorg/orphaned branch evidence are preview-only or suspended; no unverified grant can authorize a command or high-impact effect. | `test_tier_full` |
| AUTHZ-18 | `allow_all`, `add_capability`, provenance, process-local caches, provider responses and consensus receipts cannot bypass live v2 authorization. | `test_tier_required` |
### 4. 确定性计量和执行收据
执行目标上必须同时受 WASM fuel/`ModuleLimits.max_gas` 和资源计量 schedule
约束。本节的 schedule identity、扩展访问计量和 execution receipt 都是试点
前置条件，不是当前 `wasm-1` wire/journal 已有的 receipt contract。
当前 executor 基线在其既有计费路径中使用如下确定性费用口径；这段公式不等于
开放 component/schema 扩展已经获得同一 schedule，也不为未来访问费用冻结 ABI：
```text
compute = ceil(input_bytes / 1024)
        + ceil(output_bytes / 1024)
        + 2 * effect_count
        + emit_count
electricity = 1 + effect_count + emit_count + has_new_state
```
若试点增加 component read/write 或 schema decode 费用，必须先定义新的、明确
命名的 `metering_schedule_version`，并将其加入版本化 manifest/receipt/journal
合同；当前代码没有该字段。费用只能由 canonical 字节长度、声明的访问数量、
WASM fuel、effect/emit 数等确定性输入计算，不得使用 wall-clock 耗时或未排序
的集合。目标 execution receipt 至少绑定 module/instance、manifest hash、
`wasm_hash`、schedule version、输入/输出大小、fuel/费用、effect/emit 数、
状态前后 hash 和 journal position；该 receipt schema、事件承载和回放规则必须
在试点实现前补齐。
目标上，余额或 fuel 不足必须在提交前返回结构化拒绝；成功调用才产生计费和
状态/事件变更。回放应消费已记录的执行结果与计量收据，不重新按当前价格或
当前 schedule 计算历史费用；新 schedule 只影响新 manifest/新执行。当前
`ModuleRuntimeChargeEvent` 尚未承载这些 identity/receipt commitments，故不能
把上述收据语义报告为已实现。
### 5. 安装、升级、禁用和迁移
开放对象的 install/register、activate、upgrade、deactivate **目标上**必须与
manifest、artifact identity 和 `ModuleChangeSet` 同一治理闭环绑定。以下条目
是试点前置条件；当前 lifecycle wire 没有完整的 migration identity 或 atomic
batch 语义，不能把本节当作现有操作的保证：
- 目标 install 只接受已计算并重新校验的 SHA-256 `wasm_hash`，以及发布链提供的
  artifact identity/build receipt；本地任意源码编译或上传不会自动激活模块。发布
  链字段、验证顺序和错误事件必须与现有 release authority 对齐。
- 目标 upgrade 必须引用当前 `base_manifest_hash` 和 active version。保持同一
  ABI/schema 的兼容升级可复用已有 state；若 component/event schema hash
  改变，manifest 必须声明对应对象类型的确定性 `SchemaMigration`，否则拒绝
  升级。也可以注册一个新的完整 event key，让旧 event 保持原 schema；这些
  from/to/hash/migration 绑定当前尚未进入 `ModuleUpgrade` wire。
- 目标 disable 在逻辑 tick 边界停止新调用，但保留 module state、旧 manifest、
  artifact hash 和完整事件历史；禁用不是删除，也不能让回放选择另一版本。旧
  版本保留和 replay 选择规则必须先有 lifecycle event/snapshot 证据。
- 目标 apply 失败是原子的：不写半套 register/activate/upgrade/deactivate 事件，
  不更新 active registry，不覆盖旧 artifact/state。实现前必须提供 journal
  transaction 或等价 staged commit、failure injection 和 replay fixtures；不能
  从 `module-lifecycle.md` 的现有事件排序直接推导出该原子性。
目标迁移声明至少绑定对象类型、`namespace/name`、from/to schema version/hash、
迁移工件 `wasm_hash` 和迁移 manifest hash。component 迁移函数必须是确定性、
受限、无外部 I/O 的纯 state 转换，只能产生目标 namespace 的 state；event
迁移只能把历史 payload 转成已声明的 reducer 输入，不能伪造 caller、time、
caused-by 或 Kernel event。以下是目标 event shape，不是当前 `WorldEvent` 的
已实现类型；提交前必须定义其 wire、原子写入和 replay 绑定：
```text
ModuleStateMigrated { // target shape; not a current event type
  instance_id, entity_id, namespace, component,
  from_schema_version, to_schema_version,
  from_state_hash, to_state_hash,
  migration_wasm_hash, manifest_hash, caused_by,
}
```
目标回放先按旧 schema 重放历史事件，到迁移边界应用 state/event 的声明迁移，
再按新 schema 继续；历史 event/state bytes 不重写。没有 event migration 时，
旧 event 必须按其原版本交给仍声明支持它的 reducer，不能静默按新 schema
重新解释。迁移失败、hash 不匹配或输出超限时应保留旧版本并结构化拒绝。
这些规则和 manifest 自身的版本迁移（包括 `ManifestMigrated` 语义）都必须在
试点前与现有 lifecycle authority 对齐并提供 from/to hash 证据，不能由本节
单方面冻结。
### 5.1 兼容性、fail-closed 与迁移证明
兼容性按 wire surface 和证据等级记录；“有默认值”不能代替迁移证明：
| Surface | 当前兼容行为 | 目标证明 |
| --- | --- | --- |
| `ModuleManifest` / `ModuleAbiContract` | 缺省 ABI contract、declarations 与可选 artifact identity 可读取历史 manifest；未知字段不能改变既有 hash/admission 语义。 | 用旧 manifest fixture 做 decode、canonical re-encode、admission 和 snapshot/replay，证明 `module_id`、version、`wasm_hash` 与 manifest hash 不漂移。 |
| `ModuleContext` / `ModuleOutput` | 缺失 host caller 与可选 `tick_lifecycle` 的旧 wire 仍可解码；legacy caller 不能被当作新 subject 授权，tick 缺省按 suspend 处理。 | 旧模块 golden bytes 在新 host 上得到同一结果；新字段只在明确的 ABI version/manifest admission 下启用。 |
| `ModuleCommandEnvelope` | 外层 Canonical-CBOR、声明 identity 和 payload bound 有 proven 检查；payload 内容仍是 opaque，raw envelope 是兼容路径。 | descriptor registry、typed-args host encoder、semantic decode 与 schema hash 绑定必须有正反 fixture；未知 schema/version/encoding 一律拒绝。 |
| module lifecycle / state | install、activate、upgrade、deactivate 与 instance/tick snapshot persistence 已有切片；成功 tick suffix replay 依 [宿主合同](../runtime/runtime-integration.md#module-tick-journal-replay)，完整 migration atomicity 与 receipt 仍 partial/target。 | 从 `base_manifest_hash` 到 from/to schema、migration `wasm_hash`、新 manifest hash 的单 journal 证明；失败注入后旧 registry/state/artifact 完整保留；tick 另验非空 suffix 与续跑。 |
下列身份不可互换：`wasm_hash` 是工件 bytes 内容地址，`manifest_hash` 是 manifest 控制面 hash；当前 `schema_hash` 只是 declaration marker，target registry 落地后才是 descriptor bytes 内容地址；`artifact_identity` 绑定
source/build manifest/signer/signature。任一 identity、ABI version、权限、limits 或
metering schedule 变化都必须重新 admit；不能同名覆盖、只改版本字符串或依赖 provider
声明维持兼容。
迁移证明最小闭环是：保留旧 bytes；新 bytes 由显式 from/to schema 和受治理 migration
artifact 产生；migration 输出、state/event 前后 hash、manifest hash、instance target
和 journal position 可回放复算。任一 hash、descriptor、权限、artifact、实例或
finality 不可证明时，在 sandbox、计量、journal/effect commit 前结构化拒绝；历史
event/state 不原地改写，失败不得部分扣费、写 state、发 emit 或覆盖 active module。
### 6. 试点验收与非目标
一个 Alliance 或 EconomicContract 试点在进入实现前，至少需要完成下列
前置条件，并用代码、ABI fixtures、journal/replay evidence 证明；这些条目是
目标合同的 admission gate，不是对当前 runtime 已支持能力的陈述：
1. 一个 namespace 能声明 command/component/event schema，并以 canonical
   envelope 被发现、校验、调用和拒绝未知版本。
2. capability 不足、Kernel component 写入、fuel/limits 超限、余额不足和
   artifact/manifest hash 不一致均 fail closed，且无部分 state/effect/event。
3. register/activate/upgrade/deactivate、state migration、snapshot/replay
   在同一 journal 上得到一致 state/event/hash/计量收据。
4. 既有 native vertical slice、旧 `wasm-1` manifest 和未参与试点的模块行为
   保持兼容；若试点接收既有 `ModuleEmit`，必须先提供版本化 legacy JSON
   adapter，明确原始 bytes、schema/hash 和 replay 处理，不能以“兼容槽位”代替。
本试点不承诺全面 ECS、删除 `tick_consensus`、把所有制度迁入 WASM、动态
  world database、分片，或接受任意未治理上传；这些仍需独立的 runtime、
  consensus、产品和安全评审。
