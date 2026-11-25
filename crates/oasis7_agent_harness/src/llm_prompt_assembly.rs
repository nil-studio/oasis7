use serde::Serialize;

const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 8_192;
const DEFAULT_RESERVED_OUTPUT_TOKENS: usize = 1_024;
const DEFAULT_SAFETY_MARGIN_TOKENS: usize = 512;
const MIN_EFFECTIVE_INPUT_BUDGET_TOKENS: usize = 256;
const HISTORY_SOFT_CAP_TOKENS: usize = 256;
const MEMORY_SOFT_CAP_TOKENS: usize = 192;
const FINALIZE_HISTORY_SOFT_CAP_TOKENS: usize = 192;
const FINALIZE_MEMORY_SOFT_CAP_TOKENS: usize = 128;
const CONTEXT_MIN_TOKENS: usize = 64;
const PEAK_MIN_TARGET_TOKENS: usize = 768;
const PEAK_SOFT_RESERVE_TOKENS: usize = 256;
const PEAK_HARD_RESERVE_TOKENS: usize = 128;
const FINALIZE_PEAK_SOFT_RESERVE_TOKENS: usize = 320;
const FINALIZE_PEAK_HARD_RESERVE_TOKENS: usize = 192;
const PEAK_HISTORY_SOFT_CAP_TOKENS: usize = 192;
const PEAK_MEMORY_SOFT_CAP_TOKENS: usize = 128;
const PEAK_HISTORY_HARD_CAP_TOKENS: usize = 128;
const PEAK_MEMORY_HARD_CAP_TOKENS: usize = 96;
const DEFAULT_REFINE_HARDWARE_YIELD_PPM: i64 = 1_000;
const DEFAULT_REFINE_MIN_EFFECTIVE_MASS_G: i64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptBudget {
    pub context_window_tokens: usize,
    pub reserved_output_tokens: usize,
    pub safety_margin_tokens: usize,
}

impl Default for PromptBudget {
    fn default() -> Self {
        Self {
            context_window_tokens: DEFAULT_CONTEXT_WINDOW_TOKENS,
            reserved_output_tokens: DEFAULT_RESERVED_OUTPUT_TOKENS,
            safety_margin_tokens: DEFAULT_SAFETY_MARGIN_TOKENS,
        }
    }
}

impl PromptBudget {
    pub fn effective_input_budget_tokens(&self) -> usize {
        self.context_window_tokens
            .saturating_sub(self.reserved_output_tokens)
            .saturating_sub(self.safety_margin_tokens)
            .max(MIN_EFFECTIVE_INPUT_BUDGET_TOKENS)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptAssemblyInput<'a> {
    pub agent_id: &'a str,
    pub base_system_prompt: &'a str,
    pub short_term_goal: &'a str,
    pub long_term_goal: &'a str,
    pub observation_json: &'a str,
    pub module_history_json: &'a str,
    pub conversation_history_json: &'a str,
    pub memory_digest: Option<&'a str>,
    pub step_context: PromptStepContext,
    pub harvest_max_amount_cap: i64,
    pub prompt_budget: PromptBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PromptStepContext {
    pub step_index: usize,
    pub max_steps: usize,
    pub module_calls_used: usize,
    pub module_calls_max: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptSectionKind {
    Policy,
    Goals,
    Context,
    Tools,
    Conversation,
    History,
    Memory,
    OutputSchema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptSectionPriority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromptSection {
    pub kind: PromptSectionKind,
    pub priority: PromptSectionPriority,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromptSectionTrace {
    pub kind: PromptSectionKind,
    pub priority: PromptSectionPriority,
    pub included: bool,
    pub estimated_tokens: usize,
    pub emitted_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptAssemblyOutput {
    pub system_prompt: String,
    pub user_prompt: String,
    pub sections: Vec<PromptSection>,
    pub section_trace: Vec<PromptSectionTrace>,
    pub effective_input_budget_tokens: usize,
    pub estimated_input_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SectionState {
    section: PromptSection,
    required: bool,
    included: bool,
    estimated_tokens: usize,
}

impl SectionState {
    fn new(section: PromptSection, required: bool) -> Self {
        let estimated_tokens = estimate_tokens(section.content.as_str());
        Self {
            section,
            required,
            included: true,
            estimated_tokens,
        }
    }

    fn emitted_tokens(&self) -> usize {
        if self.included {
            estimate_tokens(self.section.content.as_str())
        } else {
            0
        }
    }
}

pub struct PromptAssembler;

impl PromptAssembler {
    pub fn assemble(input: PromptAssemblyInput<'_>) -> PromptAssemblyOutput {
        let mut sections = Vec::new();
        sections.push(SectionState::new(
            PromptSection {
                kind: PromptSectionKind::Policy,
                priority: PromptSectionPriority::High,
                content: format!(
                    "{}\n\n你是一个硅基文明 Agent。必须通过 tool call 输出，不要直接输出 JSON 或额外文字。",
                    input.base_system_prompt,
                ),
            },
            true,
        ));
        sections.push(SectionState::new(
            PromptSection {
                kind: PromptSectionKind::Goals,
                priority: PromptSectionPriority::High,
                content: format!(
                    "[Agent Goals]\n- short_term_goal: {}\n- long_term_goal: {}\n- anti_stagnation: 缺少新证据时避免重复同一动作。\n- exploration_bias: 局部状态不变时优先探索新线索。",
                    input.short_term_goal, input.long_term_goal,
                ),
            },
            true,
        ));
        sections.push(SectionState::new(
            PromptSection {
                kind: PromptSectionKind::Tools,
                priority: PromptSectionPriority::High,
                content: r#"[Tool Protocol]
- 本代理采用 tool-only 协议：每轮必须调用 tool，禁止输出 JSON 文本或自然语言正文
- 查询工具：agent_modules_list / environment_current_observation / memory_short_term_recent / memory_long_term_search / world_rules_guide / module.lifecycle.status / power.order_book.status / module.market.status / social.state.status
- 最终决策工具：agent_submit_decision
- 常见别名会自动纠正：agent_modules_list -> agent.modules.list，environment_current_observation -> environment.current_observation，memory_short_term_recent -> memory.short_term.recent，memory_long_term_search -> memory.long_term.search，world_rules_guide -> world.rules.guide，module_lifecycle_status -> module.lifecycle.status，power_order_book_status -> power.order_book.status，module_market_status -> module.market.status，social_state_status -> social.state.status
- 每轮只允许调用一个 tool；不要在同一回复混用查询工具与最终决策工具
- 若需要对玩家说明意图，请在 `agent_submit_decision` 参数中使用可选字段 `message_to_user`
- 开局或阶段切换时若不确定前置条件，先调用 `world.rules.guide`（topic=quickstart/resources/industry/governance）
- 当连续动作触发反重复门控时，优先查询新证据或输出 execute_until，不要复读同一终局动作"#.to_string(),
            },
            true,
        ));
        sections.push(SectionState::new(
            PromptSection {
                kind: PromptSectionKind::Context,
                priority: PromptSectionPriority::High,
                content: format!(
                    "[Context]\n- agent_id: {}\n- observation(json): {}",
                    input.agent_id, input.observation_json,
                ),
            },
            true,
        ));
        sections.push(SectionState::new(
            PromptSection {
                kind: PromptSectionKind::History,
                priority: PromptSectionPriority::Medium,
                content: format!("[Module History]\n{}", input.module_history_json),
            },
            false,
        ));
        sections.push(SectionState::new(
            PromptSection {
                kind: PromptSectionKind::Conversation,
                priority: PromptSectionPriority::Medium,
                content: format!("[Conversation]\n{}", input.conversation_history_json),
            },
            false,
        ));

        if let Some(memory_digest) = input.memory_digest
            && !memory_digest.trim().is_empty()
        {
            sections.push(SectionState::new(
                PromptSection {
                    kind: PromptSectionKind::Memory,
                    priority: PromptSectionPriority::Low,
                    content: format!("[Memory Digest]\n{}", memory_digest),
                },
                false,
            ));
        }

        sections.push(SectionState::new(
            PromptSection {
                kind: PromptSectionKind::OutputSchema,
                priority: PromptSectionPriority::High,
                content: format!(
                    r#"[Decision Tool Args Schema]
- 必须通过 tool `agent_submit_decision` 提交以下 args 结构（禁止直接输出 JSON）：
{{"decision":"wait"}}
{{"decision":"wait_ticks","ticks":<u64>}}
{{"decision":"move_agent","to":"<location_id>"}}
{{"decision":"harvest_radiation","max_amount":<i64 1..={}>}}
{{"decision":"buy_power","buyer":"<self|agent:<id>|location:<id>>","seller":"<self|agent:<id>|location:<id>>","amount":<i64 >=1>,"price_per_pu":<i64 >=0>}}
{{"decision":"sell_power","seller":"<self|agent:<id>|location:<id>>","buyer":"<self|agent:<id>|location:<id>>","amount":<i64 >=1>,"price_per_pu":<i64 >=0>}}
{{"decision":"place_power_order","owner":"<self|agent:<id>|location:<id>>","side":"<buy|sell>","amount":<i64 >=1>,"limit_price_per_pu":<i64 >=0>}}
{{"decision":"cancel_power_order","owner":"<self|agent:<id>|location:<id>>","order_id":<u64 >=1>}}
{{"decision":"transfer_resource","from_owner":"<self|agent:<id>|location:<id>>","to_owner":"<self|agent:<id>|location:<id>>","kind":"<electricity|data>","amount":<i64 >=1>}}
{{"decision":"mine_compound","owner":"<self|agent:<id>|location:<id>>","location_id":"<location_id>","compound_mass_g":<i64 >=1>}}
{{"decision":"refine_compound","owner":"<self|agent:<id>|location:<id>>","compound_mass_g":<i64 >=1>}}
{{"decision":"build_factory","owner":"<self|agent:<id>|location:<id>>","location_id":"<location_id>","factory_id":"<factory_id>","factory_kind":"<factory_kind>"}}
{{"decision":"schedule_recipe","owner":"<self|agent:<id>|location:<id>>","factory_id":"<factory_id>","recipe_id":"<recipe_id>","batches":<i64 >=1>}}
{{"decision":"compile_module_artifact_from_source","publisher":"<self|agent:<id>>","module_id":"<module_id>","manifest_path":"<relative_manifest_path>","source_files":{{"Cargo.toml":"<text>","src/lib.rs":"<text>"}}}}
{{"decision":"deploy_module_artifact","publisher":"<self|agent:<id>>","module_id":"<module_id optional>","wasm_hash":"<sha256_hex>","wasm_bytes_hex":"<hex_bytes>"}}
{{"decision":"install_module_from_artifact","installer":"<self|agent:<id>>","module_id":"<module_id>","module_version":"<semver>","wasm_hash":"<sha256_hex>","activate":<bool>}}
{{"decision":"list_module_artifact_for_sale","seller":"<self|agent:<id>>","wasm_hash":"<sha256_hex>","price_kind":"<electricity|data>","price_amount":<i64 >=1>}}
{{"decision":"buy_module_artifact","buyer":"<self|agent:<id>>","wasm_hash":"<sha256_hex>"}}
{{"decision":"delist_module_artifact","seller":"<self|agent:<id>>","wasm_hash":"<sha256_hex>"}}
{{"decision":"destroy_module_artifact","owner":"<self|agent:<id>>","wasm_hash":"<sha256_hex>","reason":"<text>"}}
{{"decision":"place_module_artifact_bid","bidder":"<self|agent:<id>>","wasm_hash":"<sha256_hex>","price_kind":"<electricity|data>","price_amount":<i64 >=1>}}
{{"decision":"cancel_module_artifact_bid","bidder":"<self|agent:<id>>","wasm_hash":"<sha256_hex>","bid_order_id":<u64 >=1>}}
{{"decision":"publish_social_fact","actor":"<self|agent:<id>|location:<id>>","schema_id":"<schema_id>","subject":"<self|agent:<id>|location:<id>>","object":"<self|agent:<id>|location:<id>>","claim":"<text>","confidence_ppm":<i64 1..=1000000>,"evidence_event_ids":[<u64 >=1>],"ttl_ticks":<u64 >=1>,"stake":{{"kind":"<electricity|data>","amount":<i64 >=1>}}}}
{{"decision":"challenge_social_fact","challenger":"<self|agent:<id>|location:<id>>","fact_id":<u64 >=1>,"reason":"<text>","stake":{{"kind":"<electricity|data>","amount":<i64 >=1>}}}}
{{"decision":"adjudicate_social_fact","adjudicator":"<self|agent:<id>|location:<id>>","fact_id":<u64 >=1>,"adjudication":"<confirm|retract>","notes":"<text>"}}
{{"decision":"revoke_social_fact","actor":"<self|agent:<id>|location:<id>>","fact_id":<u64 >=1>,"reason":"<text>"}}
{{"decision":"declare_social_edge","declarer":"<self|agent:<id>|location:<id>>","schema_id":"<schema_id>","relation_kind":"<relation_kind>","from":"<self|agent:<id>|location:<id>>","to":"<self|agent:<id>|location:<id>>","weight_bps":<i64 -10000..=10000>,"backing_fact_ids":[<u64 >=1>],"ttl_ticks":<u64 >=1>}}
{{"decision":"open_governance_proposal","proposer_agent_id":"<self|agent:<id>>","proposal_key":"<proposal_key>","title":"<title>","description":"<text>","options":["approve","reject"],"voting_window_ticks":<u64 8..=256>,"quorum_weight":<u64 >=1>,"pass_threshold_bps":<u64 1000..=9000>}}
{{"decision":"cast_governance_vote","voter_agent_id":"<self|agent:<id>>","proposal_key":"<proposal_key>","option":"<option>","weight":<u64 >=1>}}
{{"decision":"resolve_crisis","resolver_agent_id":"<self|agent:<id>>","crisis_id":"<crisis_id>","strategy":"<text>","success":<bool>}}
{{"decision":"grant_meta_progress","operator_agent_id":"<self|agent:<id>>","target_agent_id":"<self|agent:<id>>","track":"<track>","points":<i64 !=0>,"achievement_id":"<optional_text>"}}
{{"decision":"open_economic_contract","creator_agent_id":"<self|agent:<id>>","contract_id":"<contract_id>","counterparty_agent_id":"<self|agent:<id>>","settlement_kind":"<electricity|data>","settlement_amount":<i64 >=1>,"reputation_stake":<i64 >=1>,"expires_at":<u64 >=1>,"description":"<text>"}}
{{"decision":"accept_economic_contract","accepter_agent_id":"<self|agent:<id>>","contract_id":"<contract_id>"}}
{{"decision":"settle_economic_contract","operator_agent_id":"<self|agent:<id>>","contract_id":"<contract_id>","success":true,"notes":"<text>"}}
{{"decision":"execute_until","action":{{<decision_json>}},"until":{{"event":"<event_name>"}},"max_ticks":<u64>}}
- 任意决策 args 可选附带：`"message_to_user":"<string>"`
- 推荐 move 模板: {{"decision":"execute_until","action":{{"decision":"move_agent","to":"<location_id>"}},"until":{{"event_any_of":["arrive_target","action_rejected","new_visible_agent","new_visible_location"]}},"max_ticks":<u64 1..=8>}}
- 推荐 harvest 模板: {{"decision":"execute_until","action":{{"decision":"harvest_radiation","max_amount":<i64 1..={}>}},"until":{{"event_any_of":["action_rejected","insufficient_electricity","thermal_overload","new_visible_agent","new_visible_location"]}},"max_ticks":<u64 1..=3>}}
- 推荐 buy_power 模板: {{"decision":"buy_power","buyer":"self","seller":"agent:<id>","amount":<i64 >=1>,"price_per_pu":0}}
- 推荐 place_power_order 模板: {{"decision":"place_power_order","owner":"self","side":"buy","amount":<i64 >=1>,"limit_price_per_pu":0}}
- 推荐 transfer 模板: {{"decision":"transfer_resource","from_owner":"location:<id>","to_owner":"self","kind":"electricity","amount":<i64 >=1>}}
- 推荐 mine 模板: {{"decision":"mine_compound","owner":"self","location_id":"<location_id>","compound_mass_g":<i64 >=1000>}}
- 推荐 refine 模板: {{"decision":"refine_compound","owner":"self","compound_mass_g":<i64 >=1>}}
- 推荐 build_factory 模板: {{"decision":"build_factory","owner":"self","location_id":"<location_id>","factory_id":"factory.smelter.mk1","factory_kind":"factory.smelter.mk1"}}
- 推荐 schedule_recipe 模板: {{"decision":"schedule_recipe","owner":"self","factory_id":"factory.smelter.mk1","recipe_id":"recipe.smelter.iron_ingot","batches":1}}
- post_onboarding 工业主线优先级: 先 smelter（iron_ingot/copper_wire/polymer_resin/alloy_plate），再 assembler（gear/control_chip/motor/logistics_drone）；当 runtime stage 进入更高阶段后再推进 sensor_pack/module_rack/factory_core，不要在还没形成第一条 smelter 产线前直接跳到 assembler-only 规划
- 推荐 compile 模板: {{"decision":"compile_module_artifact_from_source","publisher":"self","module_id":"m.llm.example","manifest_path":"Cargo.toml","source_files":{{"Cargo.toml":"<text>","src/lib.rs":"<text>"}}}}
- 推荐 install 模板: {{"decision":"install_module_from_artifact","installer":"self","module_id":"m.llm.example","module_version":"0.1.0","wasm_hash":"<sha256_hex>","activate":true}}
- 推荐 list_module_artifact_for_sale 模板: {{"decision":"list_module_artifact_for_sale","seller":"self","wasm_hash":"<sha256_hex>","price_kind":"data","price_amount":1}}
- 推荐 place_module_artifact_bid 模板: {{"decision":"place_module_artifact_bid","bidder":"self","wasm_hash":"<sha256_hex>","price_kind":"data","price_amount":2}}
- 推荐 publish_social_fact 模板: {{"decision":"publish_social_fact","actor":"self","schema_id":"social.reputation.v1","subject":"agent:<id>","claim":"<text>","confidence_ppm":800000,"evidence_event_ids":[<u64 >=1>]}}
- 推荐 declare_social_edge 模板: {{"decision":"declare_social_edge","declarer":"self","schema_id":"social.relation.v1","relation_kind":"trusted_peer","from":"self","to":"agent:<id>","weight_bps":5000,"backing_fact_ids":[<u64 >=1>]}}
- 推荐 gameplay 模板（简版）: open_governance_proposal -> cast_governance_vote，再根据局势切换 resolve_crisis 或 grant_meta_progress
- 推荐 economic 合约链路: open_economic_contract -> accept_economic_contract -> settle_economic_contract
- 治理节奏建议：治理相关动作应形成提案/投票/危机处置/成长结算的阶段推进，不要长时间停留在 open/cast 循环；当同类治理动作连续出现时，优先切换到其他治理或韧性动作
- event_name 可选: action_rejected / new_visible_agent / new_visible_location / arrive_target / insufficient_electricity / thermal_overload / harvest_yield_below / harvest_available_below
- 当 event_name 为 harvest_yield_below / harvest_available_below 时，必须提供 until.value_lte（>=0）
- execute_until.action 必须是可执行动作，且不能使用 wait/wait_ticks
- harvest_radiation.max_amount 必须是正整数，且不超过 {}
- buy_power/sell_power.amount 必须是正整数；price_per_pu 必须 >= 0（0 表示按市场报价）
- place_power_order.side 仅允许 buy/sell；amount 必须是正整数；limit_price_per_pu 必须 >= 0
- cancel_power_order.order_id 必须是正整数
- transfer_resource.kind 仅允许 electricity/data，amount 必须为正整数
- owner 字段仅允许 self/agent:<id>/location:<id>
- publish_social_fact.confidence_ppm 必须在 1..=1000000；evidence_event_ids/backing_fact_ids 不能为空
- adjudicate_social_fact.adjudication 仅允许 confirm/retract
- declare_social_edge.weight_bps 必须在 -10000..=10000
- gameplay/economic 决策字段必须遵守 schema 中的枚举与数值约束（尤其 proposal options、vote weight、meta points、contract settlement_kind/amount）
- settle_economic_contract.success 当前必须为 true
- move_agent.to 不能是当前所在位置（若 observation 中该 location 的 distance_cm=0，则不要选择该 location）
- factory_kind 当前支持：factory.smelter.mk1、factory.assembler.mk1、factory.power.radiation.mk1（留空将被拒绝）
- recipe_id 当前支持：recipe.smelter.iron_ingot / recipe.smelter.copper_wire / recipe.smelter.polymer_resin / recipe.smelter.alloy_plate / recipe.assembler.gear / recipe.assembler.control_chip / recipe.assembler.motor_mk1 / recipe.assembler.logistics_drone / recipe.assembler.sensor_pack / recipe.assembler.module_rack / recipe.assembler.factory_core
- schedule_recipe.batches 必须是正整数
- compile_module_artifact_from_source: module_id/manifest_path/source_files 必填；source_files value 必须是 utf8 文本
- deploy_module_artifact: wasm_hash 必须为 sha256 hex；wasm_bytes_hex 必须是非空 hex 字节串
- install_module_from_artifact: installer 必须是 self 或 agent:<id>；module_version 为空时默认 0.1.0
- install_module_to_target_from_artifact: install_target_type 仅允许 self_agent/location_infrastructure（location_infrastructure 需要 install_target_location_id）
- module 市场动作（list/buy/delist/destroy/place_bid/cancel_bid）中的 agent 字段仅允许 self 或 agent:<id>
- module 市场动作的 price_amount 必须是正整数；cancel_module_artifact_bid.bid_order_id 必须是正整数
- 若准备 install 但缺少 wasm_hash，先调用 `module.lifecycle.status` 读取最近 artifact 列表再执行
- 默认 recipe_hardware_cost_per_batch：iron_ingot=2，copper_wire=2，polymer_resin=2，alloy_plate=4，gear=2，control_chip=2，motor_mk1=4，logistics_drone=8，sensor_pack=4，module_rack=6，factory_core=8
- 当 owner=self 时，schedule_recipe.batches 必须 <= floor(self_resources.data / recipe_hardware_cost_per_batch)；若上界为 0，先 refine_compound 再 schedule_recipe
- 当 observation.recipe_coverage.missing 非空且你准备重复 observation.recipe_coverage.completed 中的 recipe 时，必须先切换到 missing 列表中的配方（优先 missing[0]）
- 默认经济参数下（refine_hardware_yield_ppm={}），refine_compound 需 compound_mass_g >= {} 才会产出 >=1 hardware；低于阈值会因产出为 0 被拒绝
- [Failure Recovery Policy] 当 observation.last_action.success=false 时，必须优先按 reject_reason 切换下一动作：
  - insufficient_resource.data -> mine_compound（owner=self, location_id 使用可见 location, compound_mass_g>=1000）补足 data 前置；若 compound 已充足再 refine_compound，或 transfer_resource(kind=data)
  - insufficient_resource.electricity -> harvest_radiation 或 transfer_resource(kind=electricity)
  - factory_not_found -> build_factory（smelter 配方先 factory.smelter.mk1，assembler 配方先 factory.assembler.mk1，再 schedule_recipe）
  - location_not_found -> 仅使用 observation.visible_locations 中可见 location_id；未知 location 回退当前 location
  - rule_denied -> 检查 recipe_id 与 factory_kind 兼容关系；若失败动作属于 gameplay（open_governance_proposal/cast_governance_vote/resolve_crisis/grant_meta_progress），下一轮优先切换到另一种 gameplay 动作并更换 proposal_key/crisis_id，避免原样重试；不兼容时切换兼容工厂，smelter 配方优先 build_factory(factory.smelter.mk1)，assembler 配方优先 build_factory(factory.assembler.mk1)
  - agent_already_at_location -> 禁止重复 move_agent 到同 location，改为 schedule_recipe/refine_compound/harvest_radiation
  - 其他 reject_reason -> 先输出最小可执行补救动作，不得原样重试失败参数
- 禁止连续超过 2 轮同参数 harvest_radiation；若连续采集未推进目标，下一轮必须切到 refine_compound/build_factory/schedule_recipe
- harvest 的 execute_until.max_ticks 运行时会被硬裁剪到 3；若电力已足够下一步 refine/schedule，必须立即回切，不得继续长 harvest

[Output Hard Rules]
- 每轮只允许调用一个 tool（查询或 `agent_submit_decision`）
- 当前上下文若已足够，请直接调用 `agent_submit_decision` 提交最终决策（可 execute_until）
- 禁止在 tool call 之外输出 JSON/文本；不要使用 `---` 分隔多段内容"#,
                    input.harvest_max_amount_cap,
                    input.harvest_max_amount_cap,
                    input.harvest_max_amount_cap,
                    DEFAULT_REFINE_HARDWARE_YIELD_PPM,
                    DEFAULT_REFINE_MIN_EFFECTIVE_MASS_G,
                ),
            },
            true,
        ));

        let budget_tokens = input.prompt_budget.effective_input_budget_tokens();
        let (history_soft_cap, memory_soft_cap) = Self::soft_section_caps(input.step_context);
        Self::apply_budget(
            &mut sections,
            budget_tokens,
            history_soft_cap,
            memory_soft_cap,
        );
        let (peak_soft_tokens, peak_hard_tokens) =
            Self::peak_targets_tokens(input.step_context, budget_tokens);
        Self::apply_peak_budget(&mut sections, peak_soft_tokens, peak_hard_tokens);

        let section_trace = sections
            .iter()
            .map(|state| PromptSectionTrace {
                kind: state.section.kind,
                priority: state.section.priority,
                included: state.included,
                estimated_tokens: state.estimated_tokens,
                emitted_tokens: state.emitted_tokens(),
            })
            .collect::<Vec<_>>();

        let included_sections = sections
            .iter()
            .filter(|state| state.included)
            .map(|state| state.section.clone())
            .collect::<Vec<_>>();

        let system_prompt =
            join_included_section_content(&included_sections, is_system_prompt_section);

        let user_prompt = join_included_section_content(&included_sections, is_user_prompt_section);

        let estimated_input_tokens = estimate_tokens(system_prompt.as_str())
            .saturating_add(estimate_tokens(user_prompt.as_str()));

        PromptAssemblyOutput {
            system_prompt,
            user_prompt,
            sections: included_sections,
            section_trace,
            effective_input_budget_tokens: budget_tokens,
            estimated_input_tokens,
        }
    }

    fn soft_section_caps(step_context: PromptStepContext) -> (usize, usize) {
        let turns_remaining = step_context
            .max_steps
            .saturating_sub(step_context.step_index.saturating_add(1));
        let module_calls_remaining = step_context
            .module_calls_max
            .saturating_sub(step_context.module_calls_used);
        if turns_remaining <= 1 || module_calls_remaining <= 1 {
            (
                FINALIZE_HISTORY_SOFT_CAP_TOKENS,
                FINALIZE_MEMORY_SOFT_CAP_TOKENS,
            )
        } else {
            (HISTORY_SOFT_CAP_TOKENS, MEMORY_SOFT_CAP_TOKENS)
        }
    }

    fn apply_budget(
        sections: &mut [SectionState],
        budget_tokens: usize,
        history_soft_cap_tokens: usize,
        memory_soft_cap_tokens: usize,
    ) {
        if Self::included_tokens(sections) > budget_tokens {
            Self::truncate_soft_section(
                sections,
                PromptSectionKind::History,
                history_soft_cap_tokens,
            );
        }
        if Self::included_tokens(sections) > budget_tokens {
            Self::truncate_soft_section(
                sections,
                PromptSectionKind::Memory,
                memory_soft_cap_tokens,
            );
        }

        let removable_order = [
            PromptSectionKind::Memory,
            PromptSectionKind::History,
            PromptSectionKind::Conversation,
        ];

        for kind in removable_order {
            if Self::included_tokens(sections) <= budget_tokens {
                break;
            }
            Self::drop_optional_section(sections, kind);
        }

        if Self::included_tokens(sections) > budget_tokens {
            Self::truncate_required_context(sections, budget_tokens);
        }
    }

    fn peak_targets_tokens(
        step_context: PromptStepContext,
        budget_tokens: usize,
    ) -> (usize, usize) {
        let turns_remaining = step_context
            .max_steps
            .saturating_sub(step_context.step_index.saturating_add(1));
        let module_calls_remaining = step_context
            .module_calls_max
            .saturating_sub(step_context.module_calls_used);

        let (soft_reserve, hard_reserve) = if turns_remaining <= 1 || module_calls_remaining <= 1 {
            (
                FINALIZE_PEAK_SOFT_RESERVE_TOKENS,
                FINALIZE_PEAK_HARD_RESERVE_TOKENS,
            )
        } else {
            (PEAK_SOFT_RESERVE_TOKENS, PEAK_HARD_RESERVE_TOKENS)
        };

        let min_target = PEAK_MIN_TARGET_TOKENS.min(budget_tokens.max(1));
        let hard_target = budget_tokens.saturating_sub(hard_reserve).max(min_target);
        let soft_target = hard_target
            .saturating_sub(soft_reserve.saturating_sub(hard_reserve))
            .max(min_target.saturating_sub(96));

        (soft_target.min(hard_target), hard_target)
    }

    fn apply_peak_budget(
        sections: &mut [SectionState],
        peak_soft_tokens: usize,
        peak_hard_tokens: usize,
    ) {
        if Self::included_tokens(sections) <= peak_soft_tokens {
            return;
        }

        Self::truncate_soft_section(
            sections,
            PromptSectionKind::History,
            PEAK_HISTORY_SOFT_CAP_TOKENS,
        );
        if Self::included_tokens(sections) > peak_soft_tokens {
            Self::truncate_soft_section(
                sections,
                PromptSectionKind::Memory,
                PEAK_MEMORY_SOFT_CAP_TOKENS,
            );
        }
        if Self::included_tokens(sections) > peak_soft_tokens {
            Self::truncate_soft_section(
                sections,
                PromptSectionKind::Conversation,
                PEAK_HISTORY_SOFT_CAP_TOKENS,
            );
        }

        if Self::included_tokens(sections) > peak_hard_tokens {
            Self::truncate_soft_section(
                sections,
                PromptSectionKind::History,
                PEAK_HISTORY_HARD_CAP_TOKENS,
            );
        }
        if Self::included_tokens(sections) > peak_hard_tokens {
            Self::truncate_soft_section(
                sections,
                PromptSectionKind::Memory,
                PEAK_MEMORY_HARD_CAP_TOKENS,
            );
        }
        if Self::included_tokens(sections) > peak_hard_tokens {
            Self::drop_optional_section(sections, PromptSectionKind::Memory);
        }
        if Self::included_tokens(sections) > peak_hard_tokens {
            Self::drop_optional_section(sections, PromptSectionKind::History);
        }
        // Keep conversation/context in peak mode to avoid losing critical short-horizon state.
    }

    fn truncate_soft_section(
        sections: &mut [SectionState],
        kind: PromptSectionKind,
        token_cap: usize,
    ) {
        if let Some(state) = sections
            .iter_mut()
            .find(|state| state.section.kind == kind && state.included)
        {
            state.section.content =
                truncate_to_token_cap(state.section.content.as_str(), token_cap);
        }
    }

    fn drop_optional_section(sections: &mut [SectionState], kind: PromptSectionKind) {
        if let Some(state) = sections
            .iter_mut()
            .find(|state| state.section.kind == kind && state.included && !state.required)
        {
            state.included = false;
        }
    }

    fn truncate_required_context(sections: &mut [SectionState], budget_tokens: usize) {
        let current_total = Self::included_tokens(sections);
        if current_total <= budget_tokens {
            return;
        }

        let non_context_tokens = sections
            .iter()
            .filter(|state| state.included && state.section.kind != PromptSectionKind::Context)
            .map(SectionState::emitted_tokens)
            .sum::<usize>();
        let context_cap = budget_tokens
            .saturating_sub(non_context_tokens)
            .max(CONTEXT_MIN_TOKENS);

        if let Some(context) = sections
            .iter_mut()
            .find(|state| state.section.kind == PromptSectionKind::Context && state.included)
        {
            context.section.content =
                truncate_to_token_cap(context.section.content.as_str(), context_cap);
        }
    }

    fn included_tokens(sections: &[SectionState]) -> usize {
        sections.iter().map(SectionState::emitted_tokens).sum()
    }
}

fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    (chars.saturating_add(3) / 4).max(1)
}

fn join_included_section_content(
    sections: &[PromptSection],
    include: impl Fn(PromptSectionKind) -> bool,
) -> String {
    let mut output = String::new();
    for section in sections.iter().filter(|section| include(section.kind)) {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(section.content.as_str());
    }
    output
}

fn is_system_prompt_section(kind: PromptSectionKind) -> bool {
    matches!(
        kind,
        PromptSectionKind::Policy | PromptSectionKind::Goals | PromptSectionKind::Tools
    )
}

fn is_user_prompt_section(kind: PromptSectionKind) -> bool {
    matches!(
        kind,
        PromptSectionKind::Context
            | PromptSectionKind::Conversation
            | PromptSectionKind::History
            | PromptSectionKind::Memory
            | PromptSectionKind::OutputSchema
    )
}

fn truncate_to_token_cap(text: &str, token_cap: usize) -> String {
    if token_cap == 0 {
        return "...(truncated)".to_string();
    }
    if estimate_tokens(text) <= token_cap {
        return text.to_string();
    }

    let max_chars = token_cap.saturating_mul(4).saturating_sub(16);
    let mut result = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            break;
        }
        result.push(ch);
    }

    if !result.ends_with("\n") {
        result.push('\n');
    }
    result.push_str("...(truncated)");
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input<'a>() -> PromptAssemblyInput<'a> {
        PromptAssemblyInput {
            agent_id: "agent-1",
            base_system_prompt: "base prompt",
            short_term_goal: "short goal",
            long_term_goal: "long goal",
            observation_json: "{\"time\":1}",
            module_history_json: "[]",
            conversation_history_json: r#"[{"role":"player","content":"hello"}]"#,
            memory_digest: Some("obs@T1 ..."),
            step_context: PromptStepContext {
                step_index: 0,
                max_steps: 4,
                module_calls_used: 0,
                module_calls_max: 3,
            },
            harvest_max_amount_cap: 100,
            prompt_budget: PromptBudget::default(),
        }
    }

    #[test]
    fn prompt_budget_effective_input_budget_respects_reservations() {
        let budget = PromptBudget {
            context_window_tokens: 4_096,
            reserved_output_tokens: 512,
            safety_margin_tokens: 256,
        };
        assert_eq!(budget.effective_input_budget_tokens(), 3_328);
    }

    #[test]
    fn prompt_assembly_splits_system_and_user_sections() {
        let output = PromptAssembler::assemble(sample_input());
        assert!(output.system_prompt.contains("base prompt"));
        assert!(output.system_prompt.contains("short goal"));
        assert!(output.system_prompt.contains("agent_submit_decision"));

        assert!(output.user_prompt.contains("observation(json)"));
        assert!(output.user_prompt.contains("[Conversation]"));
        assert!(output.user_prompt.contains("[Memory Digest]"));
        assert!(output.user_prompt.contains("Decision Tool Args Schema"));
    }

    #[test]
    fn prompt_assembly_records_section_trace() {
        let output = PromptAssembler::assemble(sample_input());
        assert!(!output.sections.is_empty());
        assert!(!output.section_trace.is_empty());
        assert!(
            output
                .section_trace
                .iter()
                .any(|trace| trace.kind == PromptSectionKind::Policy && trace.included)
        );
        assert!(output.section_trace.iter().all(|trace| {
            if trace.included {
                trace.emitted_tokens > 0
            } else {
                trace.emitted_tokens == 0
            }
        }));
    }

    #[test]
    fn prompt_assembly_omits_empty_memory_digest_block() {
        let mut input = sample_input();
        input.memory_digest = Some("   ");

        let output = PromptAssembler::assemble(input);
        assert!(!output.user_prompt.contains("[Memory Digest]"));
    }

    #[test]
    fn prompt_budget_removes_low_priority_sections_first() {
        let history = "h".repeat(4_000);
        let conversation = "c".repeat(2_000);
        let memory = "m".repeat(2_000);
        let input = PromptAssemblyInput {
            agent_id: "agent-1",
            base_system_prompt: "base prompt",
            short_term_goal: "short goal",
            long_term_goal: "long goal",
            observation_json: "{\"time\":1}",
            module_history_json: history.as_str(),
            conversation_history_json: conversation.as_str(),
            memory_digest: Some(memory.as_str()),
            step_context: PromptStepContext {
                step_index: 0,
                max_steps: 4,
                module_calls_used: 0,
                module_calls_max: 3,
            },
            harvest_max_amount_cap: 100,
            prompt_budget: PromptBudget {
                context_window_tokens: 512,
                reserved_output_tokens: 320,
                safety_margin_tokens: 120,
            },
        };

        let output = PromptAssembler::assemble(input);

        let conversation = output
            .section_trace
            .iter()
            .find(|trace| trace.kind == PromptSectionKind::Conversation)
            .expect("conversation trace");
        assert!(!conversation.included);

        let schema = output
            .section_trace
            .iter()
            .find(|trace| trace.kind == PromptSectionKind::OutputSchema)
            .expect("schema trace");
        assert!(schema.included);
        assert!(output.system_prompt.contains("[Tool Protocol]"));
    }

    #[test]
    fn prompt_budget_truncates_history_before_drop() {
        let history = "x".repeat(50_000);
        let input = PromptAssemblyInput {
            agent_id: "agent-1",
            base_system_prompt: "base prompt",
            short_term_goal: "short goal",
            long_term_goal: "long goal",
            observation_json: "{\"time\":1}",
            module_history_json: history.as_str(),
            conversation_history_json: "[]",
            memory_digest: Some("obs@T1 ..."),
            step_context: PromptStepContext {
                step_index: 0,
                max_steps: 4,
                module_calls_used: 0,
                module_calls_max: 3,
            },
            harvest_max_amount_cap: 100,
            prompt_budget: PromptBudget {
                context_window_tokens: 8_192,
                reserved_output_tokens: 256,
                safety_margin_tokens: 128,
            },
        };

        let output = PromptAssembler::assemble(input);
        let history = output
            .section_trace
            .iter()
            .find(|trace| trace.kind == PromptSectionKind::History)
            .expect("history trace");
        assert!(history.included);
        assert!(history.emitted_tokens < history.estimated_tokens);
    }

    #[test]
    fn prompt_budget_keeps_soft_sections_when_budget_is_sufficient() {
        let history = "x".repeat(3_000);
        let input = PromptAssemblyInput {
            agent_id: "agent-1",
            base_system_prompt: "base prompt",
            short_term_goal: "short goal",
            long_term_goal: "long goal",
            observation_json: "{\"time\":1}",
            module_history_json: history.as_str(),
            conversation_history_json: "[]",
            memory_digest: Some("obs@T1 ..."),
            step_context: PromptStepContext {
                step_index: 0,
                max_steps: 4,
                module_calls_used: 0,
                module_calls_max: 3,
            },
            harvest_max_amount_cap: 100,
            prompt_budget: PromptBudget::default(),
        };

        let output = PromptAssembler::assemble(input);
        let history = output
            .section_trace
            .iter()
            .find(|trace| trace.kind == PromptSectionKind::History)
            .expect("history trace");
        assert!(history.included);
        assert_eq!(history.emitted_tokens, history.estimated_tokens);
    }

    #[test]
    fn prompt_budget_peak_targets_are_below_effective_budget() {
        let budget = PromptBudget {
            context_window_tokens: 4_608,
            reserved_output_tokens: 896,
            safety_margin_tokens: 512,
        };
        let step = PromptStepContext {
            step_index: 0,
            max_steps: 4,
            module_calls_used: 0,
            module_calls_max: 3,
        };

        let effective = budget.effective_input_budget_tokens();
        let (soft, hard) = PromptAssembler::peak_targets_tokens(step, effective);

        assert!(soft <= hard);
        assert!(hard < effective);
    }

    #[test]
    fn prompt_budget_peak_targets_are_stricter_near_finalize_phase() {
        let budget = PromptBudget {
            context_window_tokens: 4_608,
            reserved_output_tokens: 896,
            safety_margin_tokens: 512,
        };

        let early = PromptStepContext {
            step_index: 0,
            max_steps: 4,
            module_calls_used: 0,
            module_calls_max: 3,
        };
        let near_finalize = PromptStepContext {
            step_index: 3,
            max_steps: 4,
            module_calls_used: 2,
            module_calls_max: 3,
        };

        let effective = budget.effective_input_budget_tokens();
        let (_, early_hard) = PromptAssembler::peak_targets_tokens(early, effective);
        let (_, finalize_hard) = PromptAssembler::peak_targets_tokens(near_finalize, effective);

        assert!(finalize_hard < early_hard);
    }

    #[test]
    fn prompt_budget_soft_caps_are_relaxed_for_stability_phase() {
        let early = PromptStepContext {
            step_index: 0,
            max_steps: 4,
            module_calls_used: 0,
            module_calls_max: 3,
        };
        let near_finalize = PromptStepContext {
            step_index: 3,
            max_steps: 4,
            module_calls_used: 2,
            module_calls_max: 3,
        };

        assert_eq!(PromptAssembler::soft_section_caps(early), (256, 192));
        assert_eq!(
            PromptAssembler::soft_section_caps(near_finalize),
            (192, 128)
        );
    }

    #[test]
    fn prompt_budget_peak_targets_use_relaxed_reserve_values() {
        let step = PromptStepContext {
            step_index: 0,
            max_steps: 4,
            module_calls_used: 0,
            module_calls_max: 3,
        };
        let near_finalize = PromptStepContext {
            step_index: 3,
            max_steps: 4,
            module_calls_used: 2,
            module_calls_max: 3,
        };
        let budget_tokens = 3_328;

        let (early_soft, early_hard) = PromptAssembler::peak_targets_tokens(step, budget_tokens);
        let (finalize_soft, finalize_hard) =
            PromptAssembler::peak_targets_tokens(near_finalize, budget_tokens);

        assert_eq!(early_hard, 3_200);
        assert_eq!(early_soft, 3_072);
        assert_eq!(finalize_hard, 3_136);
        assert_eq!(finalize_soft, 3_008);
    }

    #[test]
    fn prompt_budget_peak_budget_enforces_hard_target_on_large_inputs() {
        let history = "h".repeat(14_000);
        let memory = "m".repeat(6_000);
        let step_context = PromptStepContext {
            step_index: 0,
            max_steps: 4,
            module_calls_used: 0,
            module_calls_max: 3,
        };
        let budget = PromptBudget {
            context_window_tokens: 4_608,
            reserved_output_tokens: 896,
            safety_margin_tokens: 512,
        };
        let input = PromptAssemblyInput {
            agent_id: "agent-1",
            base_system_prompt: "base prompt",
            short_term_goal: "short goal",
            long_term_goal: "long goal",
            observation_json: "{\"time\":1}",
            module_history_json: history.as_str(),
            conversation_history_json: "[]",
            memory_digest: Some(memory.as_str()),
            step_context,
            harvest_max_amount_cap: 100,
            prompt_budget: budget,
        };

        let output = PromptAssembler::assemble(input);
        let (_, hard_target) = PromptAssembler::peak_targets_tokens(
            step_context,
            budget.effective_input_budget_tokens(),
        );

        // Keep prompt near hard target while allowing limited drift when industrial guidance expands.
        let tolerated_hard_target = hard_target + 512;
        assert!(
            output.estimated_input_tokens <= tolerated_hard_target,
            "estimated_input_tokens={} hard_target={} tolerated_hard_target={}",
            output.estimated_input_tokens,
            hard_target,
            tolerated_hard_target
        );
    }

    #[test]
    fn prompt_assembly_includes_tool_call_constraints() {
        let output = PromptAssembler::assemble(sample_input());
        assert!(output.system_prompt.contains("每轮只允许调用一个 tool"));
        assert!(output.system_prompt.contains("message_to_user"));
        assert!(output.user_prompt.contains("[Output Hard Rules]"));
        assert!(output.user_prompt.contains("agent_submit_decision"));
        assert!(output.user_prompt.contains("message_to_user"));
    }

    #[test]
    fn prompt_assembly_omits_step_meta_hints_in_dialogue_mode() {
        let mut input = sample_input();
        input.step_context.step_index = 2;
        input.step_context.max_steps = 4;
        input.step_context.module_calls_used = 2;
        input.step_context.module_calls_max = 3;

        let output = PromptAssembler::assemble(input);
        assert!(!output.user_prompt.contains("[Step]"));
        assert!(!output.user_prompt.contains("module_calls_remaining"));
        assert!(!output.user_prompt.contains("turns_remaining"));
    }

    #[test]
    fn prompt_assembly_includes_harvest_max_amount_cap() {
        let mut input = sample_input();
        input.harvest_max_amount_cap = 42;

        let output = PromptAssembler::assemble(input);
        assert!(output.user_prompt.contains("<i64 1..=42>"));
        assert!(output.user_prompt.contains("\"max_ticks\":<u64 1..=3>"));
        assert!(output.user_prompt.contains("不超过 42"));
        assert!(output.user_prompt.contains("硬裁剪到 3"));
        assert!(
            output
                .user_prompt
                .contains("move_agent.to 不能是当前所在位置")
        );
        assert!(output.user_prompt.contains("transfer_resource"));
        assert!(output.user_prompt.contains("buy_power"));
        assert!(output.user_prompt.contains("sell_power"));
        assert!(output.user_prompt.contains("place_power_order"));
        assert!(output.user_prompt.contains("cancel_power_order"));
        assert!(output.user_prompt.contains("refine_compound"));
        assert!(output.user_prompt.contains("build_factory"));
        assert!(output.user_prompt.contains("schedule_recipe"));
        assert!(output.user_prompt.contains("factory.smelter.mk1"));
        assert!(output.user_prompt.contains("recipe.smelter.iron_ingot"));
        assert!(
            output
                .user_prompt
                .contains("post_onboarding 工业主线优先级")
        );
        assert!(output.user_prompt.contains("publish_social_fact"));
        assert!(output.user_prompt.contains("challenge_social_fact"));
        assert!(output.user_prompt.contains("adjudicate_social_fact"));
        assert!(output.user_prompt.contains("revoke_social_fact"));
        assert!(output.user_prompt.contains("declare_social_edge"));
        assert!(output.user_prompt.contains("open_governance_proposal"));
        assert!(output.user_prompt.contains("cast_governance_vote"));
        assert!(output.user_prompt.contains("resolve_crisis"));
        assert!(output.user_prompt.contains("grant_meta_progress"));
        assert!(output.user_prompt.contains("open_economic_contract"));
        assert!(output.user_prompt.contains("accept_economic_contract"));
        assert!(output.user_prompt.contains("settle_economic_contract"));
        assert!(output.user_prompt.contains("factory_kind 当前支持"));
        assert!(output.user_prompt.contains("recipe_id 当前支持"));
        assert!(output.user_prompt.contains("recipe.smelter.copper_wire"));
        assert!(output.user_prompt.contains("recipe.smelter.polymer_resin"));
        assert!(output.user_prompt.contains("recipe.smelter.alloy_plate"));
        assert!(output.user_prompt.contains("recipe.assembler.gear"));
        assert!(output.user_prompt.contains("recipe.assembler.sensor_pack"));
        assert!(output.user_prompt.contains("recipe.assembler.module_rack"));
        assert!(output.user_prompt.contains("recipe.assembler.factory_core"));
        assert!(
            output
                .user_prompt
                .contains("默认 recipe_hardware_cost_per_batch")
        );
        assert!(output.user_prompt.contains("iron_ingot=2"));
        assert!(output.user_prompt.contains("control_chip=2"));
        assert!(output.user_prompt.contains("factory_core=8"));
        assert!(
            output
                .user_prompt
                .contains("schedule_recipe.batches 必须 <= floor(self_resources.data")
        );
        assert!(output.user_prompt.contains("compound_mass_g >= 1000"));
        assert!(
            output
                .user_prompt
                .contains("owner 字段仅允许 self/agent:<id>/location:<id>")
        );
        assert!(
            output
                .user_prompt
                .contains("publish_social_fact.confidence_ppm 必须在 1..=1000000")
        );
        assert!(
            output
                .user_prompt
                .contains("adjudicate_social_fact.adjudication 仅允许 confirm/retract")
        );
        assert!(output.user_prompt.contains("[Failure Recovery Policy]"));
        assert!(
            output
                .user_prompt
                .contains("insufficient_resource.data -> mine_compound")
        );
        assert!(
            output
                .user_prompt
                .contains("insufficient_resource.electricity -> harvest_radiation")
        );
        assert!(
            output
                .user_prompt
                .contains("factory_not_found -> build_factory")
        );
        assert!(
            output
                .user_prompt
                .contains("agent_already_at_location -> 禁止重复 move_agent")
        );
        assert!(output.user_prompt.contains("推荐 harvest 模板"));
        assert!(output.user_prompt.contains("推荐 move 模板"));
        assert!(output.user_prompt.contains("推荐 gameplay 模板（简版）"));
        assert!(output.user_prompt.contains("推荐 economic 合约链路"));
        assert!(output.user_prompt.contains("治理节奏建议"));
        assert!(
            output
                .user_prompt
                .contains("优先切换到另一种 gameplay 动作")
        );
    }
}
