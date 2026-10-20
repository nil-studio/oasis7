# Viewer WebSocket/HTTP Bridge 设计

- 对应需求文档: `doc/world-simulator/viewer/viewer-websocket-http-bridge.prd.md`
- 历史 bridge 实施与验证记录：GitHub task issue evidence。

## 1. 设计定位
定义 Web Viewer 通过 WebSocket bridge 接入 live server 的协议、桥接生命周期与错误恢复策略。

## 2. 设计结构
- 服务端层：`oasis7_viewer_live --web-bind` 启动 bridge。
- 协议层：WebSocket text frame 与 TCP line protocol 互转。
- 客户端层：viewer wasm 接入 WebSocket 客户端与错误状态呈现。

## 3. 关键接口 / 入口
- `--bind` / `--web-bind` 参数
- `ViewerRequest/ViewerResponse` JSON 协议
- WebSocket 地址配置与 UI 状态接口

## 4. 约束与边界
- 浏览器断连时必须主动释放 upstream socket。
- Web 端只承担 Viewer + 网关接入，不承担完整分布式节点职责。
- 协议错误必须显式反馈到 UI 状态。

## 5. 设计演进计划
- 先完成 Design 补齐与互链回写。
- 后续实现与验证以新建 GitHub-backed task、当前代码和本合同为准。


## 6. 目标、上游与当前结构边界

Owner：viewer_engineer；消费者为 Web Viewer、live TCP 服务与本地 launcher。本文维护 WebSocket 到 TCP JSON 行协议的传输与连接回收，不改 Viewer 业务请求语义。原 wasm 客户端描述是历史接入阶段；当前 Web 状态消费者为 `software_safe_src/legacy_core.js`，不据此恢复旧 wasm UI。

### 2.1 需求承接与分配表

| 上游 requirement / product AC | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [客户端独立交互与重连](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | 仅承接Web连接链路的请求/响应转发与刷新后上游释放；技术细则见[配对PRD传输规则](viewer-websocket-http-bridge.prd.md#2-bridge-传输协议) | [传输与会话释放](viewer-websocket-http-bridge.design.md#bridge-session) | runtime 拥有请求接受/世界效果；launcher 提供地址；qa_engineer 提供正式surface证据 | 本设计不能单独满足整个产品要求；原生容器、定位/对象检查、跨surface验收仍由相关设计与QA负责 |

<a id="bridge-session"></a>
## 7. 流程、状态与失败恢复

连接成功后建立 upstream TCP socket；text frame 追加换行并 flush，upstream 按完整行回推 text frame。拆分到多次读取的行必须保留缓冲，不能当作多条响应。业务 JSON 的接受/拒绝由上游 Viewer 协议处理，bridge 转发成功不是 world applied。

浏览器 close、协议错误或上游结束使本会话退出，必须 shutdown upstream 两端以释放 clone 持有的连接；随后刷新可建立新会话。当前 `web_bridge.rs::serve_stream` 使用短读超时轮询与上游非阻塞读取，独立会话由线程承接。原 PRD 的读写分离风险描述不应理解为必须恢复旧读线程拓扑。

bridge 不持久化世界状态，不创建重试队列，不在重连时自动重发业务动作。前端 error/close 必须可诊断；重新连上只证明传输恢复，世界状态须以重新收到的权威消息为准。

## 8. 部署、安全、容量与兼容

保留 `--bind`/`--web-bind` 区分及原JSON行协议；默认本地开发地址不是公开网关安全承诺。当前代码另有可选 game-read HTTP API，该入口的权限/作用域合同不由本WebSocket专题替代；不因为标题含HTTP就扩展本设计到全部HTTP路由。

并发连接、慢上游和拆分响应须分别验证；这里不承诺无限客户端数、跨断线exactly-once或高可用。错误分类与断连清理是兼容边界，回滚桥接实现也不能遗留上游会话或用“假在线”掩盖失败。

Live Runtime 的单次响应帧数默认预算须容纳完整的有界历史回放：最多 256 条 batch、512 条 challenge，加初始 snapshot、恢复元数据、metrics 三帧。该预算随历史保留上限统一推导，不能因正常世界运行累积到保留上限而关闭新浏览器连接。单帧 16 MiB、单次 64 MiB 和写入 deadline 仍独立生效；显式配置的更小预算仍须拒绝超限输出，不静默截断历史或关闭资源限制。

## 11. 验证设计

### 11.1 验证映射表

| 上游 requirement | 本设计条款 | 独立 obligation 与适用条件 | 验证方法 / source / scenario / candidate | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [客户端独立交互与重连](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | [传输与会话释放](viewer-websocket-http-bridge.design.md#bridge-session) | 本地Web连接按完整JSON行双向转发；断连释放全部upstream引用并允许刷新后重连 | [测试手册](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；执行 `cargo test -p oasis7_viewer_runtime web_bridge::tests`，再真实刷新Web页面；L2 转发/拆行/并发/刷新，L4A真实Web重连；记录 source、TCP/WS绑定、runtime场景、客户端构建与浏览器；测试使用本地可用端口 | 实际PR/验证记录：bridge测试输出和退出码；转发/拆行/并发/刷新样例；真实Web重连的state、console、截图与服务端会话日志 | Rust桥接测试不代签真实Web错误显示或原生客户端；未覆盖公网安全、无限并发与跨断线exactly-once |

## 12. 决策、风险与未决范围

沿用行协议减少业务协议分叉，代价是不能把传输成功当作事务完成；背压/资源上限与公网安全仍需专项设计。当前仅静态核对源码与测试位置，没有运行socket或浏览器测试；历史bridge闭环证据仍按其原candidate解释，产品整体验收不由本表升级。
