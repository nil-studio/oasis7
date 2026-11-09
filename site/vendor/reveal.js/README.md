# Reveal.js 本地发行说明

本目录沿用 Reveal.js 5.1.0 的静态资源与 MIT 许可，供 `/deck/` 和 `/deck/en/` 使用。`LICENSE` 保留上游授权；样式及普通本地导航行为保持不变。

## 本地安全补丁

两份 deck 只需要本页键盘、hash、片段和触摸导航，没有跨窗口控制或远程 speaker notes 合同。因此本地 `dist/reveal.js` 移除通用 `message` 监听、按消息动态调用 deck 方法的入口，以及向父窗口广播状态/回调的路径。`postMessage` 默认关闭；即使 query 或本地 `configure()` 将相关配置置为 true，也不会恢复已移除的接口。普通 DOM 的 `ready`、`slidechanged` 等事件继续分发。

未被任何页面加载的 `plugin/notes/notes.js` 不再随站点发布。未来如果需要远程控制或 speaker notes，应先设计来源、发送窗口与消息 schema 的明确边界，并补实际浏览器回归，不能直接恢复旧 bundle。

补丁前 `dist/reveal.js` 的 SHA-256 为 `9736bf3d8918e391e8c07c85c53773800d4004dc25f948916e319ca371ca881a`。更新上游时需重新核对本地改动，而不是用原版 dist 覆盖。

## 验证

`scripts/site-deck-security-browser.test.cjs` 使用两个不同 loopback origin 实际发送消息，验证外部 `slide`、`configure`、`destroy`、错误 JSON 与 query/config 绕过不能控制页面或收到状态广播；同时检查中英文 hash 刷新、键盘翻页、片段以及移动端触摸按钮。该测试接入既有 `site-quality` 作业，截图和结果保存在 `output/site-deck-security-qa/`。本地运行方法见[站点验证手册](../../../doc/site/github-pages/public-site-discovery-validation.manual.md)。
