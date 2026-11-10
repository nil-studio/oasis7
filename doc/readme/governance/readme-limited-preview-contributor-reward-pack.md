# Limited Preview Early Contributor Reward Pack

审计轮次: 1

## Meta
- Owner Role: `liveops_community`
- Review Role: `producer_system_designer`
- Round Scope: `limited playable technical preview`
- Dependency: `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
- Rule: reward review is contribution-based, not invite-only-gated, and not `play-to-earn`

## 1. What Counts
Countable contribution examples:
- reproducible bug report with clear steps
- merged PR or high-quality patch
- long-session structured play sample
- content / docs / translation contribution
- builder support or ecosystem help with concrete evidence

Default non-countable examples:
- signup
- login
- casual play
- online time
- AFK / idle time
- vague praise with no evidence

## 2. Scoring Sheet

Use [reward-pack PRD §7](readme-limited-preview-contributor-reward-pack.prd.md#7-执行模板执行版) as the single source for contribution base scores (§7.1), quality modifiers (§7.2), and recommendation bands (§7.3). This sheet records those inputs; it does not define another scoring rule or approve a distribution.

| Field | Value |
| --- | --- |
| Contributor |  |
| Public Handle / GitHub |  |
| Reward Account |  |
| Contribution Type |  |
| Base Score |  |
| Quality Modifier |  |
| Total Score |  |
| Recommended Band | `no-token-recommendation / eligible-small / eligible-medium / eligible-large` |
| Reviewer |  |
| Review Date |  |

`Reward Account` 仅用于实际发放与台账执行；reward review 的名称层继续依赖 `Contributor` / `Public Handle / GitHub`，不用 raw `public key` 作为展示名称。

<a id="recognition-score-worksheet"></a>
### 2.1 稳定成员与汇总工作表

按[首版评分政策](readme-limited-preview-contributor-reward-pack.prd.md#recognition-score-v1)使用以下附表；政策须由原 producer 明确采纳其版本与适用 round，不能据模板改算历史记录。每个合格独立贡献只取一个 base 和一个 quality；C-07 恒0，缺证/虚假/重复项不因诊断分为正成为 eligible。归属和跨渠道关系按[主记录合同](contributor-recognition-intake-and-review.design.md#recognition-contribution-identity)核实，首次投稿不抢作品归属。

| 成员输入/结果 | 本次记录 |
| --- | --- |
| Contribution ID / current revision / 原 Ledger ID |  |
| Contributor ref / 可核归属与别名 / scope、purpose、window |  |
| Evidence availability / qualification / blocker |  |
| Scoring policy version / 单一 base / 单一 q / 本行 s / 本行 band |  |
| Canonical duplicate/merge/correction refs / 已批准或执行引用 |  |

| Bundle ID / revision | 原 round / purpose / contributor | 完整候选及版本 | 当前合格成员及各自 s/band | 排除/未决/已消费项与理由 | S / 汇总 band | Producer review ref |
| --- | --- | --- | --- | --- | --- | --- |
|  |  |  |  |  |  |  |

汇总只是建议，不能写入任何成员的 Recommended Band 或成为额外付款行。60分bug加30分独立文档可得到集合90/large，两行仍为medium与small；+20和+10不可累加，同一成果在多个渠道出现只算一次。同贡献跨round/purpose不能重奖，已批准/发放成员在后继集合列为已消费；补证只用原窗口和后继版本。未决项明确保留，不阻塞可分离的已核成员。

原 actual-value review 下调建议或金额时，保留本表原计算值并追加 producer 决定与理由；不通过编辑旧分值伪造新历史。独立正反例与预期见[RI03](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-03-scoring)。

## 3. Evidence Fields
Each recommendation should include:
- `proof_link`
- `issue_or_pr_link` if available
- `build_id / env`
- `repro_steps` or `session summary`
- `why_this_matters`
- `duplicate_check`
- `reviewer_note`

Missing any key evidence field means default `no-token-recommendation` until completed.
If a contributor only provides raw account derivation material, normalize it to `Reward Account` before entering this sheet.

## 3.1 GitHub PR Intake
If the contribution source is a GitHub PR, the author can keep the reward intake block in `.github/pull_request_template.md`. Authors who are not requesting reward review should delete that entire section before opening the PR.

Recommended PR fields:
- `Request reward review`
- `Reward Account`
- `Evidence / context link`
- `Notes`

If the PR does not include this block, treat it as "no reward review requested" unless `liveops_community` later backfills the fields from an approved follow-up channel.

## 3.2 Scripted Import
Use the repo script when `liveops_community` wants to import PR intake without manually copying fields:

```bash
./scripts/readme-reward-pr-intake-import.py --pr 123
./scripts/readme-reward-pr-intake-import.py \
  --body-file /tmp/pr-body.md \
  --source-link https://github.com/<owner>/<repo>/pull/123 \
  --public-handle builder01 \
  --contributor @builder01
```

Status meanings:
- `ready`: reward review explicitly requested and `Reward Account` is present.
- `deferred`: reward review requested, but the required payout field is incomplete.
- `no_reward_review_requested`: PR body does not contain the intake block.
- `invalid_intake`: intake block exists, but `Request reward review` is not explicit `yes`.

## 3.3 Merged PR Round Scan
When one reward review window wants to triage many merged PRs at once, use the round scan wrapper instead of opening each PR manually:

```bash
./scripts/readme-reward-pr-intake-round-scan.py \
  --use-gh \
  --repo <owner>/<repo> \
  --merged-after 2026-04-01 \
  --merged-before 2026-04-12 \
  --format json
```

Recommended handling:
- scan one explicit merged window at a time; do not treat an unbounded merged PR search as a review round.
- only `ready` and `deferred` rows should move into the current ledger draft.
- keep `no_reward_review_requested` and `invalid_intake` in the scan report for auditability, but do not auto-create ledger rows from them.
- round scan must reuse the same status contract as single PR import; it is a batching wrapper, not a second scoring path.

## 4. Band Rules
- `<20`: `no-token-recommendation`
- `20-49`: `eligible-small`
- `50-89`: `eligible-medium`
- `>=90`: `eligible-large`

These are recommendation bands only.

## 4.1 Producer Amount Guardrails
- Recommendation bands are not a public bounty table.
- For ordinary merged PR rows (`Source Type=PR` and `Contribution Type=C-03`), the default producer approval ceiling is `150 OC`.
- Any merged PR row approved above `150 OC` must carry an exceptional-case note explaining why the ordinary ceiling is not enough.
- `1500 OC` is reserved for rare exceptional rows and must be recorded as a round-specific decision, not a global contributor reward band mapping.

Do not say:
- fixed token amount
- fixed token/point ratio
- “play X hours, get Y token”

## 5. Safe Copy
Allowed phrasing:
- `early contributor reward`
- `contribution-based review`
- `small auditable reward consideration`
- `bug report / PR / structured feedback may be eligible`
- `subject to review and approval`

Forbidden phrasing:
- `play-to-earn`
- `login reward`
- `time played = token`
- `come play to earn`
- `airdrop for players`
- `just try the game and get token`
- `invite-only reward drop`

## 6. Review Chain
1. `liveops_community` records contribution + evidence + score
2. `producer_system_designer` reviews recommendation band
3. if approved, later treasury / execution owner handles actual distribution

No public promise should be made before producer approval.

接收、审核、批准与执行的玩家表达按[同一读模型](contributor-recognition-intake-and-review.design.md#recognition-feedback-recovery)返回范围、证据可用性、当前状态、blocker及真实下一步；感谢或ready不能代替结果。已批准/发放后失证保留原决定和实际执行，追加当前核查/暂停理由；不得自动追回、追债或重奖。操作时使用[ledger修订模板](readme-limited-preview-contributor-reward-ledger.md#recognition-ledger-records)，不覆盖历史轮次。
