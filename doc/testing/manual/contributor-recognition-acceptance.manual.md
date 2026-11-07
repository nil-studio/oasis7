# 有限预览贡献认可验收手册

- Owner role：`qa_engineer`；协作：`liveops_community`、`producer_system_designer`、原 reward execution owner。
- 生命周期：`active` 验收计划；Last reviewed：2026-11-05。以下是独立验证方法，没有执行真实审核、发放、退款或玩家流程。
- 产品范围：[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001)与[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002)及其原 AC；专业合同：[贡献 intake、审核与回访](../../readme/governance/contributor-recognition-intake-and-review.design.md)。
- 评分与金额主责：[reward pack PRD](../../readme/governance/readme-limited-preview-contributor-reward-pack.prd.md)；台账/批准/执行主责：[ledger PRD](../../readme/governance/readme-limited-preview-contributor-reward-ledger.prd.md)。本手册不决定新金额、汇率、权益、渠道资格或真实服务承诺。

Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5、O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a。

## 1. 固定输入、层次和判据

固定 source/tested tree、原计划/round/purpose、窗口及其时区/边界、当前评分政策版本、受支持渠道、审核/批准/执行角色与当前权限。测试材料包含可核主贡献 C1/C2、同贡献的跨渠道引用、不同贡献者的归属争议、证据的可用/失效版本、两版决定和原执行状态。材料和实际记录均用各自身份，不把测试贡献写入历史运营轮次。

确定性 `test_tier_required` 先核真实 intake parser、有限主记录/别名索引、决定更新和实际反馈消费者；手工建立 expected 后再送入受测路径。`test_tier_full` 选择当前已获准且真实可达的渠道及隔离测试执行入口，核并发、窗口关闭、权限和查询恢复。真实发放另须原授权；只验证合同或只读反馈时执行次数必须为零。当前缺主记录写入者、跨渠道消费者、批准/执行查询或真实反馈入口时分别记录 blocked/inconclusive，不以 Markdown 模板或 PR 导入脚本存在代签完整链路。

记录原 source 对象/版本、intake/主贡献/决定/分配与原执行引用、来源可用性、当前资格、角色、前后索引和玩家反馈。证据原文按当前读权保管；公开报告仅保留必要引用/摘要，不收集密钥、私人聊天全文或已删除源内容。不可变 trace 指原事实和后继关系不能静默覆盖，不等于有权永久保存或继续展示所有原文。

## 2. 独立方法

<a id="contributor-recognition-01-intake"></a>
### RI01：五类来源和审核/权益分离

对应[来源接收](../../readme/governance/contributor-recognition-intake-and-review.design.md#recognition-intake-source)。为当前计划真实支持的 `issue / PR / DM / content / session summary` 各建立一份可核材料，另外为一个不支持渠道建立明确反例。每个输入先记录原件身份、范围、作者/来源证明、取得时点及读权，再进入原专业接收入口；不能以任意聊天、感谢或客户端标签制造正式计划准入。

| 输入/操作 | 独立预期 |
| --- | --- |
| PR intake 明确 yes、字段完整、原窗口和范围合法 | 保持原 `ready` 导入结果；只能生成 draft/captured 类候选，再核主记录和审核条件。不自动填批准人、Approval ID、实际金额、Distribution Ref，不发币/授 World 资格。 |
| PR 缺 Reward Account；未请求；保留区块但 request 非 yes | 分别沿原 `deferred`、`no_reward_review_requested`、`invalid_intake` 合同处理。后两者不自动创建 reward ledger row；公开 key 或显示名不能替代 Reward Account。已存在的历史 intake/决定另行保留。 |
| issue/DM/content/session summary 的真实受支持输入 | 收到状态只能证明原渠道确实接收；进入计划须保留其实际范围、证据及核验条件。未授权私信不能被公开复制，session summary 自述不替代原事实。缺输入时说清无证据、待补或未进入该计划。 |
| 未支持渠道、审核者不可用、证据暂不可读 | 返回该范围的真实延期/关闭/正确入口或等待说明。不能由“已收到”推断已入账、优先审核、获奖或未来访问权，也不伪造已发送的反馈。 |
| 状态逐步推进 | 分别显示已收到/待核验、核验中、结果记录、需补/延期/关闭，以及独立的已批准和已执行。每一显示能回到准确原 canonical 记录；approved 无执行证据时不得显示 distributed。 |

对原导入脚本的实际定义和测试只按其 PR 输入层取证；其他四类输入、真实人工核验和玩家反馈分别记录结果，不能用一个 PR 正例代签全部渠道。

<a id="contributor-recognition-02-dedup"></a>
### RI02：稳定贡献、归属和跨渠道/跨轮次去重

对应[贡献主身份](../../readme/governance/contributor-recognition-intake-and-review.design.md#recognition-contribution-identity)。建立一个真实成果 C1，再分别用 PR、对应 issue、DM 引用、转载和 session summary 申报同一成果；另备实际独立成果 C2，其新增价值、原件与区别经原 reviewer 核实。

1. C1 的五份申报保留各自不可变 intake 引用，指向同一主贡献；重试、重新扫描、换标题/handle/账号/Reward Account 只增必要关联或返回原件，不增加第二份 base、优先权或付款依据。alias 不能成为可重新批准的主项。
2. C2 只有在可独立核验的不同事实/成果成立时成为新主项；相同 bug 的 issue、修复说明与摘要不因类型标签不同各拿一次 base。引用数量、渠道数量和提交顺序均不决定贡献价值。
3. 以两个主体先后/并发申报 C1，不能以 first-submit 抢定作者或收款归属。保留原投稿时间事实、共同署名与争议，核当前合法证明；争议只暂停受影响主项。无关已核 C2 可以继续，汇总明确列出纳入/排除项及原因。
4. 同时导入同一 PR、两条渠道同时建主项、一次来源更名及一次 reviewer revision 交错。最终有效索引只有一个主项及确定的后继；失败方不覆盖成功方的决定或重新取得审核机会。重复导出行只是候选，不能因此获得第二项权威认可。
5. 将 C1 移入另一个 round/purpose/推荐包、关闭后重新注册，保留其原主身份、窗口和已批准/执行引用。原主项不能因换容器再计一次权益或再次推荐支付；真正新成果仍须证明独立身份及当期资格。
6. 已先分为 C1a/C1b 后查明同一成果，追加准确 merge/alias/supersession 和原因。保留之前 intake、评分、批准与实际执行的历史数值；当前可依赖集合去重，未执行部分重新核定。不得抹去一次真实发放，也不得把合并当成自动退款/追债或新补发权限。

证据比较原件集合、主项与别名索引、归属证明、有限推荐集合、批准/分配条目和实际执行数；不能只断言 `Duplicate Check=unique` 文本存在。无法核清身份或索引完整性时，受影响项保留待核和真实下一步，不猜全网唯一。

<a id="contributor-recognition-03-scoring"></a>
### RI03：评分单元、档位与金额护栏

对应[评分接收](../../readme/governance/contributor-recognition-intake-and-review.design.md#recognition-scoring-policy)及原 pack 的具体政策版本。先独立判断贡献的证据、归属、范围、窗口与去重资格；缺项不能借正分变为合格。每个合格主贡献只有一个 base 类别和一个质量修正，`s=max(0,base+q)`，C07 固定0。有限推荐集合给出成员 ID/版本、纳入/排除原因、总分和政策版本；成员分数/档位与集合汇总档位分开。

| 可核输入 | 独立预期 |
| --- | --- |
| 独立 blocking bug C1：40+20；另一个主题的独立 docs C2：20+10；两者属同贡献者、同原 round/purpose 且都合格 | 成员分别60/medium、30/small；集合总分90/large。历史价值和实际新增价值按原审核规则判断，large 只是汇总建议，不创建第三条付款行、不将两成员都改成large。 |
| 只提交一个 merged PR，base40，分别满足+20和+10描述 | 按原政策选一个适用修正；最高60/medium，不能相加为70，也不能将“已合入”同时重复作为第二类 base。 |
| C1 在三渠道申报，加上同一修复的说明/转载 | 仍只有同一个主贡献60；不得得到180或另加文档 base。只有已证明不同新增成果才进入另一主项。 |
| C07 登录/在线/闲置，试图附+20；虚假或不可核材料 base40−20 | C07恒0；失证或归属未核的条目不进入合格集合/批准。其诊断分即使20也不产生small资格，负向修正不替代证据 gate。 |
| 合格集合分数15、20、45、50、85、90 | 分别 none、small、small、medium、medium、large；缺证项明确排除，不把未知记为0后隐藏分母。金额仍没有 point→OC 汇率。 |
| 一个普通 merged PR 的共同贡献分配80+80 OC | 总160超过原主项150 OC默认 ceiling；分配成多行/改渠道或类型标签不规避原限制。没有明确合法 exceptional note 及原 producer 决定，不能进入执行。 |
| 普通 merged PR 150 OC；151 OC无例外；1500 OC只因large标签 | 150仍须真实批准与actual-value review；151无例外拒绝推进；1500不能从档位推导，只能取原极少数 round-specific exceptional 决定。两个PR合成large不扩大任何单项额度。 |
| C1已批准/发放，后来加入新C2形成后继集合 | 旧评分、批准和执行不被改写，C1不因再次出现在说明性历史集合而重新推荐支付；当前可批准增量只能来自尚未被原主项消费、且真实有资格的部分。 |

分别保存独立算式、成员原件及资格、集合前后版本、每项金额/例外和 actual-value review。若 producer 下调原计划，必须在执行前追加可追溯决定及原因；不能只改展示总数或保留旧高额执行输入。

<a id="contributor-recognition-04-decisions"></a>
### RI04：不可变决定与证据失效的当前含义

对应[决定生命周期](../../readme/governance/contributor-recognition-intake-and-review.design.md#recognition-decision-lifecycle)。分别在 captured、reviewed、approved 未执行、部分执行、distributed、归档后撤回/删除/更正证据；另测只是暂时不可访问和事实已被推翻，两者原因不得混同。

1. 原 intake/decision 保留其 ID、输入版本/来源引用、当时状态、理由、决定者和时间；变更追加后继 revision，不能把原 approved/distributed 行覆盖成仿佛从未批准的 draft。
2. 当前可依赖资格、证据可用性与历史决定分列。证据已删除不展示仍可访问；可以保留合法引用/摘要和可用性理由，不要求重建或永久存放被删内容。暂时不可读不得自动推定欺诈。
3. 未执行部分按原权限进入当前 hold/复核，阻止新的分配与对外签发准入，不凭旧 approved 标签取得新许可。已经登记对外签发或可能投递的原件不能由本地 hold 宣称停止；未确认结果继续占用并按原件查询。已经真实执行部分保留实际金额、账户和执行引用；不创造惩罚性债务、追回、冻结世界资产、自动补发或另一次奖励。
4. 合法补充只能沿原计划允许的真实窗口和入口，核新来源/归属/范围后形成后继决定；恢复一个链接不自动恢复批准、延长窗口或复活旧执行请求。若原窗口不允许补充，明确关闭/原适用路径。
5. 两 reviewer 基于同一旧 revision 分别批准/拒绝；批准与证据失效交错。只能有一条可核当前决定链，旧输入不能覆盖新 hold 或把已失效成员重新加入当前集合；冲突必须查询当前原件并重新明确决定。
6. 原审核者失去权限、渠道退出、主体归属不明或计划结束时，保留其过去合法决定和真实执行，后续只能由当前原 authority 处理；不靠临时换人/新账号给未决项新的资格。

核对每个状态的玩家说明、当前执行资格、历史 trace 和真实副作用。没有新增转账不代表通过：若错误显示“已撤销付款”或“自动恢复资格”，同样失败。

<a id="contributor-recognition-05-distribution"></a>
### RI05：批准到实际执行的唯一接收与未知结果

对应[执行消费者](../../readme/governance/contributor-recognition-intake-and-review.design.md#recognition-distribution-consumer)。用隔离的、已有原授权的执行 fixture 核验合同；真实链上/多签发放另须原执行授权。本方法不新增支付 API、签名权或资金来源。

| 序列 | 独立预期 |
| --- | --- |
| 完整当前批准、actual-value review、账户与执行权限均有效 | 每项分配关联准确主贡献/决定/原有效分配族及原执行记录；主贡献占用与对外签发登记分别按当前 ledger revision 接收，后者在任何 proposal/签名材料离开 ledger 前重验当前 hold、批准、金额和账户。准入、提交与真正执行分别记状态。只有原执行 authority 的可核确认才能写实际金额/Distribution Ref/distributed。 |
| 只有计划、建议档位、ready/draft、旧批准或缺账户 | 不进入执行，不从分数生成金额。缺字段明确等待补充/复核，不伪造已批准或已发送。 |
| 批准后 Reward Account/金额/归属/成员集合变化 | 旧批准只覆盖原内容；按当前核验与原批准路径明确修订后才可执行新内容。格式合法或一个新handle不能证明有权更换收款人。已执行的旧账户/金额保留。 |
| 发送后ACK丢失/网络超时，重试或从另一渠道催办 | 原执行状态unknown，先查同一原执行引用和批准分配；查询不可用仍保持unknown。不能新建付款、换分配ID再次提交，也不能把超时记为确定失败或退款成功。 |
| 查询确认已执行；确认没有提交；原执行明确失败 | 已执行只恢复原结果；释放占用或恢复尝试须由原执行 authority 同时证明原件未生效且不能再生效，不能只凭未查到、超时或猜测 nonce。满足该条件后也仅沿原执行合同在当前批准仍有效时处理，保留原逻辑分配与尝试历史。不会生成第二个认可依据或突破原总额。 |
| 两执行者使用相同或不同 allocation ID 消费同一主贡献/批准/有效分配族 | 当前主贡献占用只能接受一次；改 ID、渠道或执行者也不能另开第二占用。分别在占用前后、对外签发登记前后插入 hold/账户变更，旧 revision 不得覆盖当前决定；登记前不再取得新签发许可，登记后原件可能已在外部，不能声称本地暂停已取消。不能证明原件不再生效时保持原 unknown 占用并查询，不重付。 |

对账每个主项的批准上限、有效分配之和、已执行和未执行部分及原余额来源，保留全部实际执行引用。实际执行系统尚无必要唯一性/查询能力时，标出该执行前置缺口并停止受影响动作；手工台账的分配ID不能自证支付系统已经去重。

<a id="contributor-recognition-06-feedback"></a>
### RI06：玩家反馈、回访与关闭后恢复

对应[反馈恢复](../../readme/governance/contributor-recognition-intake-and-review.design.md#recognition-feedback-recovery)与[有限读取](../../readme/governance/contributor-recognition-intake-and-review.design.md#recognition-bounds)。在本轮真实支持的每种渠道，使用同一 intake/主记录分别读取初次反馈、审核变化、证据失效、窗口关闭及返回查询；有界批量读取须保留完整分页/范围，不能截断决定链或别名后假装没有记录。

1. 玩家能读到原计划/窗口/贡献范围、当前状态、来源引用或合规摘要（无可用证据时明确说明）、是否需补充、主要 blocker 和真实下一步。感谢可以作为文案，不能代替这些结果。
2. 同贡献跨渠道返回同一原主记录及重复/合并关联；不会分配新的优先级、重置最初窗口或重新承诺审核。新设备/重连/换handle继续按当前身份和读权查询，不凭知道ID读取私人材料。
3. 非法 reader、失权 reviewer、公共渠道、私有DM各自按当前权限裁剪；拒绝不泄露他人收款账户、私信正文或未公开审核材料。可合法公开的历史事实与私有证据正文分开判断。
4. 读取失败、过期缓存或记录版本混合时明确“当前未确认”；保留原引用，不推断被拒/没有发放/可以重新申报。恢复后取当前原记录并呈现后继关系，不靠创建新申请刷新状态。
5. 计划窗口结束、渠道退出、身份未核或没有补证入口时，按原计划提供延期、关闭、适用路径或停止；只有真正允许的补充窗口才可提出补证。转入另一个计划也不清除已消费的原主贡献。
6. 比较汇总与成员、历史决定与当前可依赖状态、批准与实际执行。玩家不得从90分/large、已审核、已记录或某个入口标签推导 token、World OC、治理票、访问资格或未来合作权。
7. 在完整有限 manifest 中，以完整分片计划中两批各自同时满足64主项、256事件和1 MiB约束的 N=65 小元数据样本分两批准备，最终核全部成员、排除项、版本、计数与摘要；不能把前64项冒充全体。分别核单次64主项、256事件/revision、1,048,576 UTF-8 bytes元数据的边界与越界；附件只保原引用。两个各600 KiB的合法单项虽 N=2 也须按总字节分成两批，批数不能只取 ceil(N/64)。不可分的合并/重审超限时完整阻断，不截尾、半写或静默放宽 profile；最终结论引用完整准备计划与实际各批，而非遗漏项的局部总分。

证据包括实际渠道的请求/原反馈、授权结果、准确记录版本、可读说明及0次额外分配/提交。查询期间若原在途支付真实确认，必须如实记录其原执行结果，不把它抹成0或改称本次重付。表格或API对象具备字段不足以证明玩家入口已接收；各入口缺口和真实可读性单列。

## 3. 完成与交接

每项记录 method ID、固定候选、实际源/authority、独立 expected、观察结果、单变量反例、恢复及未覆盖层次。仅对真实执行且满足正常/拒绝/恢复的范围给 verdict；历史轮次与真实付款不改写为本次测试证据。文档、链接、算式和模板检查不代签人工审批、实际跨渠道运行、支付唯一性或真人理解。

实际运行结果、缺陷和修复复测留在对应 PR/Task；发布后仍按原产品边界表达参与、认可、独立批准与真实执行，不把本手册或设计采纳作为公开发放承诺。
