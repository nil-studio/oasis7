---
name: oasis7-play
description: Execute an authorized Oasis7 native task through the signed game client, read the complete private goal and captured context, choose a supported action, and verify its canonical result. Use for the prepared-world task execution profile with an operator-provided device configuration.
---

# Oasis7 native task play

This profile uses the ordinary `oasis7_game_client` executable and an operator-provided device configuration. It supports task discovery, complete private goal/context capture, `native.move_agent`, `native.transfer_material`, built-in `native.build_factory` and `native.schedule_recipe` with signed quotes, authenticated industry observation, original result lookup and terminal provider outcomes. It does not provide cold world entry, token pairing, the full Game HTTP write catalog.

Use only the supplied executable and device configuration. Do not inspect owner keys, server storage, fixture source or other players' state. Never print signer files or copy private goal content into public action reasons. Keep full goals and observations inside the authorized model context.

The native identity prerequisite has a separate [owner-control reference](references/identity-entry.md).
It is not a cold-world play profile. Default executors must not invoke it, unlock
identity storage or obtain owner credentials. A missing world/Agent/task binding
still requires stopping at that boundary; identity registration alone cannot
authorize the commands below.

## Executor selection

An operator may select a built-in consumer through the trusted launcher BYOK
session entry. That entry must authenticate the owner and bind a legitimate
GameClient executor before using the same signed SDK. A model connection is not
world authority, and missing bindings must not be reported as a started worker.
Real-model play through this entry remains separately authorized work.

Do not run CLI effects concurrently with another worker for the same Agent.
Switching executors requires the owner-controlled fence and recovery of original
operations; never replace an unknown request or increase the same run's immutable
budget. The CLI instructions below apply when you are the authorized executor,
with no competing worker. They do not authorize paid inference or establish
complete built-in/Runtime parity.

## Discover and capture

Run the supplied executable with `--help`, then:

```sh
<client> --config <device.json> describe
<client> --config <device.json> task
<client> --config <device.json> capabilities
<client> --config <device.json> industry
<client> --config <device.json> context --operation <unique-stable-context-id>
```

Choose a unique operation ID once per decision context. Retain it across transport errors and restarts. The client durably records the original signed preparation and public prefix before sending. Do not substitute another nonce or operation after an unknown result.

If context returns `usable_context=false`, inspect its original canonical outcome. Pending or unknown outcomes require original lookup/retry. A settled rejection or expiry cannot become a usable context by repeating it: preserve that result and reread the task. A committed prefix can still occupy the active turn after its private context expires; use the explicit expired-context recovery below before creating a new decision context. A changing world may invalidate a captured base between preparation and submission.

A context is usable only after its prefix is canonically committed. Read the returned complete goal, soft constraints, current task/executor fence, observation schema and actual capability catalog. `world-agent-state/v1` is raw Agent state, not the legacy ProviderObservation format. Derive your decision from this goal and observation. Contexts have a bounded tick lease, at most 16 ticks and never beyond the capability grant expiry; refresh promptly before making a new decision.

## World-rule orders, supply and organization cases

The signed client also exposes `native.world_rule` through the current context and task fence. Read `industry.world_rule_status` for order/case/authorization identities, revisions, committed business receipts and reasons. An existing native delegation requires an explicit `WorldRule` grant for the command's exact factory/order/authorization/batch/case object. An electricity grant never replaces governance approval, inventory, industrial capacity or estate authority.

```sh
<client> --config <device.json> world-rule --context <context-id> --request <stable-business-id> --command '{"kind":"ConfirmAgentOrder","data":{"factory_id":"<owned-assembler>","quoted_at":<quote-tick>,"quote_digest":"<quote-digest>"}}'
```

Use only typed commands supported by the current server and the authorized goal. Retain the exact context, request and JSON across unknown responses; `lookup --kind action` recovers the signed operation. A receipt confirming an order or allocation is not Agent delivery or material arrival. Read the order's delivered time and claim, or the restricted-power balance and actual production receipt, after authoritative progress. Missing policy, stale revision, pending protection/appeal or failed resource checks remain blocked; do not switch IDs to bypass them. Governance commands require a passed proposal whose description is the canonical `world-rule-v1:` JSON binding generated by the shared API; proposing a command does not approve it. Organization recovery requests remain separate from the protection stage and do not restore control while pending.

For an estate operation that requires `effect_ref`, first run `<client> --config <device.json> world-rule-key --request <stable-business-id>`. This local read returns the exact effect identity for the configured Agent and request. Include it in the command before obtaining any required governance approval, then submit that same request and JSON. Computing a key grants no estate authority, reserves nothing, and cannot replace the committed receipt. Changing the request after approval changes the effect identity.

## Recipe substitution

Read `industry.world_rule_status.recipe_substitution` before choosing a substitute. It is private to the current economic owner and supplies complete preview payloads, declared risks/value units, exact root/slot identities and next recheck times. `unknown` is missing or stale authority; `no_legal_substitute` is a completed finite search. Truncation is not proof that omitted choices do not exist.

The three choices are distinct: `same_material` changes a qualified source without a converter; `profile_substitute` requires an explicit rule and a separate actual conversion; `new_recipe` references a certified fresh candidate with zero inherited work. Do not invent source qualification, ratios, prices, finality certificates or profile rules. Batch import and root/authority publication require operator-supplied certified facts; an executor grant alone cannot create them.

Use the existing `world-rule` CLI form with `kind: "RecipeSubstitution"` and nested `data.command.operation`. `confirm` carries the exact returned preview, selected `candidate_digest`, explicit `scope` (`decision_only` or `decision_and_exact_conversion`), `maximum_source`, `maximum_costs`, `maximum_power` and the published cutover plan digest when applicable. Choose the scope from the authorized goal; recording a choice never authorizes an arbitrary conversion. Copy certified quantities and limits from the chosen candidate instead of filling missing values. Each command still needs a current `WorldRule` grant for its exact root or decision object.

A confirmed decision consumes no materials and provides no production progress. A separately authorized `convert` must return the original linked settlement before `apply_binding` can use converted input. Same-material binding has no conversion receipt. A new recipe uses the full `cutover_plan` and digest published in the preview, followed by its normal `IndustrialWork/start_root`; it never inherits old receipts, stock reservations or work. Do not compute hidden hashes or construct substitute plans from source code. Unsupported converter modes or post-sink dispositions remain blocked.

After any write, recover the original request and reread industry. Check full qualified identities, actual escrow consumption, the independent settlement and ordinary pending/completed recipe results. A decision, start receipt or successful transport response cannot prove conversion or production. Preserve stable request/context/payload on unknown results; use a fresh preview only after the original result is known. The currently supported native post-sink `terminate_root` is available only when explicitly declared by the accepted baseline; it preserves consumed-input and power loss and never refunds or completes production.

## Decide and act

The movement CLI proposal is movement of the configured Agent to three signed i64 centimeter coordinates:

```sh
<client> --config <device.json> move --context <context-id> --to <x_cm,y_cm,z_cm>
<client> --config <device.json> lookup --context <context-id> --kind action
```

Choose coordinates yourself to satisfy the actual goal. The runtime still checks authority, capability admission, current task/goal/executor fences, base binding and available resources. A submitted or pending request is not a successful world effect. Inspect the canonical outcome and action receipt, and read a new context to check the resulting position when another decision is needed.

For a material-delivery goal, read `industry` before choosing source/destination ledgers, material, route path and amount. The authenticated read includes actual material ledgers, route availability/capacity, factories, pending transits and recipe jobs; each collection is capped at 128 entries and reports truncation. Do not treat omitted entries as absent. This command is an observation, not a signed cost quote.

```sh
<client> --config <device.json> transfer --context <context-id> --from <ledger-id> --to <ledger-id> --material <kind> --amount <positive-i64> --distance-km <nonnegative-i64> --priority <standard|urgent> --routes <route-id,...|direct>
```

Ledger IDs are `world`, `agent:<id>`, `site:<id>` or `factory:<id>`. Routes are an ordered path of at most 16 distinct IDs; `direct` requests a transfer without a registered route path. Automatic rerouting is disabled. The requester is always the configured Agent. Choose arguments from the actual goal and industry state; the runtime checks ledger ownership, route continuity/availability, inventory, power and native grants. A committed admission may only start a transit: reread industry after world progress to verify destination delivery.

`capabilities` separates local CLI proposal forms, the module command catalog and native action/object electricity grants. Inspect grant status, action kinds, object scope and remaining units. Native authorization is object-specific: a `BuildFactory` grant authorizes its exact `grant.object_id` as the new factory ID, not arbitrary names or a factory kind. Choose an unbuilt ID from a current matching grant; `ScheduleRecipe` also requires its current factory-object grant. Never invent a grant or infer object authority from the module catalog. A module catalog entry or a current native grant alone does not guarantee action admission. Agent signer delegation is a separate identity boundary. Generic module proposals and custom recipe plans are not supported by this CLI.

For a factory-construction goal, read `industry` and choose a ready site authorized for the configured Agent at its current authorized location. Inspect the site's location anchor, construction electricity profile, Agent material ledger and power. Collection truncation means omitted entries are unknown. Only `factory.smelter.mk1` and `factory.assembler.mk1` are supported factory kinds; the builder is always the configured Agent.

```text
<client> --config <device.json> quote-build --context <context-id> --site <site-id> --factory <authorized-factory-id> --kind <factory.smelter.mk1|factory.assembler.mk1>
<client> --config <device.json> build --context <context-id> --site <site-id> --factory <authorized-factory-id> --kind <factory.smelter.mk1|factory.assembler.mk1>
```

Read the signed quote's rejection or construction preview, material ledger/costs, power requirements and duration. A quote checks current admission without reserving inventory, power or delegation budget. `build` obtains a verified quote on its first submission and uses the authoritative factory spec and same returned public context. Do not inject a spec, costs, power obligation or grant. Committed admission or a pending construction job does not mean the factory is ready: after world progress, reread `industry` and verify the completed `FactoryBuilt` result and actual factory before transfer or scheduling. Capture a new context for that next decision.

A verified, signed read-only quote rejection is decision feedback with no submitted action. Read its safe rejection code, inspect current capabilities and industry, and correct the selected arguments while the same context remains usable. Do not blindly retry the denied selection. This is a new model decision and consumes the same run's remaining model budget; it does not renew the context lease, increase permissions or reset budgets. A built-in worker stops the rest of that model response's tool batch after such a rejection and returns results for skipped calls before asking the model to decide again. If the corrected selection needs permissions absent from current grants, ask the owner to renew or stop.

An opaque HTTP 403, failed signature/binding validation, changed task/executor fence, expired context or unknown transport result is not this safe rejection feedback. Preserve any original operation and follow its recovery rules; do not reinterpret an authentication failure as permission to try another object. A rejected quote itself is not an action receipt or a successful world effect.

If the built-in executor reports `input_limit`, the complete required model input cannot fit its configured capacity. This is not a rejected world action or permission to replace an operation. Stop new inference, retain the original context/action and inspect its original result before resuming. Do not repeatedly Start, change the operation ID to evade the limit, raise the same run's budget, or discard required goal, observation, tool-result or protocol history. Capacity and history configuration must be corrected by the execution host; a resumed executor must recheck the current task, fence and original results with its remaining budget.

A built-in `model_failed` report may include a fixed `model_failure` classification. `output_limit` means the generated response was truncated: no part of that response is a valid action, even if a partial tool call looks complete. Preserve original operations, stop new inference and have the execution host review the configured output capacity within explicit limits. Model-identity, unsupported-tool, invalid-tool-identity and unsupported-protocol failures require diagnosis; do not guess actions from text, change models/protocols, raise the run budget or blindly restart. The classification describes a model failure, not a world action receipt.


Both build commands recover a cached original Action before any fresh context or quote: the same site, factory ID and canonical kind must match. Unknown/pending results freeze that exact signed operation, including after a process restart. Recover with `lookup --context <context-id> --kind action`; do not replace it with a new build or recipe. This profile assumes an already claimed and configured Agent with an authorized site. Factory construction does not establish cold entry, claiming, token pairing, bootstrap or full AC-EXT-010 coverage.

For a production goal, select a factory owned by the configured Agent and an exact built-in recipe ID from the current goal and industry state. The eleven IDs are `recipe.smelter.iron_ingot`, `recipe.smelter.copper_wire`, `recipe.smelter.polymer_resin`, `recipe.smelter.alloy_plate`, `recipe.assembler.gear`, `recipe.assembler.control_chip`, `recipe.assembler.motor_mk1`, `recipe.assembler.logistics_drone`, `recipe.assembler.sensor_pack`, `recipe.assembler.module_rack` and `recipe.assembler.factory_core`.

```sh
<client> --config <device.json> quote-schedule --context <context-id> --factory <factory-id> --recipe <canonical-recipe-id> --batches <positive-u32>
<client> --config <device.json> schedule --context <context-id> --factory <factory-id> --recipe <canonical-recipe-id> --batches <positive-u32>
<client> --config <device.json> lookup --context <context-id> --kind action
```

Read the signed quote's rejection or start preview, input/output ledgers, materials, byproducts, electricity and duration. A quote checks current canonical production and grant admission without reserving materials, power or budget. It is conditional on its signed read binding; it does not guarantee later admission. `schedule` obtains a verified quote on the first submission and uses its authoritative plan and context. Do not supply a raw plan, costs or grant. A started recipe is not completed production: reread `industry` after world progress to verify finished output. Quotes require a usable captured context; if a quote reports changed state, preserve any original action and use the ordinary context recovery rules.

If `schedule` or `quote-schedule` finds an original cached action, it checks the same factory, recipe and batches and queries that original result before reading another context or requesting another quote. Unknown or pending results keep the original operation; they do not create a replacement action. This lookup also works after a process restart. Use `lookup --kind action` to recover a lost submission response.

Each context has one original action shared by movement, transfer, construction and production. Retry the exact same action arguments and context to recover that exact result. Changing the action kind, route order, ledgers, amount or other arguments conflicts and leaves the original intact. After a committed action, capture a new context with a new operation ID before another decision.

If the original action is canonically rejected or expired, retain that result. Rejection or lease expiry does not necessarily release its active context: recover the original terminal result and explicitly settle an expired active turn when allowed below before capturing a new decision context. Never treat rejection as an effect or create a replacement operation while the original outcome is unknown.

## Recover and finish

For a lost response, use `lookup --kind prefix|action|outcome` with the original context ID. Preserve rejected and expired results as evidence; do not report them as successful actions. For a current usable context that cannot produce an action, terminate with a fixed public reason:

```sh
<client> --config <device.json> outcome --context <context-id> --reason executor_aborted
```

For a usable context, the other supported reasons are `provider_failed` and `transport_exhausted`. Do not place private explanation text in public reasons. An outcome can itself be rejected when the world advances beyond its captured base. This CLI preserves that immutable original outcome; it does not automatically re-sign it with a fresh base. Query its original result; do not create a competing outcome with another reason or base. Expired contexts cannot be decrypted through this CLI. Grant expiry requires owner renewal or Stop, not unauthorized reuse.

For an expired context whose original prefix committed but no action or outcome was created, first recover its original results:

```sh
<client> --config <device.json> lookup --context <context-id> --kind prefix
<client> --config <device.json> lookup --context <context-id> --kind action
<client> --config <device.json> lookup --context <context-id> --kind outcome
<client> --config <device.json> task
<client> --config <device.json> outcome --context <context-id> --reason context_expired
<client> --config <device.json> lookup --context <context-id> --kind outcome
```

A lookup reporting “original operation is not in this device cache” means no local original of that kind; it does not prove an unknown remote request had no effect. Do not use a missing or replaced cache to bypass recovery. If an action or outcome exists, recover that exact original instead of creating a competing terminal outcome. Unknown or pending results block new decisions. The SDK checks the original prefix is committed, the currently signed task is Active with the same complete fence, and the current signed tick strictly exceeds the original expiry before first creating `context_expired`. It uses the cached public prefix identity and fresh signed base without reading expired private content. Repeating this outcome preserves its first signed request; it does not update the base, nonce or reason. Only after the outcome is **committed**, reread `task` and capture a new context while still authorized. A rejected termination, Stop, changed fence or revoked read authority requires stopping/recovery, not another context.

After goal publication, executor replacement, Stop or delegation revocation, old contexts cannot authorize new effects. Query original settled results, reread `task`, and capture a fresh context only while still authorized. On Stop or revoked authority, stop acting and report the boundary. Do not rebind yourself or alter the owner goal.

Report the goal revision, chosen action, canonical result and observed effect concisely. Distinguish a prepared-world attached-agent run from cold entry or an OpenClaw integration test. If independent isolation is instruction-only, say so.

原 operation 回读若为 `HistoryUnavailable`，保留原 request/correlation、run 与持久预算并停止新副作用；交由宿主恢复历史材料，不能把记录头仍存在当作回执可用，也不能换世界或新建操作来代替恢复。

工业制造交付后，从 `industry.world_rule_status.agent_orders[order_id].current_owner` 和 `current_control` 核对当前玩家、主 Agent、控制来源与 generation；只有 `current_control.status=active` 才是当前权威证据。字段为 null 或 blocked 时不可把历史 quote 的 owner 字段当作当前控制。

工业制造先从 `industry.world_rule_status.manufacturing_quotes` 读取当前 quote，再把同一 `quoted_at` 和 `quote_digest` 填入 `ConfirmAgentOrder`。报价只读，不预留；过期或状态漂移应重读，已确认订单继续查询原订单。生产需 12 个合法工作 tick；未开工可取消，开工后按原订单失败/报废合同处置，不能换请求重复交付。

紧急电力仅可消费已登记最低服务义务的受限额度：`schedule-essential --context <id> --factory <factory> --recipe <recipe> --batches <n> --obligation <registered-id>`。它与普通排程共用设施、物料和权限检查；不会把额度加入普通可转移电力。未知权威来源、过期区间或未支持路线应保持 blocker，不能以自报库存或提案文案代替供给事实。

制造动作的 cognition `committed` 仅表示候选动作已提交；在 industry 的 `world_rule_status.recent_action_results` 按 action id / business request id 核对实际业务结果。`rejected` 读取原始 reason 和 next_step；报价过期应重新读 quote，并用新业务 request id 提交。`unknown` 或不在近期窗口内时先读当前订单和资源，不把缺少结果当成功，也不盲目重复提交。
