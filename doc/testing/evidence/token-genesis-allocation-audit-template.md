# oasis7 主链 Token 创世分配审计模板

## Meta
- 审计日期:
- 审计角色: `qa_engineer`
- 对应专题: `doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md`
- 参数表来源:
- 评审版本 / commit:
- 当前结论: `pass` / `block` / `conditional_draft_only`

> 此文件是可重复填写的模板，不是已执行审计，也不构成 mint、分配或发布的放行证据。真实控制主体/多签、个人受益拆分或执行细节缺失时，结论只能是 `conditional_draft_only` 或 `block`。

## 创世参数表
| bucket_id | ratio_bps | recipient | start_epoch | cliff_epochs | linear_unlock_epochs | genesis_liquid | controller | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `team_long_term_vesting` |  |  |  |  |  |  |  |  |
| `early_contributor_reward_reserve` |  |  |  |  |  |  |  |  |
| `node_service_genesis_custody` |  |  |  |  |  |  |  |  |
| `staking_genesis_custody` |  |  |  |  |  |  |  |  |
| `ecosystem_governance_reserve` |  |  |  |  |  |  |  |  |
| `security_reserve_emergency` |  |  |  |  |  |  |  |  |
| `foundation_ops_reserve` |  |  |  |  |  |  |  |  |

## 汇总值
- `sum(allocation_bps)=`
- `project_control_bps=`
- `protocol_long_term_reserve_bps=`
- `founder_direct_max_bps=`
- `genesis_liquid_bps=`
- `year1_external_release_bps=`

## 审计项
| Audit ID | 检查项 | 实际值 | 目标值 | 结果 | 备注 / 修正建议 |
| --- | --- | --- | --- | --- | --- |
| TGA-01 | 分配比例总和 |  | `10000 bps` |  |  |
| TGA-02 | bucket 完整性 |  | `7/7` |  |  |
| TGA-03 | 项目战略控制 |  | `5000 bps` |  |  |
| TGA-04 | 协议长期储备 |  | `3500 bps` |  |  |
| TGA-05 | 个人直持上限 |  | `<=1500 bps` |  |  |
| TGA-06 | 创世液态流通 |  | `0 bps` |  |  |
| TGA-07 | 首年外部释放 |  | `<=500 bps` |  |  |
| TGA-08 | custody / treasury 语义 |  | `创世只进 vested_balance` |  |  |
| TGA-09 | 奖励叙事禁语 |  | `0 命中` |  |  |

## 阻断摘要
- 是否阻断:
- 阻断项:
- 最小修正动作:

## 证据
- 文档证据:
- runtime 语义证据:
- 相关 PRD / project:
