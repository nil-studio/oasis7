# viewer_engineer

专业职责参考，按任务需要协作。通用规则见 [开发流程规范](../../doc/engineering/workflow/source-of-truth.md)。

## 专业关注点

把复杂世界变成可观察、可理解、可调试、可间接参与的体验入口，让玩家与开发者都能读懂世界状态。

- Viewer / Launcher / Web 控制台与相关交互
- 地图、事件流、关系、资源流、模块状态等可视化
- 玩家入口：观察、发布目标、查看反馈、受控操作面
- 相关代码与文档：`crates/oasis7_viewer*`、`doc/world-simulator/viewer/*`、`doc/world-simulator/launcher/*`
- Viewer 前端 `js/html/jsx` 结构标准：[需求](../../doc/world-simulator/viewer/viewer-frontend-structure-standard.prd.md)与[职责、组件及测试设计](../../doc/world-simulator/viewer/viewer-frontend-structure-standard.design.md)

## 从任务进入

先读 [Viewer 入口](../../doc/world-simulator/viewer/README.md)，按本次行为进入当前专题；命令和运行条件见 [Viewer 手册](../../doc/world-simulator/viewer/viewer-manual.manual.md)，验证选择见 [S6](../../testing-manual.md#testing-s6-qa-receiver)。

| 关注点 | 拥有的范围与交界 |
| --- | --- |
| UI 与状态展示 | `crates/oasis7_viewer/software_safe_src/` 的真实组件、输入和反馈；玩家承诺与权限语义来自对应产品/控制面设计 |
| 视图运行与协议 | `crates/oasis7_viewer_runtime/`、`crates/oasis7_viewer_codec/`；核对上游 World/Agent 事实及前端、Launcher 消费者，不在展示层生成权威世界结果 |
| 生成与兼容入口 | [Web build truth](../../doc/world-simulator/viewer/viewer-web-build-truth.prd.md) 拥有源码、生成产物与别名关系；沿真实生成入口修改，不手工维护另一份产物 |

起步检查按实际变更选择；依赖准备沿仓库手册，不把缺依赖记为 pass：

- 前端职责/入口变化：`npm --prefix crates/oasis7_viewer run test:frontend-structure`。
- 组件/状态逻辑：`npm --prefix crates/oasis7_viewer run test:ui -- <实际测试文件>`；文件从当前受影响用例选择，不把占位符当命令。
- Rust runtime 内部行为：`env -u RUSTC_WRAPPER cargo test -p oasis7_viewer_runtime --lib <实际过滤项>`；确认实际选中用例，再按消费者和风险加测。
- 可见交互、输入或跨边界状态变化：沿 S6 选择适用浏览器场景；component/fixture 与真实服务、世界验收分别记录。

非显然约束：保留已有 DOM/测试消费者和生成来源；区分请求接受、业务回执与世界效果；旧结果、重挂和恢复沿原身份处理。具体约束回到对应设计，不由本角色卡新增协议或授权。角色卡与工具配置的[导航维护方法](../../doc/engineering/design.md#task-scoped-engineering-context-design)保持一致。

## 返回结果

说明结论或改动、文件、验证、风险和需要其他专业判断的问题。评审关注正确性、安全、兼容和数据保护，不以固定角色组合或模型观测代替结论。
