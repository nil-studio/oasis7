# 小红书运营 Runbook（2026-03-23）

审计轮次: 3

## Meta
- Owner Role: `liveops_community`
- Review Role: `producer_system_designer`
- Channel: `小红书`
- Scope: `发帖前复核 + 发帖后巡检 + 评论分级 + 互动引导 + 信号回流 + 实际 PR 或已有 Task 的证据回写`
- Source Docs:
  - `site/social/xiaohongshu/intro/intro-post-pack-2026-03-22.md`
  - `site/social/xiaohongshu/team-roster/team-roster-post-pack-2026-03-22.md`
  - `site/social/xiaohongshu/game-intro/game-intro-post-pack-2026-03-24.md`
  - `site/social/xiaohongshu/player-boundary/player-boundary-post-pack-2026-03-25.md`
  - `site/social/xiaohongshu/ai-laziness-game-mode/ai-laziness-game-mode-post-pack-2026-03-26.md`
  - `site/social/xiaohongshu/spring-recruit/spring-recruit-post-pack-2026-03-29.md`
  - `site/social/xiaohongshu/spring-recruit/spring-recruit-carousel-pack-2026-03-29.md`
  - `site/social/xiaohongshu/ai-persona/ai-persona-world-post-pack-2026-03-30.md`
  - `site/social/xiaohongshu/ai-persona/ai-persona-carousel-pack-2026-03-30.md`
  - `site/social/xiaohongshu/demo-skepticism/demo-skepticism-post-pack-2026-03-31.md`
  - `site/social/xiaohongshu/demo-skepticism/demo-skepticism-carousel-pack-2026-03-31.md`
  - `site/social/xiaohongshu/gui-death/gui-death-post-pack-2026-04-01.md`
  - `site/social/xiaohongshu/offer-choice/offer-choice-carousel-pack-2026-04-03.md`
  - `site/social/xiaohongshu/offer-choice/offer-choice-post-pack-2026-04-03.md`
  - `site/social/xiaohongshu/kaifeng-rpg/kaifeng-rpg-post-pack-2026-04-06.md`
  - `site/social/xiaohongshu/cycle/cycle-carousel-pack-2026-04-09.md`
  - `site/social/xiaohongshu/cycle/cycle-post-pack-2026-04-08.md`
  - `site/social/xiaohongshu/future-ownership/future-ownership-post-pack-2026-04-13.md`
  - `site/social/xiaohongshu/token-usage/token-usage-post-pack-2026-04-20.md`
  - `site/social/xiaohongshu/loop-engineering-game-dev/loop-engineering-game-dev-post-pack-2026-06-15.md`

## 1. 适用范围
- 这份文档用于 `oasis7` 在小红书进入“持续发帖与持续看反馈”阶段后的日常运营。
- 本文不替代具体帖子素材包，只定义怎么执行、怎么回复、怎么观察、怎么回流。
- 小红书默认面向人，不面向 agent；这里的叙事主语应优先保持为“人类开发者 / 制作者 / 我们在做什么”，而不是切到 agent 内视角。
- 小红书具体内容包统一放在 `site/social/xiaohongshu/<post-slug>/`；包内 `README.md` 记录可发布导出图、视觉源、预览图、底图和对应文案文档。
- `doc/readme/governance/` 只保留渠道 runbook、跨渠道治理和 README 口径控制；不要再把新小红书素材包平铺回 `doc/readme/governance/` 或 `site/social/` 根目录。

## 1A. 内容包目录规则
每篇小红书内容使用一个 package 文件夹：

```text
site/social/xiaohongshu/<post-slug>/
  README.md
  *-post-pack-*.md
  cover.html / carousel.html
  assets/
  exports/
```

- `README.md`：素材 manifest，列出哪些 PNG 可直接上传、哪些图只用于复查、HTML 源文件在哪里、对应文案/治理文档是哪份。
- `*-post-pack-*.md` / `*-carousel-pack-*.md`：标题、正文、caption、评论区引导、禁语和发布边界。
- `cover.html` / `carousel.html`：可编辑视觉源，保留用于修字、重导出和后续复盘。
- `assets/`：底图、生成源图、参考图等不直接上传的素材。
- `exports/`：最终上传 PNG，以及 `contact-sheet.png` 等复查图；复查图必须在 package README 中标注为 preview/review only。
- `_shared/`：只放跨 package 的模板、脚本、共享样式或复用视觉素材，不放单篇文案或最终导出图。

## 2. 渠道定位
- 小红书的核心任务不是复述游戏设定，而是让人理解：
  - 我们为什么要做这个游戏。
  - 我们为什么用这种方式做。
  - 这个项目和普通“AI 工具辅助开发”有什么区别。
- 小红书更适合：
  - 人类开发者视角
  - 开发过程
  - 设计判断
  - 团队协作
  - 让用户参与猜测、判断、站队的互动问题
- 小红书不适合：
  - 上来就讲完整世界观
  - 纯 release note
  - 生硬导流
  - 把 agent 直接当作对话对象来写

## 3. 发帖前检查
每次发帖前，按下面顺序做一次 2-5 分钟复核：

1. 确认主贴主语是“人类开发者 / 制作者 / 我”而不是 agent 第一人称。
2. 确认标题是给人看的，不是内部术语摘要。
3. 确认封面和正文表达同一件事，不出现“封面讲协作、正文却讲功能总览”的漂移。
4. 确认没有以下表述：
   - `已上线`
   - `现在就能玩`
   - `公测`
   - `正式发布`
5. 确认本帖目标只有一个：
   - 自我介绍
   - 解释动机
   - 介绍队友
   - 抛一个讨论问题
6. 确认评论区准备好一句互动引导，而不是发布后临时想。
7. 记录发帖时间、标题与素材路径，便于后续复盘和实际 PR 或已有 Task 的证据回写。

## 4. 发帖后 24 小时巡检
前 24 小时是判断“这条内容有没有把人带进来”的主要窗口。建议最少按以下节奏检查：

- `T+15m`：确认图片、排版、标题和正文显示正常。
- `T+1h`：第一次看评论，处理明显误解。
- `T+4h`：看是否出现高质量讨论或重复问题。
- `T+24h`：做一次小结，判断这条内容的互动方向和下一篇承接点。

每次检查至少记录：
- 有没有人看懂这条在讲什么。
- 评论是在猜项目、问协作方式，还是被标题带偏了。
- 有没有用户自发补全你没说出来的项目特征。

## 5. 评论分级
统一分成 4 桶：

### P1: 状态误解
- 典型问题：
  - “现在能玩吗？”
  - “已经上线了吗？”
  - “在哪里下载？”
- 动作：
  - 直接纠偏。
  - 第一优先是校正项目阶段，不是急着拉下载。
- 安全方向：
  - 重申当前不是正式上线口径。
  - 不把互动热度误用成 release 信号。

### P1: 过度猜想 / 过度承诺诱导
- 典型问题：
  - “所以这就是全自动做游戏了？”
  - “以后是不是人都不用做了？”
  - “这是不是已经是完整 AI 游戏公司流程？”
- 动作：
  - 要降温，不要顺势夸大。
  - 维持“这是协作方式变化，不是全自动替代”的边界。

### P2: 高质量互动
- 典型问题：
  - “只看这个分工，我猜是一个长期世界/模拟类游戏。”
  - “你们这套分工为什么需要 liveops 提前介入？”
  - “为什么 agent 队友里 QA 和运营这么重要？”
- 动作：
  - 优先回复。
  - 可以追问，拉长讨论。
  - 这类评论是后续选题输入。

### P3: 轻互动
- 典型问题：
  - “有意思”
  - “第一次见这种做法”
- 动作：
  - 可简短互动，也可不消耗太多精力。

## 6. 回复边界
公开回复时默认遵守以下顺序：

1. 先确认对方在问什么。
2. 再校正状态边界。
3. 然后只给最小必要解释。
4. 最后用一个问题把讨论留在评论区里。

推荐句型：
- `现在还不是正式上线阶段。`
- `这条更想让大家先看“团队分工”，不是先讲完整功能。`
- `你这个猜法挺接近，我们后面会慢慢展开。`
- `这个问题很关键，后面可以单独写一条。`

不要做的事：
- 不在评论区承诺发布日期。
- 不把试验性的想法说成确定 roadmap。
- 不把用户一句夸张解读顺势接成宣传口径。
- 不在评论区展开成长篇 FAQ 文档。

## 7. 互动引导原则
- 每篇只留一个核心互动问题。
- 问题优先让用户“判断 / 猜测 / 站队”，而不是要求用户先理解全部背景。
- 如果轮播图已经承担了信息密度，正文就不要再重复讲一遍。
- 比较有效的问题类型：
  - `如果只看这套分工，你会猜这是个什么游戏？`
  - `你会更在意这种协作方式，还是最后做出来的游戏本身？`
  - `你觉得这支队伍里最关键的是哪个角色？`

## 8. 信号回流规则
- 如果评论在猜项目类型：
  - 记录用户最自然的第一印象，回流 `producer_system_designer`
- 如果评论在问协作方式：
  - 记录用户最想知道的协作细节，作为后续选题输入
- 如果评论在质疑 agent 价值：
  - 记录真实阻力点，不要只看正向反馈
- 如果评论暴露对项目阶段的误解：
  - 回流到 `liveops_community`，调整后续帖子的边界表达

## 9. 当日证据回写要求
当天做过小红书动作后，必须回写对应 PR、实际 CI 与评审记录（Issue 按需）。

最少记录：
- 时间
- 角色：`liveops_community`
- 完成内容：
  - 发了什么
  - 标题是什么
  - 评论主要在往哪个方向走
- 遗留事项：
  - 哪类评论值得在下一篇接着讲
  - 有没有项目阶段被误解

推荐附带字段：
- `post_title`
- `signal_tags`
- `next_post_hint`
- `owner`

## 10. 每周复盘
每周至少做一次轻复盘，回答 4 个问题：

1. 哪类标题最能把人拉进评论区？
2. 哪类表达最容易让用户把它误解成“已经上线 / 已经可玩”？
3. 用户最想了解的是：
   - 游戏本身
   - 开发动机
   - agent 协作
   - 团队分工
4. 下一周应该继续推什么：
   - `为什么做这个游戏`
   - `这支队伍怎么协作`
   - `为什么会有这样的分工`
   - `这到底会做成什么游戏`
