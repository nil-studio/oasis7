# Codex Issue 开发队列

这是用户选择采用的本机协作入口：dot 准备明确的 Rust/JS 开发 Issue，Codex 所在机器每 5 分钟检查可领取任务，开发会话实现并跟进 PR，dot 继续已获准的部署、测试和反馈。普通 PR 可以沿用现有流程，不要求先建 Issue。

Trace refs: O7-REQ-59e8a26e-0c16-4a0b-b449-ce94601ebf2d

本目录承接[开发流程规范的可选交接要求](../../doc/engineering/workflow/source-of-truth.md#codex-issue-queue)：用持久线程和有界执行减少重复派工，以 GitHub 实际交付结果决定下一步。它不授予新的模型、GitHub、合入或部署权限，不提供游戏内 Agent 能力。dot 的职责和时间表仍见 [dot 工作约定](../../.agents/dot/README.md)。

## 分工与完成条件

| 执行者 | 负责什么 | 什么时候交接 |
| --- | --- | --- |
| dot | 查重；写清目标、来源与范围、验收、依赖、交付终点；在已有授权内发布任务 | Issue 内容和依赖清楚，且位于获准的队列消费范围内时添加 `codex:ready` |
| Codex 端运行器 | 验证发布者，去重领取，保存线程与事件，核对 GitHub 变化，安排或暂停后续轮次 | 首次领取创建线程，同一 Issue 后续继续原线程；未知结果先核对，不盲目重新启动 |
| Codex 开发会话 | 在独立工作区实现、自测、组织适用评审，交付 PR 并处理当前 HEAD 的 CI/评审问题 | 按任务的实际授权交付可评审 PR 或完成正常受保护的合入；不把会话完成视为任务验收 |
| dot | 在具体环境授权内部署、测试，核实版本和实际结果；失败反馈到原 Issue | 整体要求及适用部署、真实验收满足后，在已有关闭权限内关闭 Issue |

PR 使用 `Refs #<issue>`，避免代码合入提前关闭仍有部署或验收义务的任务。调度器不自动合入 PR、不自动关闭 Issue；`allow_merge` 默认为 `false`。即使运行器允许合入，任务仍须有明确合入授权并满足通用规范，Issue 正文不能替用户扩大授权。

## 投递与反馈

使用[代码开发 Issue 表单](../../.github/ISSUE_TEMPLATE/code-development.yml)，或提供等价正文。任务类型必须含稳定标记 `code-development`；运行器不依赖标题格式或中文 Markdown 字段顺序识别任务。模板不默认添加 ready 标签。

只有受信任的 Issue 作者与受信任的 ready 标签操作人共同满足配置时，运行器才允许领取。`trusted_users` 应只包含已经获准投递和控制此队列的 GitHub 登录名；未知作者、未获准的标签操作、缺少标记或事实读取失败，不进入付费执行。标签名称本身不是授权。

| GitHub 状态 | 含义 |
| --- | --- |
| `codex:ready` | 授权发布者声明任务已具备领取条件；不是已经开始执行 |
| `codex:active` | 队列已接手任务；实际线程、PR 和当前进展仍须回读 |
| `codex:blocked` | 当前存在需要处理的具体阻塞；保留原因与已有工作，不靠反复触发绕过 |
| `codex:paused` | 用户暂停后续执行；解除暂停需要对应的后续授权 |
| Issue 已关闭 | 不再领取或续跑；关闭应发生在整体义务完成或用户明确终止后 |

受信任账号也可在原 Issue 评论 `/codex pause`，后续由有权解除该暂停的账号评论 `/codex resume`。运行器读取最新有效控制评论；`codex:paused` 标签仍优先，resume 评论不会移除它。需要恢复时核对原暂停来源并解除对应限制，不通过改写任务或新建 Issue 绕过。

同目标的测试失败、部署后复现或评审反馈补回原 Issue，附受测 source/build、环境范围、复现、实际结果和预期结果；不要每次失败新建 Issue。只传必要且可公开的信息。代码 PR 合入后，dot 仍需完成适用验收；完成后由受信任账号发布：

```text
/codex accept <完整 source-head-sha>
```

此处使用已验收 PR 的完整源分支 HEAD，不是 squash/merge commit。运行器须回读 PR 实际已合入、源 HEAD 匹配，且验收声明晚于最近有效反馈，才接受该声明；评论本身不代替部署或测试证据，也不自动关闭 Issue。新增代码修复继续原任务和线程，验收声明不能沿用到另一个候选版本或覆盖之后的新失败反馈。

合入后的测试若确认需要代码修复，dot 在原 Issue 使用下面的首行并附复现与证据。运行器只为匹配当前已合入 PR 源 HEAD 的修复请求接续开发；部署进度、旧构建反馈和没有修复指令的报告不会另起代码轮次。开放 PR 的当前 HEAD CI 失败仍自动触发修复。

```text
/codex fix <完整 source-head-sha>
环境、受测构建、复现、实际结果、预期结果与证据链接。
```

队列及其开发会话发布的评论首行使用 `<!-- oasis7-codex-queue -->`，这些进度与交付报告不作为新输入。dot 的测试反馈、用户控制和验收评论不加此标记，避免把自己的报告反复派回模型。

依赖未满足时不发布 ready。需要新环境、费用或用户决定时，仅暂停依赖新增授权的动作；原范围内可独立推进的诊断与准备继续。普通修复不重复申请已经覆盖的授权。

## 本机配置

运行器使用 Python 标准库、已安装的 `gh` CLI 和 `codex app-server`，只支持具有 POSIX 文件锁的 macOS/Linux。GitHub 与 Codex 沿用本机已经配置的身份；脚本不安装工具、不登录、不索取凭据。使用专门的私有配置和状态目录，放在仓库与共享目录之外；不要提交数据库、线程内容或日志。

App Server 使用 `workspaceWrite` 与 `approvalPolicy: never`，不会自动开放网络或响应新的权限申请。本机既有且已获准的 Codex 配置须允许所需 GitHub 访问；仅在外层登录 `gh` 不代表模型沙箱能够推送 PR 或读取 CI。缺少该条件会保留阻塞，不修改全局权限来绕过。

```json
{
  "repository": "nil-studio/oasis7",
  "checkout": "/absolute/path/to/oasis7",
  "state_dir": "/absolute/private/path/oasis7-code-queue",
  "trusted_users": ["approved-github-login"],
  "codex_command": ["codex", "app-server"],
  "gh_command": ["gh"],
  "max_turn_seconds": 3600,
  "max_turns_per_issue": 8,
  "allow_merge": false
}
```

`checkout` 是已核对仓库身份的干净根 checkout，不是任意待执行的 Issue 附件路径。运行器首次使用仓库既有 worktree 入口创建独立工作区，串行任务复用该路径及编译缓存；切换分支前保留未提交工作，不强制重置。命令以参数数组执行，不把 Issue 文本拼接进 shell。配置文件使用仅当前用户可读写的权限，状态目录只供当前用户访问；模型设置、凭据和其他运行权限沿用获准的本机配置。

启用前明确仓库、受信任发布者、模型与费用范围、交付终点、停止方式及部署边界。`max_turn_seconds` 和 `max_turns_per_issue` 是执行上限，不是精确费用估算；同一事实最多执行 2 轮，每个 Issue 默认总计最多 8 轮。达到上限保留已完成工作并报告阻塞，先诊断原因，不自行扩大额度；`resume` 不清除轮次计数。同目标续跑在该范围内无需逐次确认，新的范围仍按已有授权规则处理。

一个 `state_dir` 只运行一个 dispatcher 和一个 worker，以文件锁和私有 SQLite 保存领取、线程与事件。五分钟触发只检查事实，没有可执行变化时不调用模型。不要在多台机器以不同状态目录同时消费同一队列；本入口不提供跨主机分布式锁。

## 手动检查与控制

首次配置仓库标签时，可在已有仓库管理授权内运行以下命令。它只创建缺少的四个队列标签，不给任何 Issue 加 ready，也不启动模型：

```sh
python3 scripts/ci-codex-queue/dispatcher.py --config /absolute/private/config.json init-labels
```

以下示例中的绝对路径须换成本机已核实的配置；从持有本目录脚本的仓库执行。`tick` 是实际消费入口，有 ready 任务时可能启动付费执行，应在队列服务与预算已获准后使用。

```sh
python3 scripts/ci-codex-queue/dispatcher.py --config /absolute/private/config.json status
python3 scripts/ci-codex-queue/dispatcher.py --config /absolute/private/config.json tick
python3 scripts/ci-codex-queue/dispatcher.py --config /absolute/private/config.json pause
python3 scripts/ci-codex-queue/dispatcher.py --config /absolute/private/config.json resume
python3 scripts/ci-codex-queue/dispatcher.py --config /absolute/private/config.json resume --issue 123
```

`pause` 写入 `state_dir/STOP`，后续领取与执行轮次须遵守该停止条件。`resume` 是显式解除本机暂停，不解除 Issue 上仍有效的 `codex:paused` 或用户限制。暂停后回读实际 worker 和 App Server 状态；仅有停止文件、标签或定时器停止，不表示在途调用已经终止。内部 `worker` 子命令由 dispatcher 使用，不手动复制派工。

`resume --issue 123` 只请求恢复这个已领取任务，保留原会话、分支和已用轮次，不解除全局 STOP 或 Issue 暂停。原事件结果不明确时先回读原事件；明确结束但需要再尝试时生成新的事件并继续原线程。Issue 领取评论会记录线程 ID；`status` 提供本机详细诊断，公开阻塞评论只给出安全原因码、轮次与恢复入口。

## 每 5 分钟触发

定时器运行在 Codex 所在机器的普通登录用户下：macOS 使用 launchd，Linux 使用 systemd user timer。先渲染查看文件，不安装、不启动：

```sh
python3 scripts/ci-codex-queue/schedule.py --config /absolute/private/config.json render --output-dir /absolute/private/rendered
```

全局选项 `--platform macos` 或 `--platform linux` 可用于跨平台预览；`--python` 指定绝对 Python 路径，`--runtime-path` 指定仅由绝对目录组成的 PATH，确保后台服务能找到已安装的 `gh`、`codex` 及服务管理器。脚本只保存所需路径，不复制身份令牌到服务定义。

只有在目标机器、服务身份和消费授权已经核实后，才安装并回读真实服务状态：

```sh
python3 scripts/ci-codex-queue/schedule.py --config /absolute/private/config.json install
```

安装成功后配置为每 300 秒触发，首轮计划在安装后的 300 秒触发；机器休眠或用户服务不可用时不能保证准点。安装动作不立即启动 Codex 任务。`install` 会在服务状态核实成功后解除本机 STOP，恢复定时检查；若原 worker 已因暂停结束，再用 dispatcher 的 `resume` 或 `resume --issue <编号>` 恢复原会话。安装或回读失败时保持暂停，不把生成文件当作已经启用。

停止或卸载使用同一配置：

```sh
python3 scripts/ci-codex-queue/schedule.py --config /absolute/private/config.json stop
python3 scripts/ci-codex-queue/schedule.py --config /absolute/private/config.json uninstall
```

两者先写 STOP 并停用定时器，保留 SQLite 和线程记录以便恢复；卸载只移除本安装拥有且未被外部修改的调度文件。在途 worker 的停止仍须独立核实。不要删除状态目录来代替暂停或修复，否则可能丢失领取和线程身份。停止后若只是运行 dispatcher 的 `resume`，不会重新安装或启用定时器。

这个 5 分钟入口与 dot 原有的 20 分钟日程分别配置。提交本目录、创建 PR 或渲染定时器，都不意味着任一日程已启用。

## 失败、恢复与验证边界

运行器在提交模型轮次前持久化事件，收到线程身份后立即保存；恢复先查原线程及对应事件，不能因超时、响应丢失或进程退出就再创建一次任务。无法确认上一轮结果时保留阻塞，由实际线程、PR 和进程事实协调恢复；不宣称跨 GitHub、SQLite 和模型服务的事务性“恰好一次”。

PR 跟进读取当前 source HEAD、适用检查和评审。仍在运行的 CI 继续等待；明确失败或新的有效反馈才触发对应修复。main 前进本身不使已有检查失效。模型文字称“完成”不替代 PR、检查、评审、实际合入及 dot 的部署/验收结果。

纯本地测试应覆盖重复 tick、同 Issue 恢复、响应丢失、暂停、执行上限、未知或失败的 GitHub 状态、当前 HEAD 变化以及调度文件渲染。协议 fixture 和模拟服务管理器只能验证确定性的控制逻辑，不能证明真实 Codex 登录、费用、远程写入、定时器已安装或实际部署验收。真实启用后的核验记录放在当前 PR/Issue，长期文档仅维护此操作方法与边界。
