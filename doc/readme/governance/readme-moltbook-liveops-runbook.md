# Moltbook 运营 Runbook

审计轮次: 6

## Meta
- Owner Role: `liveops_community`
- Review Role: `producer_system_designer`
- Channel: `Moltbook`
- Last Platform Verified: `unverified in current environment`
- Verification Failure Policy: `fail_closed`；平台机制或发布可见性无法确认时，只允许人工检查、记录 blocker 或暂停，不自动发帖
- Scope: `发帖前复核 + 发帖后巡检 + 评论分级 + GitHub 回流 + 实际 PR 或已有 Task 的证据回写`
- Source Docs:
  - `doc/product/player-entry-distribution/release-communications-and-public-claims.prd.md`
  - `doc/readme/governance/readme-moltbook-post-pack.md`

## 1. 适用范围
- 这份文档用于 `oasis7` 在 Moltbook 进入“持续发帖与持续看反馈”阶段后的日常运营。
- 本文不替代产品公开口径或帖文素材库，只定义执行、回复、升级和回写方式。
- 任何真实外发内容都必须重新绑定根 README 当前 claim；渠道 presence 不代表 Moltbook identity/provider/on-chain 产品集成、合作或正式发布。

### 1.1 稳定渠道策略

- 主要受众按 agent builder / creator / observer 区分；内容采用 proof-first、build-in-public 与“一帖一个 CTA”，避免泛 AI/Web3 愿景堆叠。
- 稳定内容支柱为 World Proof、Agent Behavior、Builder Question、Co-Creation；每次使用都必须重新绑定当前公开 claim、受支持入口与证据。
- 产品、缺陷与合作信号分别回流正式 owner；互动量、关注或回复不能升级 roadmap、readiness 或公开承诺。

## 1.2 执行环境与平台预检
- 凭据只存在于受控 secret store；文档、日志、截图和命令输出不得记录 token、cookie 或本地 secret 路径。
- 执行前记录绝对时间与官方验证来源，分别确认浏览器、API 和网络/proxy 链路；任一路径成功不能代签另一条路径。
- 若平台机制、endpoint 或发布可见性无法复验，停止自动发布；仅执行人工检查、记录 blocker 或等待 owner 恢复权限。

## 2. 发帖前检查
每次发帖前，按下面顺序做一次 2-5 分钟复核：

1. 确认主贴使用已批准的文案或其安全变体。
2. 确认首评已准备好，且承担更长解释、链接或补充说明。
3. 确认主贴没有以下表述：
   - `live now`
   - `play now`
   - `public launch`
   - `official Moltbook integration`
4. 确认外链只指向当前稳定公开入口。
5. 确认素材与主张一致，不拿 `software_safe` 或 `pure_api` 去替代 3D 视觉 claim。
6. 记录发帖时间、帖子标题和 post id，便于后续巡检和实际 PR 或已有 Task 的证据回写。

## 3. 发帖后 24 小时巡检
前 24 小时是高价值窗口。建议最少按以下节奏检查：

- `T+15m`：确认帖子已正常可见，主贴/首评没有格式问题。
- `T+1h`：第一次看通知和评论，处理明显误解。
- `T+4h`：第二次看互动，筛合作线索和真实问题。
- `T+24h`：做一次小结，决定是否继续跟评、追问或回流 owner。

每次检查都按固定顺序：

1. 先看 `GET /api/v1/home`
   - 目标：快速判断有没有未读通知、哪些帖子有新活动。
2. 再看 `GET /api/v1/notifications`
   - 目标：区分“新关注 / 点赞 / 评论 / 回复”。
3. 如果指向某条帖子，再看：
   - `GET /api/v1/posts/:id`
   - `GET /api/v1/posts/:id/comments?sort=new&limit=35`
4. 如需人工确认公开呈现，再查看 profile 或 post page。

注意：
- `home` 有未读不等于有评论。
- 如果 `activity_on_your_posts` 为空但未读数不为 0，先怀疑是新关注或其他通知，不要误判成“有人留言”。

## 4. 常规日巡检
没有新帖的常规日，也建议至少 1 次轻巡检：

1. 看 `/home` 是否有新的帖子活动或未读通知。
2. 看最新 1-3 条自家帖子是否出现迟到评论。
3. 检查是否有值得补一句 follow-up comment 的高质量讨论。
4. 把需要跨角色处理的内容记录到对应 PR、实际 CI 与评审记录（Issue 按需）。

如果当天有新帖或外部讨论升温，可加到 2-3 次。

## 5. 评论与通知分级
统一分成 4 桶：

### P1: 口径风险 / 状态误解
- 典型问题：
  - “已经上线了吗？”
  - “现在能玩吗？”
  - “是不是已经和 Moltbook 集成了？”
- 动作：
  - 直接回复。
  - 第一优先是纠偏，不是扩写愿景。
- 安全方向：
  - 重申 `limited playable technical preview`
  - 重申“这里只是平台原生推广，不代表正式集成已宣布”

### P1: 合作 / 集成 / 路线图追问
- 典型问题：
  - “什么时候做 identity / onchain / provider？”
  - “能不能一起做某个合作？”
- 动作：
  - 可以礼貌确认“这是有价值信号”。
  - 不公开承诺时间、范围或 owner。
  - 同步升级给 `producer_system_designer`。

### P2: 真实 bug / friction / 文档缺口
- 典型问题：
  - “我试了预览，这里报错”
  - “文档没看懂 / 缺步骤”
  - “某个 surface 很 rough”
- 动作：
  - 优先引导到 GitHub `issue`。
  - 如果对方已有修复方案，优先引导到 GitHub `PR`。
  - 内部再同步给 `qa_engineer` 与对应工程 owner。

### P2: 机制与设计讨论
- 典型问题：
  - “为什么要做 `pure_api`？”
  - “当前两个玩家 access surface（`viewer / pure_api`）的边界是什么？`software_safe` 只是 `viewer` compatibility alias。”
- 动作：
  - 可以直接回答。
  - 尽量用已批准主张和具体证据回答，不讲未落地 roadmap。

### P3: 纯情绪互动
- 典型问题：
  - “cool”
  - “interesting”
- 动作：
  - 可简短互动，也可不消耗太多精力。
  - 不需要升级。

## 6. 回复边界
公开回复时默认遵守以下顺序：

1. 先校正状态边界。
2. 再给最小必要解释。
3. 最后给明确下一步动作。

推荐句型：
- `Still a limited playable technical preview.`
- `This thread is not announcing formal Moltbook integration.`
- `If you tried the preview and hit a rough edge, the best next step is a GitHub issue.`
- `If you already have a fix in mind, a PR is even better.`

不要做的事：
- 不在评论区承诺发布日期。
- 不在评论区承诺合作已确定。
- 不在评论区把探索性方向说成 roadmap。
- 不把渠道互动区当成长期 debug 线程。

## 7. GitHub 回流规则
- 外部反馈属于 `bug / friction / missing docs`
  - 引导到 GitHub `issue`
- 外部反馈属于 `I can fix this / I want to submit a change`
  - 引导到 GitHub `PR`
- 外部反馈属于 `feature idea / product direction`
  - 可先在评论里收一句上下文，再内部回流 `producer_system_designer`

口径要求：
- 用 `after you inspect or try the preview`
- 不用 `after you play the game`

## 8. 升级矩阵
| 场景 | 直接 owner | 升级动作 |
| --- | --- | --- |
| 对外承诺、合作、路线图追问 | `producer_system_designer` | 记录原话、帖子链接、你的拟回复边界 |
| 真实缺陷、兼容性、体验阻断 | `qa_engineer` + 对应工程 owner | 记录 surface、现象、是否已有 GitHub issue |
| 对玩法、世界机制的高价值兴趣 | `producer_system_designer` | 记录兴趣点和频次 |
| 创作者放大、联动意向 | `liveops_community` -> `producer_system_designer` | 先判定是否越界，再决定 follow-up |

## 9. 当日证据回写要求
当天做过 Moltbook 动作后，必须回写对应 PR、实际 CI 与评审记录（Issue 按需）。

最少记录：
- 时间
- 角色：`liveops_community`
- 完成内容：
  - 发了什么
  - 查了什么
  - 有没有评论 / 关注 / 质量信号 / 合作信号
- 遗留事项：
  - 需要谁 follow-up
  - 哪条评论还没回
  - 哪个问题需要升级

推荐附带字段：
- `post_id`
- `signal_tags`
- `owner`
- `next_action`

## 10. 每周复盘
每周至少做一次轻复盘，回答 4 个问题：

1. 哪类帖子最容易引发高质量讨论？
2. 哪类表述最容易引发“是不是已经上线了”的误解？
3. 本周有多少条信号转成了 GitHub `issue` / `PR` / owner follow-up？
4. 下周继续推什么：
   - `world proof`
   - `agent diary`
   - `builder hook`
   - `pure_api`

## 11. 首周运营模板
下面这套模板只服务于冷启动第一周。它假设你已经有：
- 1 个已批准的身份帖
- 1 套首批文案包
- 可回链的 GitHub / 公开入口

### Day 1: Identity
- 主动作：
  - 发布 `Post 1: identity`
  - 补首评，强调 `limited playable technical preview`
- 检查窗口：
  - `T+15m`
  - `T+1h`
  - `T+4h`
- 回复目标：
  - 优先纠正“是不是已上线 / 能不能玩”
  - 至少处理 1-3 条状态误解类评论
- 记录重点：
  - 哪句定位最容易被复述
  - 有没有人直接把项目理解成“正式可玩”

### Day 2: Surfaces
- 主动作：
  - 发布 `Post 2: access surfaces`
  - 如果 Day 1 有高质量问答，可引用其中 1 个问题做 follow-up
- 检查窗口：
  - 上午 1 次
  - 下午 1 次
  - 晚间 1 次
- 回复目标：
  - 优先回答当前两个玩家访问面 `viewer / pure_api` 的边界问题，并明确 `software_safe` 只是 `viewer` compatibility alias
  - 把“同一世界，不同 proof boundary”说清楚
- 记录重点：
  - 哪个 surface 最引发兴趣
  - 有没有人误把 `software_safe` 或 `pure_api` 当成 3D 视觉证明

### Day 3: No New Post, Focus Replies
- 主动作：
  - 不强行发新帖
  - 集中处理 Day 1-2 的延迟评论与 follow-up
- 检查窗口：
  - 中午 1 次
  - 晚上 1 次
- 回复目标：
  - 至少补 2 条高质量机制解释
  - 有 bug / friction 的，明确引导 GitHub `issue`
- 记录重点：
  - 哪类问题已经开始重复出现
  - 哪类问题值得沉淀成固定回复模板

### Day 4: World Proof
- 主动作：
  - 发布 `Post 3: world proof`
  - 用 `before -> action -> after` 结构，不讲空泛愿景
- 检查窗口：
  - `T+15m`
  - `T+2h`
  - `T+8h`
- 回复目标：
  - 把讨论拉回“可观察证据”
  - 引导大家说下一个想看的 subsystem
- 记录重点：
  - 哪类 proof 最能带来高质量讨论
  - 是 `economy / conflict / logistics / agent decision-making` 哪个更受关注

### Day 5: Agent Diary
- 主动作：
  - 发布 `Post 4: agent diary`
  - 重点写 `goal / blocker / recovery path`
- 检查窗口：
  - 上午 1 次
  - 下午 1 次
  - 晚间 1 次
- 回复目标：
  - 鼓励对 agent decision-making 的追问
  - 避免把 agent 能力说成“fully general”
- 记录重点：
  - 大家更关心 agent 的目标、失误还是恢复机制
  - 是否出现新的 overclaim 风险词

### Day 6: Builder Hook
- 主动作：
  - 发布 `Post 5: builder hook`
  - 直接提问，让对方用编号回复优先级
- 检查窗口：
  - `T+30m`
  - `T+3h`
  - `T+24h`
- 回复目标：
  - 至少收集 3 个 builder-facing 信号
  - 对明确贡献意向，统一引导 GitHub `issue` / `PR`
- 记录重点：
  - 大家最先关心 `state observability / action boundaries / recovery / provenance / no-UI`
  - 是否出现可升级给 `producer_system_designer` 的合作线索

### Day 7: Recap And Triage
- 主动作：
  - 发布 `Post 6: week-one recap`，或如果前 6 天信号不足，则不发 recap，只做内部总结
  - 汇总首周互动到 4 个 bucket：
    - `功能兴趣`
    - `技术质疑`
    - `合作线索`
    - `口径风险`
- 检查窗口：
  - 上午 1 次
  - 晚间 1 次
- 回复目标：
  - 把剩余高价值评论全部收口
  - 明确下一周继续推的内容线
- 记录重点：
  - 本周最佳帖型
  - 本周最常见误解
  - 本周转成 GitHub / owner follow-up 的数量

## 12. 参考入口
- `https://www.moltbook.com/skill.md`
- `https://www.moltbook.com/api/v1/home`
- `https://www.moltbook.com/api/v1/notifications`
- `https://www.moltbook.com/api/v1/posts/:id`
- `https://www.moltbook.com/api/v1/posts/:id/comments`
- `doc/product/player-entry-distribution/release-communications-and-public-claims.prd.md`
- `doc/readme/governance/readme-moltbook-post-pack.md`

## 13. 历史实战观察（观察于 2026-03-22，复用前必须重验）
下面这些结论来自当时的真实帖子执行，不是当前平台事实或自动发布许可。13.4–13.6 的投放/下一帖安排只属于 2026-03-22～24 原窗口；唯一原 Task 尚未核实，原记录保留，不恢复排期。可复用 SOP 与内容/风控方法保留，复用前重新核实当前平台和授权。

### 13.1 有效内容设计模式
- `identity + boundary` 仍然是第一帖必需动作，但第一帖更适合做定位，不适合承载太多解释。
- 第二帖如果只是继续解释产品结构，讨论度一般；把话题改成 `builder discussion hook` 后，回复质量明显更高。
- 相比“我们有三个 surface”，更容易引发回复的写法是：
  - 抛一个真实评估问题
  - 给 3-5 个可选检查项
  - 让 builder 直接按优先级回答
- 当前已验证更有效的讨论钩子：
  - `shared world state`
  - `recovery after failure`
  - `no-UI inspection`
  - `proof boundaries`
- 其中 `recovery after failure` 在首轮最容易引发高质量 builder 回复，说明“失败后的可恢复性”比“抽象 persistent world 叙事”更能让人落到具体讨论。

### 13.2 已验证的发帖组织方式
- 主贴负责：
  - 抛问题
  - 立边界
  - 留出讨论入口
- 首评负责：
  - 补偏向性的设计立场
  - 视情况再引到 GitHub
- 回复负责：
  - 沿着用户已经给出的方向继续加深，不要把讨论硬扯回统一宣传口径

### 13.3 spam 风险模式
- 已实测触发风控的高风险组合：
  - 新帖刚发出
  - 官方账号立刻自评
  - 自评里带裸 `GitHub` 链接
- 这类组合即使内容安全，也可能被平台标成 `is_spam: true`。
- 当前可操作的规避策略：
  - 不要在帖子刚发布后第一时间用自评带裸链
  - 优先等待真实评论出现，再在回复里补深度说明
  - 如果必须首评，先写观点或边界说明，不急着放外链
  - 外链更适合在已有讨论上下文后再放出

### 13.4 后续执行调整
- 第一帖：
  - 保持 `identity + technical preview boundary`
  - GitHub 地址可以延后，不必抢在第一屏出现
- 第二帖及之后：
  - 优先发可讨论的问题帖，而不是纯说明帖
  - 先找“值得争论的系统取舍”，再把产品特性嵌进去
- 评论运营：
  - 优先回复真实 builder 评论
  - 让回复本身继续展开一个设计方向，例如 recovery、shared state、inspectability

### 13.5 选题继续下钻：local failure / continuity
- 在 `memory / consequences / recovery after failure` 已经拿到高质量讨论后，下一步不一定要直接切到 `blockchain` 或 `distributed systems` 品牌词。
- 当前更稳的下钻方式是：
  - 继续从 `agent world continuity` 视角提问
  - 把讨论落在 `local failure` 之后什么必须保留
  - 让 builder 自己把话题带向 identity、memory、obligations、shared facts
- 已实测可用的标题与正文变体：
  - `What should survive local failure in an agent world?`
  - `If one part of a world fails, what should still survive?`
  - `Identity? Memory? Obligations? Shared facts?`
- 这类帖子比直接谈 `blockchain` 更稳，因为：
  - 仍然以世界体验和 agent legibility 为中心
  - 允许评论自然延伸到 recovery / consistency / shared truth
  - 不会太早把讨论拖进基础设施立场争论
  - 把 GitHub CTA 放在更自然的 follow-up 节点，而不是每条自评固定复制

### 13.6 `gaming` 试投复盘：话题成立，分类不成立
- 2026-03-23 额外把同一 continuity 题眼改写成：
  - `What should survive local failure in an open sandbox world?`
  - 并投到 `m/gaming`
- 截至 2026-03-24 的直接结果：
  - `introductions` 版 continuity 帖保持 `score=6`、有真实讨论，评论自然落在 `shared facts / sync / recovery`
  - `gaming` 版 sandbox 帖 `score=0`，唯一评论是偏联盟/crypto 招募的噪音回复，且被平台标成 `is_spam=true` / `is_crypto=true`
- 当前可复用的判断：
  - `continuity / shared facts / recovery / legibility` 这些题眼本身是有效的
  - 但把同一问题改包装成 `open sandbox world` 再投 `m/gaming`，并没有自动换来更高相关讨论
  - 对 Moltbook 当前语境来说，用户更容易响应 `agent-native world question`，而不是 `game category`
- 后续执行建议：
  - 继续优先投 `introductions`、`general` 或 builder 更密集的场域
  - 除非正文直接讨论 `agent-vs-agent strategy`、leaderboard、constraints for agents，否则不要把 `gaming` 当默认 submolt
  - 如果确实要碰 `gaming`，优先写“what game systems reveal about agent strategy”，不要只把 persistent world 换个 `sandbox` 标签就发
