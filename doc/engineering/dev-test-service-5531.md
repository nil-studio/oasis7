# 开发测试服务共同入口

Refs #5076、#5531。Trace refs: O7-REQ-56c3a553-8216-4e44-86dc-bd368f3b53cb。

`dev-test-serve --profile <private-config.json>` 启动串行 Unix socket 服务；`dev-test-call --socket <approved-socket>` 从可信宿主 stdin 接受 JSON。模型隔离及实际外部 Agent 接入由 #5532/#5533 承接，本入口不证明该隔离。

配置须由当前用户拥有且权限 0600，socket 父目录 0700。配置字段：profile=`dev-test-accounts-v1`、socket、max_request_bytes（256～65536）、timeout_ms（1～30000）、max_calls（正整数）、credentials（caller_id、token〔至少32字符〕、expires_at_unix、revoked）。认证材料只由可信宿主管理，不进入模型输入。无管理接口；管理认证后继复用既有身份能力。

请求仅接受 profile、audience=`oasis7-dev-test-tools`、credential、method=`capabilities`。错误版本、用途、过期、撤销与未知方法拒绝，未知字段拒绝。返回 accounts_available=false、writes_available=false。并发固定1；超时断开请求。只有成功能力调用消耗累计上限，拒绝流量不扣。用量写入 socket 同名 `.usage` 文件，先持久化再回复；重启保留该文件。重启前由可信运营者确认旧进程终止并删除遗留 socket，保留用量文件。配置与socket路径固定；跨配置稳定授权账本由 #5535 承接。

当前验证：macOS canonical cargo check、能力认证单测和真实服务/CLI调用；Linux Docker daemon 启动失败，Linux实际启动尚未完成。实际外部 Agent 未验证。
