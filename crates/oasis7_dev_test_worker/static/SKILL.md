---
name: oasis7-account-service
description: Use the standalone account service to select authorized accounts and manage bounded sessions. No game world or local backend is required.
---

# 通用账号服务 Skill

当前环境：**{{ENVIRONMENT}}**。测试与正式环境的账号、凭据、会话和额度相互独立。

## 连接

使用本页面所在站点的 HTTPS Origin，加上 `/accounts/mcp` 作为 MCP 地址。
管理员通过私密渠道提供本环境的消费者 Bearer token；在连接器的认证配置中保存。
不要把 token、账号原始凭据或完整认证头写入聊天、页面、日志或仓库。
本页面不提供公共体验 token。未获得授权时，请向服务管理员申请。

每次请求使用 `POST`、`Content-Type: application/json` 与 `Authorization: Bearer <token>`。
支持 MCP `initialize`、`ping`、`tools/list` 和 `tools/call`；调用工具 `account_service_call`。

## 操作顺序

1. `accounts` 列出当前消费者可用的账号及 `grant_ref`。只使用返回的引用，不猜测其他消费者的账号。
2. `open` 选择一个 `grant_ref`，指定小于授权截止时间的绝对 Unix 秒 `expires_at_unix`，保存返回的 `session_ref`。
3. `describe` 用 `session_ref` 获取当前账号公开元数据。切换账号时另行 open，保留各自的会话引用。
4. 不再使用时 `close`。关闭后不可重新使用该会话；额度耗尽时仍可关闭。

请求示例（Body，认证头通过连接器配置）：

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"account_service_call","arguments":{"method":"accounts"}}}
```

后续调用只替换 `arguments`：

```json
{"method":"open","grant_ref":"<accounts 返回的引用>","expires_at_unix":<未来的 Unix 秒>}
{"method":"describe","session_ref":"<open 返回的引用>"}
{"method":"close","session_ref":"<open 返回的引用>"}
```

以上尖括号内容是占位符，实际发送前替换为返回的字符串和整数。

## 结果与失败

读取 `result.structuredContent`，仅 `status: "ok"` 代表本次业务成功。
成功结果含 `value.policy_generation`、`value.result` 和累计 `value.usage`。
`accounts/open/describe` 成功各扣一次额度；拒绝请求不扣额度，close 不扣额度。
重连、账号切换、Worker 更新不会重置累计用量。
HTTP 401 表示消费者认证无效；403 表示 Origin 不符；503 表示配置或持久状态不可用。
业务 `status: "unavailable"` 时读取 `code`，不要当作成功，也不要无限重试。
过期、关闭、撤权或凭据轮换后应停止使用旧会话，重新取得明确授权。

## 边界

服务只管理授权目录、会话和公开账号元数据。它不返回账号原始凭据，
不执行第三方登录，不执行游戏操作，也不代理任意 URL。
管理账号、轮换、停用与额度由受信任管理员通过 Worker Secrets 和版本化策略更新；
消费者工具没有管理员写接口。不要把正式凭据复制到测试环境。
