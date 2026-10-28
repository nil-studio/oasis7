# headless-runtime 模块设计总览

审计轮次: 6

- 对应需求文档: `doc/headless-runtime/prd.md`
- 当前任务追踪: PR、实际 CI 与评审记录（Issue 按需）
- 对应文件级索引: `doc/headless-runtime/prd.index.md`

## 1. 设计定位
`headless-runtime` 模块的 `design.md` 描述无界面运行链路的总体设计，包括：
- 非 Viewer 运行模式的系统边界；
- 长时运行、认证、安全与归档相关设计；
- 与 world-runtime / p2p / testing 的集成位置。

## 2. 阅读顺序
1. `doc/headless-runtime/prd.md`
2. `doc/headless-runtime/design.md`
3. `doc/headless-runtime/prd.index.md`
4. 按当前索引进入 `checklists/` 与 `templates/`；旧 `nonviewer/` 专题已由模块 PRD/design 承接，历史追溯见 `doc/headless-runtime/README.md`。

## 3. 设计结构

### 组件职责与分层

- 运行形态层：定义 headless/nonviewer 模式与运行约束。
- 稳定性层：定义长时运行、内存、归档与恢复策略。
- 安全边界层：定义认证、协议与线上约束。

## 4. 集成点
- `doc/world-runtime/prd.md`
- `doc/p2p/prd.md`
- `doc/testing/prd.md`

## 5. 专题导航
- 当前鉴权、防重放、长稳与归档设计直接由本文件和模块 PRD 承载。
- `doc/headless-runtime/README.md` 解释已退役专题的旧命名与历史追溯。

## 6. 鉴权与恢复边界

### 鉴权主链路与拒绝流程

- ingress 使用版本化 canonical payload 与 ed25519 proof；payload 绑定动作、业务字段、player、公钥与 nonce，校验顺序是格式/字段绑定、验签、严格单调 nonce、防重放消费，最后才进入 Agent 绑定等业务授权。
- 缺失 proof、签名篡改、player/key 不匹配与 nonce replay 都是不同的显式拒绝；不得只记录日志后继续执行。

### nonce 状态流与恢复边界

- `player_auth_last_nonce` 属于可恢复状态，snapshot/restore 后继续阻止旧请求；journal replay 不重新授权或再次执行业务动作。

## 7. 长稳与冷归档边界

- 内存队列、动态 peer、committed batch 与热日志采用容量、TTL 或热窗口守卫；冷历史通过 CAS blob 加 refs/index 追溯。
- 归档不得阻塞在线主路径，也不得把本机 refs 等同分布式可得性。checkpoint、retention、GC、replay 与恢复的技术 authority 是 `doc/world-runtime/runtime/runtime-storage-footprint-governance.prd.md`。
- 旧专题中的 split-crate 文件路径仅是历史实施细节，不得复制为当前 authority；操作清单、事故模板和 release-gate 对接继续由 `checklists/` 与 `templates/` 承载。

## 设计目标
- 提供 `headless-runtime` 模块的总体设计入口。

## 设计范围
- 覆盖模块级结构、主链路、分层与专题导航。
- 不替代专题 `*.design.md` 的细化设计。

## 关键接口 / 入口
- 需求入口：`doc/headless-runtime/prd.md`
- 执行入口：PR、实际 CI 与评审记录（Issue 按需）
- 索引入口：`doc/headless-runtime/prd.index.md`

## 设计演进计划
- 历史建档（2026-03-09，ROUND-006）：模块级 `design.md` 入口来源；原 Task 未唯一定位，记录保留，不作为当前排期。
- 持续结构要求：高复杂度专题在对应 `*.design.md` 维护细化合同；不恢复旧 M2 任务。

## 设计风险
- 若专题级设计未及时补齐，模块级 `design.md` 可能承载过多导航职责。
- 若 legacy redirect 未明确标注为兼容跳转，读者可能误判历史入口为当前执行入口。
