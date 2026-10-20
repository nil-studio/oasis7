# 安全移动替代操作与验证手册

- 适用设计：[安全 canonical 替代](../../world-runtime/runtime/safe-canonical-alternative-confirmation.design.md)。
- 当前生产范围：不受支持的途经点或定时移动请求，比较直接 `MoveAgent` 端点；不承诺实现原路径或到达时间。模块影响不能确定时返回 `evaluation_unavailable`，停止执行。

## Viewer

在 Agent Chat 内展开 Safe movement alternatives，填写原目标、可选原到达 tick 及厘米坐标 JSON，例如 `[{"x_cm":0,"y_cm":0,"z_cm":0}]`。比较后逐项检查目标、范围、成本上限、风险、后果、撤回边界与理由。比较与确认不移动 Agent。确认精确候选后，再单独执行；实际结果显示 canonical receipt。

连接中断或状态 unknown 时，点击 Recover original command。浏览器在首次发送前保存原签名命令到当前标签页 sessionStorage；刷新页面后恢复同一条命令，不重新确认。如果尚未 admission，可用 Retry exact original 重发同一签名原件。当前 owner/world/Agent 不匹配时不显示旧响应。新标签页与清除浏览器存储后不能恢复本地原件；应保留原 request identity 并通过有权读面查询，不能假设没有执行。

## 第一方 native SDK

`WorldServiceTaskClient::prepare_safe_alternative(owner_proof)` 接收独立 owner 签名的 `SignedReadRequest<SafeAlternativeCommandV1>`，签名 domain 来自 `command.signing_domain()`，可用 `oasis7_client_api::world_service::sign_read_request` 在 owner 端构造。该 API 不借用 native executor 的配置私钥签确认，也不加入模型工具。

1. `Propose` 保存原请求与有效期，得到候选集。
2. 用户比较后独立签 `Confirm`，固定 generation、candidate ID/digest、confirmation ID 与期限。
3. 独立签 `Execute`，固定 authorization ID 与 execution ID。
4. 每个步骤先持久化 `prepare_safe_alternative` 返回的完整 envelope，再调用 `submit_safe_alternative`。响应丢失时将同一 envelope 交给 `recover_safe_alternative`；不得重新分配身份或签新确认代替重试。
5. `Read` 重读当前 lineage，`Cancel` 显式拒绝或取消。当前所有权、generation 或权限变化仍由 runtime 重验；SDK 签名验证不是领域授权。

## 验证

验证零效果预览、独立确认、单次执行、原件恢复、不同候选并发冲突、权限替换和过期拒绝。UI 需要真实浏览器桌面与窄屏检查，不能用仅 DOM 测试替代。模型建议或普通聊天签名不构成替代确认。
