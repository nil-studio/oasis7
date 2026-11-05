# 托管 Control 调用

此模式属于已批准的 `native-broker-custody-v1`、`custody-native-v1` 控制链。操作者提供固定资源 handle 与 socket；模型不得选择服务 origin、文件路径、账户、密钥或凭据。当前仅支持 V1 TestSession，V2 计划明确拒绝。

## 模型调用

通过私有 stdin 将闭合 JSON 交给正式客户端：

```sh
<client> custody-control-call --socket <approved-socket>
```

输入为 `api_version:1`、操作者给定的 `resource_id`、稳定 `request_id` 和 `method`。method 仅可为 `initialize`、`tools_list`、`heartbeat`、`request_finish`、`request_abort`。不得加入任意 HTTP、签名原文、token、Owner/Executor 请求或其他工具字段。每个逻辑请求只生成一个 request_id；unknown 后保留该 ID 和完整原输入，不能换 ID 规避未知状态。

Host 返回 accepted 只证明控制请求受理；historical 是已记录响应；unknown 需要原请求恢复或操作者处理。`close_requested` 只表示收紧到 Closing，不能报告 World Closed 或 TestSession Closed。不得用自然超时、Finish/Abort ACK 或一个撤销字段替代世界权威结果。当前业务动作和 Read 不可用，应明确报告能力缺口，不能切换普通持钥客户端继续同一托管会话。

## 可信操作者

Host 与模型必须使用不同受限 OS 主体或已批准的远程隔离。Host 私有 vault、wrapping key、客户端签名 seed 与 capability 不提供给模型；Native SID/CSRF 和 Owner/Executor seed 不迁入 Host。mode0600、descriptor 或 Passkey 本身不证明隔离。固定 Runtime/Skill 来源与路由登记由操作者安装，Host 使用内核 peer UID 校验 socket 对端。

正式命令为 `custody-host-init`、`custody-host-enroll`、`custody-host-bind`、`custody-host-claim`，以及 `custody-host-profile` / `custody-host-serve`。前四项使用固定 `--vault`，后两项另有固定 `--socket`；闭合私有 JSON 和 wrapping key 仅通过可信 stdin 传递，不放在 argv/env/stdout。输入结构以同版本 CLI 和实际批准合同为准，不让模型填这些操作者原件。

Enrollment 只是元数据配对；bind 不授权限，必须完成精确 fresh-MFA 批准与正式 credential claim。claim 历史响应没有 token 恢复通道；丢失首次私有 credential 回复时须明确由操作者执行获准替换，不伪造新凭据或从历史读取 token。

Host 在出站前持久保留原 typed request 与发送尝试，固定 HTTPS origin、禁止 redirect 和自动重试。冷恢复保留原 nonce、body 和 scope；未知效果不退款、不重新签替代原件。隔离组件、真实 HTTPS 控制链及部署验收分别记录；该参考不证明实际生产配置、工业任务或 CT-008 已验收。
