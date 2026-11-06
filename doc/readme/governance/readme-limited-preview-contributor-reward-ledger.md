# Limited Preview Contributor Reward Ledger

审计轮次: 1

## Meta
- Round ID:
- Candidate ID:
- Window:
- Claim Envelope: `limited playable technical preview`
- Owner Role: `liveops_community`
- Review Role: `producer_system_designer`
- Execution Role:
- Ledger Status: `draft / under_review / approved / partially_distributed / archived`

## 1. Intake Rules
- Only record contribution-based rows.
- Use the contribution base scores, quality modifiers, and recommendation bands in [reward-pack PRD §7](readme-limited-preview-contributor-reward-pack.prd.md#7-执行模板执行版); the operation pack is a worksheet, not a separate scoring authority.
- Use `Contributor` and `Public Handle / GitHub` for review identity; keep `Reward Account` as the execution field.
- If `Source Type=PR`, first read the optional reward intake block from `.github/pull_request_template.md`.
- Do not include signup, login, casual play, online time, AFK, or vague praise as rewardable rows.
- Missing key evidence means default `deferred` or `no-token-recommendation`.

## 2. Ledger
| Ledger ID | Contributor | Public Handle / GitHub | Reward Account | Source Type | Source Link | Contribution Type | Base Score | Quality Modifier | Total Score | Recommended Band | Duplicate Check | Reviewer | Review Status | Producer Decision | Approval ID | Actual Amount | Distribution Ref | Distribution Date | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| LTRL-001 |  |  |  | `issue / PR / DM / content / session summary` |  | `C-01..C-07` |  |  |  | `no-token-recommendation / eligible-small / eligible-medium / eligible-large` | `unique / duplicate / superseded` |  | `draft / reviewed / approved / rejected / deferred / distributed` |  |  |  |  |  |  |

<a id="recognition-ledger-records"></a>
### 2.1 主记录与不可变修订附表

旧20列表仍可接收importer的draft输出；以下附表承接[专业主合同](contributor-recognition-intake-and-review.design.md#recognition-contribution-identity)，不能把脚本stdout当已持久入账。按一个canonical revision同时接收实际事件、主项/别名索引、当前投影与决定引用；操作者、权限依据及原记录定位真实可核。未采用新合同的历史row不改写，只追加准确legacy关联。

| Contribution ID / revision / previous ref | 原 Ledger ID / round / scope / purpose / window | 原 intake/event refs 与来源别名 | Contributor ref / 归属证据 / 共同署名 | 当前证据可用性 / qualification / blocker | Canonical merge/correction ref / 旧批准及执行引用 |
| --- | --- | --- | --- | --- | --- |
|  |  |  |  |  |  |

| Event / decision ID / revision | Previous ref / expected canonical revision | 实际操作者及原权限依据 | 准确输入版本 / 原请求 | 结果 / 原因 / 有效范围 | 持久回读 ref / 当前反馈定位 |
| --- | --- | --- | --- | --- | --- |
|  |  |  |  |  |  |

| Allocation ID / 主贡献占用键 | 当前 approval revision / actual-value review | 固定收款明细 / 原资产网络 / 最终金额 | 原 proposal/transaction/request / payload摘要 / attempt | 当前占用及原执行结果 | Distribution ref / 日期 / unresolved原因 |
| --- | --- | --- | --- | --- | --- |
|  |  |  |  |  |  |

同一成果跨渠道、跨handle、跨round/purpose只有一个主项；Reward Account是执行字段，first-submit或同账户都不证明归属。先分后并只追加alias/correction，不删除任何原intake、批准或发放。已消费贡献不得进入第二份资格；身份/归属未决只hold相关项，明确其他已核成员可继续。

单一Review Status不足以表达全部事实：已批准但当前失证，应并列保留历史approved与当前暂停；已发放后失证，应保留distributed及原引用，再记当前不可核/复核。不可访问不代表曾经不存在；已删除内容不复制留存，必要最小引用与可用性/理由保留。窗口结束或换渠道只能按原规则补证、等待或关闭，不重开旧资格。

## 3. Evidence Checklist
Each non-rejected row should include:
- `proof_link`
- `issue_or_pr_link` if available
- `build_id / env`
- `repro_steps` or `session summary`
- `why_this_matters`
- `duplicate_check`
- `reviewer_note`

If a contributor only provides raw account derivation material, normalize it to `Reward Account` before filling the ledger.
If a PR source has no reward intake block, treat it as "no reward review requested". Only create or advance a reward row after `Reward Account` is backfilled from an approved follow-up channel.

## 3.1 Scripted Import
Use the repo script before copying PR-sourced rows into the ledger:

```bash
./scripts/readme-reward-pr-intake-import.py --pr 123
./scripts/readme-reward-pr-intake-import.py \
  --body-file /tmp/pr-body.md \
  --source-link https://github.com/<owner>/<repo>/pull/123 \
  --public-handle builder01 \
  --contributor @builder01 \
  --ledger-id LTRL-PR-001 \
  --format ledger-md
```

Recommended handling:
- `ready`: import the emitted row as `draft`.
- `deferred`: keep the row deferred until claimant fields are completed.
- `no_reward_review_requested`: do not create a ledger row.
- `invalid_intake`: ask the PR author to fix or delete the intake block before import.

## 3.2 Merged PR Round Scan
When one reward review round wants to review many merged PRs together, batch them through the round scan wrapper first:

```bash
./scripts/readme-reward-pr-intake-round-scan.py \
  --use-gh \
  --repo <owner>/<repo> \
  --merged-after 2026-04-01 \
  --merged-before 2026-04-12 \
  --format json

./scripts/readme-reward-pr-intake-round-scan.py \
  --use-gh \
  --repo <owner>/<repo> \
  --merged-after 2026-04-01 \
  --merged-before 2026-04-12 \
  --format ledger-draft-md

./scripts/readme-reward-pr-intake-round-scan.py \
  --use-gh \
  --repo <owner>/<repo> \
  --merged-after 2026-04-01 \
  --merged-before 2026-04-12 \
  --format ledger-csv
```

Round scan handling:
- keep one explicit merged window per round.
- only import `ready` and `deferred` results into the ledger table.
- keep `no_reward_review_requested` and `invalid_intake` in the scan report, but do not auto-create ledger rows from them.
- round scan is only a batching wrapper around the single PR import contract; do not let it drift into a second rule set.
- `ledger-draft-md` is for a human-readable first ledger draft with round meta placeholders still blank.
- `ledger-csv` is for spreadsheet-friendly row import; it only exports `ready` and `deferred` rows.

Planned grant amounts remain provisional until producer finishes the final actual-value review before distribution.

## 4. Band Summary

本表只按成员行自身band计数；另用[评分汇总附表](readme-limited-preview-contributor-reward-pack.md#recognition-score-worksheet)记录同round/purpose/contributor的固定成员集合及S。成员60/30时本表计medium1、small1，集合90/large另记，不能再加一条large付款row。候选、合格、排除、未决及已消费成员必须分别对账；同贡献跨包/轮次不重计资格。
| Band | Row Count | Contributor Count | Status |
| --- | --- | --- | --- |
| `eligible-large` |  |  |  |
| `eligible-medium` |  |  |  |
| `eligible-small` |  |  |  |
| `no-token-recommendation` |  |  |  |

## 4.1 Producer Amount Guardrails
- Recommendation bands are review labels, not a public amount table.
- For ordinary merged PR rows (`Source Type=PR` and `Contribution Type=C-03`), the default producer approval ceiling is `150 OC`.
- Any merged PR row above `150 OC` must include an exceptional-case note in both the row notes and approval summary before it can stay approved.
- `1500 OC` is reserved for rare exceptional rows and must be recorded as a round-specific decision, not as the default meaning of `eligible-large`.

## 5. Approval Summary
- Producer Review Date:
- Approved Rows:
- Rejected Rows:
- Deferred Rows:
- Approval Notes:
  - If a planned grant is downshifted before distribution, record the actual-value review reason here before execution.

批准必须引用附表中的确切主项/建议版本、真实producer决定与当前有效分配。下调、撤销、证据失效或先分后并都追加后继revision，保留原分/档/金额/理由；不要静默覆盖原批准。普通PR拆分收款或多次修订后的总额仍共享同一150 OC默认cap；汇总large不能放宽它，超过依原exceptional note和round-specific决定。

## 6. Distribution Closure
| Approval ID | Ledger ID | Contributor | Actual Amount | Distribution Ref | Distribution Date | Execution Owner | Closure Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
|  |  |  |  |  |  |  | `pending / distributed / failed / retried` |

执行前按[原件与唯一占用合同](contributor-recognition-intake-and-review.design.md#recognition-distribution-consumer)核当前批准、证据和actual-value review，再持久保存主项占用及原签发/请求身份；ledger与外部multisig不是一个原子事务。签定/投递可能在途时不能仅靠本地撤销、hold或换账户宣布取消。ACK丢失或已发放未回填先查同一原件，恢复原closure；没有明确未生效且原请求不能再生效的证据，不得换nonce或allocation再发。原公开链确认与当前可读权限按其实际通路核验，余额变化不代原分配回执。

“retried”只记录原协议允许的恢复动作，不代表一份新奖励或额外实际金额。已真实distributed后证据失效保持原支付事实，另记当前复核；本台账不自动追回、追债、补发或授World权益。完整正反方法见[RI05](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-05-distribution)。

## 7. Safe Copy Check
- Public wording reviewed: `yes / no`
- Forbidden phrase hits: `0`
- Notes:

Forbidden phrasing still includes:
- `play-to-earn`
- `login reward`
- `time played = token`
- `come play to earn`
- `airdrop for players`
- `just try the game and get token`

## 8. Next Actions
- Unresolved items:
- Missing accounts:
- Rows waiting producer review:
- Rows waiting distribution:
- Archive note:

### 8.1 当前玩家反馈

每个已采用来源的受理、审核和查询须返回：原计划/scope/window、当前主贡献与来源关联、证据摘要或无可用证据说明、当前处理/历史决定/原执行状态、主要blocker、是否需补充及真实下一步。PR/issue沿原通路，DM只回原获权受众，content/session summary沿真实投稿/结果入口；无实际入口明确未提交，不能伪造后台受理。反馈发布失败保留同一结果并记录未送达；读原件/重连不重排队、不重置窗口或重复发放。按[反馈合同](contributor-recognition-intake-and-review.design.md#recognition-feedback-recovery)与[RI06](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-06-feedback)核对，公开ledger或链可见不授予私有证据读取权。
