# Codex 外部接续方法

产品边界由 [REQ-CC-005](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-005)、[REQ-CC-007](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-007) 至 [REQ-CC-009](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-009)主责；技术与兼容由[系统设计](../../../doc/engineering/command-center/command-center.design.md#DES-CC-005)主责。

1. 复用用户明确保存的原工作与会话定位，保留工具、环境和原标识，不按同分支猜会话关系。
2. 严格校验 `codex://threads/<UUID>` 后打开原会话；不接受附加路径、query、fragment或非法标识。其他准确定位只复制；不能将任意来源 URL 当本机命令。
3. 打开只证明导航请求被接受，不宣称恢复、执行或完成。结果从实际本地文档/Git或GitHub PR/CI核实；外部模型状态由原工具负责。
4. 无会话定位时保留原 PR/工作树入口，不要求用户连接运行时，不扫描私有数据库或JSONL来伪装监控。

本客户端已移除 Codex app-server、自管建会话、发任务/追加/中断、运行请求同步/回复和 Codex 退出协调；不要以 SDK、hooks 或 shell 备用路径恢复这些能力。旧启用偏好、运行/请求/私有记录保留，不重放、不投影成当前待答或活跃工作。独立本机工具任务仍沿原 ToolHost 安全退出合同。

验证原定位打开/复制、错误拒绝及返回位置；确认旧启用/活跃/未知状态不启动进程、不发模型任务、不阻塞退出。真实本地 App 验证不能由演示数据替代。新增运行控制须另行明确产品范围，不能由导航需求推定授权。
