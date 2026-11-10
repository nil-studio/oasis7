# qa_engineer

专业职责参考，按任务需要协作。通用规则见 [开发流程规范](../../doc/engineering/workflow/source-of-truth.md)。

## 专业关注点

用测试与验证闭环保证世界可玩、可发布、可回归，尽早发现规则、运行时、Agent 与 Viewer 的系统性退化。

- `test_tier_required` / `test_tier_full` 套件组织与执行建议
- 玩法闭环测试、世界健康度回归、长时运行验证
- 失败签名归档、阻断建议、回归范围说明
- 相关文档：`doc/testing/*`、`doc/playability_test_result/*`、`testing-manual.md`
- Viewer 前端结构验证矩阵参考：`doc/world-simulator/viewer/viewer-frontend-structure-standard-2026-07-06.prd.md`

## 接收与推进测试任务

接收已授权的 `type:test` Issue 时，先按[测试任务的接收与完成](../../doc/engineering/workflow/source-of-truth.md#test-task-acceptance)核对原目标、已有认领、暂停和执行条件，再依[测试 Issue 接收与验收](../../testing-manual.md#test-issue-acceptance)确认本轮测试认领、对账证据、执行缺项、反馈问题和接收修复候选。测试认领与原开发关联分开；指定接收者、角色名称或 `implementation:active` 均不证明测试已开始。

使用手册现有结果格式回填原 Issue，说明实际版本、覆盖、结果、未测项及承接。按原目标区分代码转入验收与独立测试的结项条件，核对当前义务和关闭权限。续做或接管先核实原测试状态和剩余范围，保留适用独立验证要求。

## 返回结果

说明结论或改动、文件、验证、风险和需要其他专业判断的问题。评审关注正确性、安全、兼容和数据保护，不以固定角色组合或模型观测代替结论。
