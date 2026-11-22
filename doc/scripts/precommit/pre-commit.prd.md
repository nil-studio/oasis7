# Pre-commit 与显式本地修复

- 执行记录: Git、PR、实际 CI 和评审记录；Issue 与 Project 按需使用

审计轮次: 4


## 目标
- 普通本地提交不执行格式化、编译、测试、lint 或治理检查；验证按[开发流程规范](../../engineering/workflow/source-of-truth.md)先取得最小有效本地反馈，再由 CI 执行必要检查；适用评审与实际保护设置继续约束合入。
- 将普通提交的兼容入口与操作者显式触发的修复/诊断分开，避免把本地便利脚本误报为 lifecycle proof。

## 范围
- **范围内**：保留 `scripts/pre-commit.sh` 作为已安装 legacy hook 的静默成功兼容入口。
- **范围内**：保留 `scripts/fix-precommit.sh` 作为操作者显式运行的 Rust 格式化、重新暂存和 `commit` tier 诊断入口。
- **范围外**：在本专题重复定义 `commit` / `required` / `full` 覆盖矩阵或 CI 分层。

## 接口 / 数据
- 脚本路径：`scripts/pre-commit.sh`
- 运行命令：`./scripts/pre-commit.sh`
- 执行内容：静默返回成功，不读取或修改暂存区，不调用 `git`、`rustfmt`、`cargo`、`npm`、`ci-tests.sh` 或治理脚本；开发者可在同一任务分支内用多个本地 commit 低成本记录内部步骤，待显式本地检查收敛后再成批推送到单个完整 PR，避免逐小步高频 push 反复启动并取消远程 CI。
- 日常按包开发反馈使用 `./scripts/cargo-dev.sh ...`，推送或提 PR 前的只读显式检查使用 `python3 scripts/check-local.py --profile quick` 或 `--profile affected`（含非阻断 `workflow_advisory` 任务文件规模与合批提醒，见 [testing-manual.md](../../../testing-manual.md#local-check-entry)）。`./scripts/ci-tests.sh commit` 保留为显式诊断/开发命令，不由普通 commit 调用，也不是 lifecycle proof。
- `./scripts/fix-precommit.sh` 依次运行 `env -u RUSTC_WRAPPER cargo fmt --all`、`git add -u` 和 `./scripts/ci-tests.sh commit`；它会修改已跟踪文件和暂存状态并无条件执行 `commit` 诊断清单，执行前后均应检查 diff，不能替代只读 `check-local.py`。
- 需要显式本地 required 检查时优先按[测试手册本地检查入口](../../../testing-manual.md#local-check-entry)选择 `check-local.py`（或按原合同运行 `./scripts/ci-tests.sh required`）；不应通过修复脚本或 legacy hook 隐式触发。
- 不另设 frozen-head Pre-PR Ready 或 `claim-ready` 通用准入；实际 required CI 与适用评审要求以开发流程规范和当前保护设置为准，本文不解除任何现行必需检查。
- 规则归属：
  - 普通 commit no-op 定义：canonical workflow、本文件与 `scripts/pre-commit.sh`
  - required/full 覆盖命令矩阵：`doc/testing/ci/ci-test-coverage.prd.md` 与 `scripts/ci-tests.sh`
  - required/full 分层定义：`doc/testing/ci/ci-tiered-execution.prd.md`
  - case 标签定义（`test_tier_required`/`test_tier_full`）：`doc/testing/ci/ci-testcase-tiering.prd.md`

## 最小验收命令
- `./scripts/pre-commit.sh`
- `bash scripts/pre-commit.test.sh`
- `bash -n scripts/pre-commit.sh scripts/fix-precommit.sh`

## Git Hook
- **注意**：Git hooks 不会随仓库内容一并版本化；克隆到新仓库（或重新初始化 `.git`）后，默认不会自动带上 `pre-commit` hook，需要手动重新注册。
- 在仓库根目录重新注册：
```
cat > .git/hooks/pre-commit <<'HOOK'
#!/usr/bin/env bash
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

./scripts/pre-commit.sh
HOOK

chmod +x .git/hooks/pre-commit
```
- 可用以下命令确认是否已注册：
```
test -x .git/hooks/pre-commit && echo "pre-commit hook installed"
```

## 失败修复
- legacy hook 调用该入口应始终静默成功；如果它失败或有输出，先运行 `bash scripts/pre-commit.test.sh` 验证兼容契约。
- Rust 格式漂移或显式 `commit` tier 失败时，运行 `./scripts/fix-precommit.sh`，审查它更新的暂存 diff，再针对剩余失败签名处理。
- `required` 或 CI 失败不是“pre-commit 失败”；其分层、覆盖和修复路由由 `doc/testing/ci/ci-tiered-execution.prd.md`、`doc/testing/ci/ci-test-coverage.prd.md` 与对应失败签名管理。

## 里程碑
- **当前态**：legacy pre-commit 保持静默 no-op，显式格式修复与 `commit` tier 诊断由 `scripts/fix-precommit.sh` 承载，CI tier 定义留在 `doc/testing/ci/`。

## 风险
- **覆盖时延**：未做最小有效本地验证时，回归可能延后到 CI required gate 才暴露；不得据此重新把检查塞回普通 commit。
- **环境差异**：本地与 CI 依赖不同可能造成结果不一致。
