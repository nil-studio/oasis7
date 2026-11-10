# Reveal.js 本地发行说明

本目录沿用 Reveal.js 5.1.0 的静态资源与 MIT 许可，供 `/deck/` 和 `/deck/en/` 使用。`LICENSE` 保留上游授权；样式及普通本地导航行为保持不变。

## 本地安全补丁

两份 deck 只需要本页键盘、hash、片段和触摸导航，没有跨窗口控制或远程 speaker notes 合同。因此本地 `dist/reveal.js` 移除通用 `message` 监听、按消息动态调用 deck 方法的入口，以及向父窗口广播状态/回调的路径。`postMessage` 默认关闭；即使 query 或本地 `configure()` 将相关配置置为 true，也不会恢复已移除的接口。普通 DOM 的 `ready`、`slidechanged` 等事件继续分发。

未被任何页面加载的 `plugin/notes/notes.js` 不再随站点发布。未来如果需要远程控制或 speaker notes，应先设计来源、发送窗口与消息 schema 的明确边界，并补实际浏览器回归，不能直接恢复旧 bundle。

嵌入播放器的格式化、开始和停止共用 URL 解析与精确 origin/path 判断：YouTube 为 `https://youtube.com/embed/` 或 `https://www.youtube.com/embed/`，Vimeo 为 `https://player.vimeo.com/video/`；带用户信息或其他端口的地址不识别为这些服务。相似主机及路径/query/fragment 中夹带服务名不获得专属参数或播放命令。API 参数用 `URLSearchParams` 更新，保留其他参数与 fragment；专属消息限定目标 origin，普通 iframe 的固定 slide 生命周期消息保持。现有双语 deck 未引入媒体内容。

补丁前 `dist/reveal.js` 的 SHA-256 为 `9736bf3d8918e391e8c07c85c53773800d4004dc25f948916e319ca371ca881a`。更新上游时需重新核对本地改动，而不是用原版 dist 覆盖。

媒体 `data-src` 与背景 video/iframe 在写入 `src` 前解析并仅接受 HTTP(S)，保留合法相对路径、query/hash 与各自的懒加载/卸载状态；危险或无效项不激活，也不标记成功。固定 `about:blank` 仅供内部卸载，CSS background-image 不属于这组入口。背景视频继续逐项处理逗号分隔来源，拒绝项不影响其他有效项。

## 验证

`scripts/site-deck-media-url-cases.cjs` 通过真实 bundle 检查媒体协议拒绝、合法相对/网络地址、source 与背景路径、preload/fragment/离场/再次进入，以及重复激活不重写 `src`。受控资源响应只验证 URL 与生命周期，不声称解码或播放真实媒体服务。

`scripts/site-deck-security-browser.test.cjs` 使用两个不同 loopback origin 实际发送消息，验证外部 `slide`、`configure`、`destroy`、错误 JSON 与 query/config 绕过不能控制页面或收到状态广播；同时检查中英文 hash 刷新、键盘翻页、片段以及移动端触摸按钮。该测试接入既有 `site-quality` 作业，截图和结果保存在 `output/site-deck-security-qa/`。本地运行方法见[站点验证手册](../../../doc/site/github-pages/public-site-discovery-validation.manual.md)。

同一入口执行 `scripts/site-deck-media-cases.cjs` 的真实浏览器媒体回归：实际加载本地 bundle，通过受控 iframe 响应核对合法服务和伪装地址的参数与 play/pause/slide 消息、query/hash 及自动播放/忽略控制。测试不连接真实第三方播放器，不代签视频服务可用性。
