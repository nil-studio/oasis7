# 单命令 ECS Preview 部署

## 上游需求承接

Trace refs: O7-REQ-54717f75-7a50-43ae-a713-c5d89272b3e4；承接[单机 Preview 部署入口](../prd.md#single-ecs-preview-deploy)。

### 2.1 需求承接与分配表

本次为部署工程入口，不改变玩家规则、产品范围或游戏 AC，因此产品层不适用；专业上游承接 scripts PRD 的单机部署要求。

| 上游 requirement / professional acceptance | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [单机部署要求](../prd.md#single-ecs-preview-deploy) | 已初始化 Preview 通过一个命令及一个 SSH 会话完成同源包校验、一致备份、切换及验收；失败如实隔离 | [部署合同](ecs-preview-deployment.design.md#deployment-contract) | 运维提供完整备份路径、原世界身份、有效 SSH 与同源成功包 | 产品 N/A；reason=只编排现有运维操作，不改变玩家规则或产品承诺；scope=已初始化 Linux/systemd Preview。排除新机初始化、世界治理、云扩容；自动验收不代签真实玩法 |

## 范围与入口

`scripts/deploy-ecs-preview.py --config <local-profile.json> --run <successful-ecs-run-id>` 是已初始化 Linux/systemd Preview 的推荐部署入口。配置一次 SSH target、release/backup 路径、现有 chain/player unit、公开 origin、只读世界身份与 reader key 文件位置。密钥只留在目标机；配置不含密码或 seed。沿用 OpenSSH agent/config/正常交互认证，只建立一个 SSH 进程，启用严格 HostKey 校验；认证失败立即结束，不自动重试。没有初始化世界、治理变更、云扩容或付费模型操作。

控制器固定 GitHub run 的完整 source SHA，要求 ECS workflow/main 成功且同 SHA 的 required-gate 成功。下载同一 run 的最终 ECS artifact，先核对外部 SHA256、归档成员边界、BUILDINFO 和整包 SHA256SUMS。所有后续动作绑定该 SHA，不追逐移动中的 main，不覆盖已存在但不匹配的 release。

<a id="deployment-contract"></a>
## 设计条款：执行顺序与失败

一个 SSH stdio 会话承载有界 JSON 控制帧与有界二进制流：init → install → backup → compare → switch → verify → accept。默认 remote 模式在目标本轮 operation 目录保存 0700 目录 / 0600 停机备份，fsync 后就地核对 SHA256、长度并以 `tar --compare` 对照原文件；控制器本地持久保存其身份和验收结果，不往返传输整个世界。init 在停服务前为源文件的保守备份字节估算、新版本和 528 MiB 余量做容量预检，停机前重新检查。备份输出目录不得落在备份源树内。显式 local 模式保留本机接收及回传比对协议，其两次整库公网传输增加停机时间，仅适用于调用者选择的环境。未比较成功不得切换。备份包含现有世界和身份、SQLite sidecars、两个 unit、部署配置与 Skill include；不会改写历史。安装流解压到独立 staging，只接受普通文件/目录、限制总展开长度、拒绝路径逃逸/链接/重复路径。安装前后均至少留 528 MiB，保留旧 release。

目标端持有维护锁，核对两个实际 unit 和完整进程范围；停止 player 再停止 chain，检查 cgroup 没有残留写入者。保存完整旧 unit，路径替换只发生在明确 old-release prefix，保留其他参数和环境；任何部分 unit 替换前即标记已修改，切换前失败原子恢复并复核旧字节，启动旧 chain 再启动旧 player。新 chain 启动是不可混淆的写入边界：此后失败或连接丢失先隔离当前候选写入者并保留历史，不直接让旧代码接管可能变化的数据，不将 binary rollback 称为数据 rollback。隔离失败记录真实错误；不终止身份已变化的其他部署。

切换前重新读取签名世界 tick，作为本轮不可倒退的基线。启动阶段使用独立的有界 readiness 等待：先确认 chain 与签名世界可读，再确认 launcher、Viewer 与 loopback admission 就绪；身份、签名或哈希错误立即失败，进程身份稳定后才冻结验收范围。启动等待不计入后续 60 秒观察。验收核对 chain/launcher/Viewer 子进程的实际 executable 哈希，基于原 reader/service pin 的签名只读世界响应保持 world/genesis/service identity，并在完整 60 秒内至少观察 3 次推进。probe 的 Python crypto 依赖在目标预先安装；缺失时在停服务前阻塞，不运行 pip 或下载依赖。公开 admission 路由、HEAD/POST/未知路径/Origin 边界按[部署手册](../../testing/manual/single-ecs-preview-deploy.manual.md)验证。

同版本 Skill 复用包内 `ecs-skill-release.py activate/verify`。调用它前释放外层同名维护锁，保留控制器锁；调用后重新获取维护锁并复核候选服务身份。并发状态变化时不覆盖新部署。只有全部部署验收成功才 accept；保留本轮一致备份、旧 release 和脱敏部署结果。此自动验收不代替外部 Chrome、Ed25519 登录、原请求回读或真实玩法完成。

## 固定会话协议

首行为 bootstrap JSON，包含受测 remote/probe 源；remote 程序随后读 JSON 行。普通响应为 `{ok:true,...}` 或 `{ok:false,error:<bounded-code>}`。`install` 请求声明 archive bytes/digest/expanded bytes/source，随后恰好传输 archive bytes；remote 模式 `backup` 返回 `{stream:false,bytes,sha256,remote_archive}`，`compare` 发送同一 bytes/digest 及 `remote:true`，不传输备份载荷，目标只验证本轮自己的固定归档。显式 local 模式 `backup` 先返回 stream header，随后传输 big-endian u32 长度与 chunk，长度 0 结束，再返回 bytes/digest；`compare` 声明同一备份 bytes/digest 后传输恰好该长度。默认 chunk≤1 MiB；总长和超时受已验证配置与本地可用空间约束。未知 op、缺帧、越界或错误均拒绝，不解释数据为命令。stdout 只作协议，进度与脱敏错误走 stderr。

## 验证

验证包括假 SSH 的完整调用顺序、单一认证/进程、精确包来源、低空间/篡改归档/半写 unit/备份不匹配/连接中断和 post-writer 隔离；真实部署另报告 host、source、run 与实际结果，fixture 通过不能代签线上完成。

### 11.1 验证映射表

| 上游 requirement / professional acceptance | 本设计条款 | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、场景与环境 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [单机部署要求](../prd.md#single-ecs-preview-deploy) | [部署合同](ecs-preview-deployment.design.md#deployment-contract) | 单会话、源校验、停机备份比较先于切换、写入后失败隔离 | [部署测试入口](../../../scripts/test-ecs-preview-deploy.sh) 运行 controller/remote/probe 回归；目标 Linux/systemd 主机需另外对固定包执行真实验收 | 当前 PR 的精确版本测试；真实部署按[部署手册](../../testing/manual/single-ecs-preview-deploy.manual.md#单命令更新)记录结果 | fixture 不证明线上；自动探针不证明外部 Chrome 登录与玩法 |

## 决策与风险

使用单一 SSH stdio 会话减少重复认证和中间文件，同时让断连对应明确的失败恢复边界。停机备份换取原始文件的一致性；新写入后的恢复需要保留当前历史并另行处理，不能自动启动旧代码。备份路径由运维配置完整覆盖世界、身份和 SQLite 目录；机器空间、已有 Python crypto 依赖和有效 SSH 认证属于部署前置条件，缺失时停止，不能由 fixture 验证代替。

公网往返完整世界的方案对低带宽、大历史世界会将停机时间放大到数小时，且可能超过阶段超时，因此采用原机一致备份及原机比对为默认。磁盘不足仍在停服务前阻塞；历史备份的离机迁移在服务在线期间完成，完整恢复链和记录验证保存后才释放远端副本。自动部署不猜测或删除旧备份，不承诺云扩容或零停机。
