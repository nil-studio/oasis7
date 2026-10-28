# Site 手册镜像治理入口

`doc/site/manual/` 只治理公开静态手册镜像与仓库 canonical 手册之间的映射；它不替代公开 HTML 页面，也不成为 Viewer 产品行为的权威来源。

## 从这里开始

- 想确认静态 docs hub、CN/EN 镜像和发布同步边界：读 `site-manual-static-docs.prd.md`。
- 想核对该专题的已完成状态、内容基线与后续同步责任：读 PR、实际 CI 与评审记录（Issue 按需）。
- 需要追溯 Viewer 手册的当前能力与历史退役边界：读 `doc/world-simulator/viewer/viewer-manual.manual.md`；任务过程从 PR、实际 CI、评审记录（Issue 按需）及 git history 追溯。
- 需要当前中文 canonical 手册：读 `doc/world-simulator/viewer/viewer-manual.manual.md`；公开只读镜像位于 `site/doc/cn|en/viewer-manual.html`。

## 收敛规则

- `site-manual-static-docs.*` 是本子树的主专题，承担当前静态手册架构、镜像边界与维护约定。
- 已完成的 2026-02 Viewer 手册搬迁碎片已语义收敛并删除；当前能力、退役边界与镜像责任分别由 canonical Viewer 手册和 `site-manual-static-docs.*` 承担。
- 当前内容真值由 canonical Viewer 手册及实际 `site/doc/**` 镜像共同决定。专题三件套保留任务、范围和审计可追溯性，不能据此覆盖当前产品行为。

## 删除审计

2026-02 Viewer 手册内容搬迁三件套已完成语义回填、引用修复和安全删除。玩家模式承诺以 `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md` 为准；当前 Viewer 操作及退役边界以 canonical Viewer 手册为准；CN/EN 镜像责任以 `site-manual-static-docs.*` 为准。
