# 原工具外部接续方法

产品范围见 [REQ-CC-005](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-005) 与 [REQ-CC-007](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-007)，导航校验见[系统设计](../../../doc/engineering/command-center/command-center.design.md#DES-CC-007)。

1. 使用用户明确保存的原工作与工具定位，保留工具、环境和原标识，不按同分支猜测关系。
2. 严格校验 `codex://threads/<UUID>` 后打开原会话；附加路径、query、fragment 或非法标识均拒绝。其他准确定位只复制。
3. 没有会话定位时使用原 PR 或工作树入口。CI 和评审从原 GitHub 页面查看，执行和模型状态在原工具处理。
4. 打开只证明导航请求被接受，不表示已恢复、执行或完成。结果沿实际文档、Git 和 GitHub PR 核实。

验证原定位打开/复制、错误拒绝与返回位置。真实本地 App 验证使用实际仓库和来源。旧个人与私有记录的兼容义务见[REQ-CC-008](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-008)。
