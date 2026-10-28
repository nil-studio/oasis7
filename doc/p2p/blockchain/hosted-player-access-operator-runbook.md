# Hosted Player Access Operator Runbook

## 文档身份

- Owner role：`liveops_community`
- Review roles：`blockchain_ops_engineer`、`runtime_engineer`、`qa_engineer`、`producer_system_designer`
- 适用范围：`hosted_public_join` 的玩家入口分享、访问面隔离、session 处置、事故收口与 claim freeze
- 产品边界：[`doc/product/player-entry-distribution/prd.md`](../../product/player-entry-distribution/prd.md)
- 专业合同：[`doc/p2p/prd.md`](../prd.md)
- 身份与托管后继专题：[`hosted-public-join-managed-identity-custody.prd.md`](hosted-public-join-managed-identity-custody.prd.md)
- 环境边界：[`doc/engineering/governance/environment-lanes-and-inventory-2026-05-29.md`](../../engineering/governance/environment-lanes-and-inventory-2026-05-29.md)

本文是长期稳定的 operator procedure。它不定义协议、实现、custody 方案、发布状态或公开 claim；当前公开状态始终以根 [`README.md`](../../../README.md) 为准。

## 1. 入口与信任面

Viewer 的 WS 目标由 launcher 的部署配置注入 HTML 中唯一的 `oasis7-viewer-runtime-config` JSON 数据节点。公开反代部署启动 launcher 前设置 `OASIS7_VIEWER_PUBLIC_WS_URL=wss://<公开域名>/<实际 runtime 路径>`，包括反代要求的固定 query；该值必须与浏览器真正访问的 WSS 目标一致。不得从请求 `Host`、分享 URL 的 `ws`/`addr` 或 `hosted_access` 展示提示推导信任目标。缺失或重复的数据节点、无效配置、不同端口/路径/query、userinfo 和 fragment 均阻止认证连接。

本地直接访问从实际 runtime bind 生成 WS 目标；`0.0.0.0`/`[::]` 明确映射为 loopback，只用于同机访问。远程访问必须配置浏览器可访问的公开 WSS URL；指定远程 hosted bind 而未配置公开目标时，launcher 拒绝启动。

分享 URL 可省略 `ws`/`addr`；保留参数时，它只能等于注入配置的规范化完整目标。静态 HTML 必须由 launcher 的配置注入链提供，不能直接部署一份缺配置的 release HTML 并通过 query 指定任意 runtime。

- `public player plane`：只承载公开 Viewer/join URL、guest/player session 与经服务端授权的低风险玩家动作。
- `private control plane`：承载 world lifecycle、operator control、配置、封禁和恢复；默认只允许 loopback、私网、VPN、堡垒机或受控 tunnel。
- `identity plane`：承载登录因子、账户恢复、device session 的签发、刷新、撤销与限流。
- `custody / signer plane`：只通过受控 sign API、step-up policy 与审计引用敏感授权；不得向浏览器、公开 API、URL、日志或证据暴露长期私钥和 signer material。
- node、validator 与 governance signer 不复用玩家托管身份或玩家 signer plane。

`hosted_public_join` 是 `viewer` 的 deployment/session context，不是环境、第三种玩家模式或发行等级。共享玩家链接不会授予 operator/control authority。

## 2. 分享前与分享中

分享前必须确认：

1. 准备分享的是 canonical public player join URL，而不是 control origin、console 或内部拓扑地址。
2. public proxy 只 allowlist 玩家面；未知路径默认拒绝，private control 仍不可由公网玩家访问。
3. 公开响应不包含 signer bootstrap、secret、operator state、内部日志或配置。
4. 当前 admission、session enforcement 与敏感动作边界有专业证据；无法确认时不得分享。
5. 玩家说明只引用根 README 当前允许的 claim，不写死 hostname、端口、路由清单、环境 verdict 或历史 smoke 结论。

分享时只发送玩家 join URL，并说明它是玩家入口。不要发送 control/operator URL、private endpoint、signer material、OTP、approval code、seed、日志、配置或内部拓扑。会话过期或被撤销时，只引导玩家从 canonical join 入口 reconnect、re-register 或 re-auth；不得要求玩家提供私钥、OTP 或包含 secret 的截图。

## 3. Session revoke、材料轮换与恢复

仅在已确认 compromise、abuse、异常复用或有边界的 operator 处置中撤销 session。处置记录至少包含 operator identity、目标 player/session、理由、evidence ref、影响范围与玩家恢复路径。

- revoke 后旧 authority 必须失效；恢复是一次新的有效绑定，不是静默恢复旧权限。
- rotate registration issuer、session/custody material 或相关 credential 时，必须按对应工程/运维程序执行，记录 blast radius、旧材料失效边界、重新认证预期与审计证据。
- registration issuer、session/replay ledger、SMTP、account store、strong-auth/custody material、风控与审计按环境隔离，不得跨 local/test/production 复用。
- 常规 rollback 不删除 session/replay ledger；损坏时先隔离留证，再按可信备份和专业恢复程序处理。
- 轮换可能使未消费 grant 或既有 session 失效；恢复外部流量前应确认受影响玩家能通过受控 re-auth/re-register 获得新 authority。

本文不保存 key、TTL、命令、供应商配置或具体 endpoint；这些实现与执行细节由当前工程/运维 authority 和 task evidence 拥有。

## 4. Freeze 与恢复分享

出现以下任一情况，立即暂停新增分享和 claim 升级：

- public player、private control、identity 或 signer plane 无法明确区分；
- control route 被公网访问，或公开响应疑似暴露 signer/secret；
- admission、session enforcement、revoke 或 strong-auth/sensitive-action 边界存在未解释穿透；
- operator 无法确认分享链接只落到玩家入口。

恢复分享前必须完成 containment、信任面复核、受影响 session/material 处置和证据留存，并取得适用的 engineering/ops 证据、QA 结论与 producer/LiveOps 决策。LiveOps 或 operator 不能单方面解除 freeze，也不得把账户连续性 smoke、单次 revoke 或历史 preview evidence 当作 production custody 或 hosted-entry readiness。

## 5. Incident 流程

1. 停止继续传播错误链接或执行高风险动作。
2. 保存最小必要证据，避免复制 secret、完整 token、私钥、OTP 或无关个人信息。
3. 分类受影响的环境、信任面、玩家/operator 影响与已知范围。
4. 边界不明时先 freeze claims，并将技术 containment 交给 runtime / blockchain ops owner。
5. QA 判断证据与 release impact，producer 决定 claim/resume，LiveOps 负责已批准的更正和玩家恢复说明。
6. 只有在安全玩家入口已确认后，才发布更正链接并关闭事故。

内部最小记录字段：`incident_id`、发现时间、报告人、环境、受影响信任面、已知范围、evidence ref、containment、claim 状态、玩家/operator 影响、owner、下一检查点、更正引用与未决风险。记录进入 task 或获批准的 incident sink，不把内部细节复制到公开文档。

## 6. 最小披露模板

- 初始通知：正在调查访问问题，请停止使用先前链接或 session，等待更新后的玩家入口；只提供已确认的恢复说明。
- 更正链接：只分享已批准的 public join URL，说明旧访问信息不应继续使用，并提示可能需要重新认证或恢复。
- Session 处置：说明旧 session 已失效，请从玩家入口获取新 session；不披露原因、operator identity、内部 endpoint、用户标识或安全实现。

不得宣称“完全修复”“任何人都可安全分享”“production-ready”“零中断”“production custody”或暗示 public/mainnet launch，除非根 README、产品决策与 QA/LiveOps 证据明确授权。

## 7. 验证与升级

- plane isolation、revoke/recovery、admission 与 structured failure 的验证证据由 `doc/testing/**` 管理。
- 账户连续性、SMTP/store smoke 只证明对应链路，不证明 hosted join、custody、incident readiness 或扩大分享就绪。
- 任何玩家影响、外部承诺或公开更正升级给 TPM、`producer_system_designer` 与 `liveops_community`；技术 containment 升级给 runtime / blockchain ops owner；release impact 升级给 `qa_engineer`。
- 变更公开 claim、恢复分享、承诺时间表或补偿必须取得相应产品与流程授权。
