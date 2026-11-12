# 本地单执行者 TODO

本模式用于用户选择的受限运行方式：现有获准触发器唤醒 dot，dot 在自己的云端机器执行一次本地 Python 命令，由父任务恢复和持久保存 TODO 快照及执行记录。脚本不是 LLM 执行器，不运行 TODO 字符串、shell、Codex、模型或远端写入，不部署服务、不依赖 Sites 后端 cron、不创建或恢复定时任务。

## 采用条件与权限

父任务必须串行恢复与保存，不能覆盖活跃执行使用的本地文件；保证同一队列只有一个有效执行者，机器切换前核实旧执行已结束，恢复最新已确认快照。无法确认唯一执行者、最新状态或保存结果时，停止依赖动作。`flock` 仅保护同一机器、同一规范路径的本地文件；不同副本、跨机器、网络文件系统不受保护。版本与 SHA-256 校验能发现与预期不符的输入，但不是跨系统 CAS、授权证明或恰一次派工保证；父任务不能从待验证文件本身推导“可信最新版本”。本模式不承诺无人恢复。

统一 Issue、Sites、CI 三项及旧每日 Issue 保持暂停；本次交付不变更外部任务配置。以后启用仍须有对应用户授权并回读真实状态。时间和业务范围沿用 [SCHEDULE](SCHEDULE.md) 与任务说明；父任务负责生成已授权、已到期的有界输入，保留原实例 ID、计划日期、覆盖水位及全部未完成义务，脚本不自行展开日程或吞并历史任务。

dot 可直接完成已有授权范围内的有限核对与操作，不要求为此部署具备跨机器 CAS 的协调服务，也不强制另派 subagent。需要原生子任务时仍须具备原有服务与预算授权；创建响应未知保留 `unknown`，不能因超时、锁释放或租约过期再次创建。已有执行者不接管，Issue 未满足全部验收不关闭，未知写入先查原结果；Sites 更新接口授权、共用作业与 CAS 未验证时仍不写。禁付费、部署、权限修改等现有边界不因本模式改变。

## 文件边界与一轮操作

使用 Linux/POSIX 的 Python 3 标准库。调用入口为 [dot-todo.py](../../scripts/dot-todo.py)，每条命令至多变更一项，不循环执行任务。路径由调用者传入，运行目录位于仓库外；不要把真实状态、结果、日志、个人记录或凭据放进 Git。仓库只保留通用代码与合成测试。

1. 父任务通过已有持久化工具读取最新快照及其独立确认的 `revision` / `checksum`，恢复到私有本地目录。首次建队列必须显式核对来源及旧记录；脚本没有 `init`，文件缺失或损坏不创建空队列。无可恢复记录时停止相关工作。
2. `show` 校验后显示全部 TODO；`claim` 按输入顺序领取一个 `pending`。将输出中的完整 `snapshot` 持久保存并回读核对版本、内容。保存失败或未知不执行任务，先协调保存结果。
3. `start --id …` 将该项置为 `running`，再次保存完整快照并回读。只有本轮刚成功启动且保存核验成功的执行者可以做原授权动作；恢复得到的 `claimed` / `running` 只查询原操作，不能视为新的执行许可。父任务同时私有保存原操作目标、操作身份、原生子任务 ID（若已确认）和回执引用；外部动作前已有持久化意图，崩溃后即使无法判断是否真正发出也保持未决。
4. dot 核实原目标和原操作结果，写结果 JSON，用 `result` 推进状态。脚本只检查格式、任务内容摘要和状态转换，无法验证 URL、证据真实性、权限或业务完成；这些由 dot 和父任务通过真实工具核验。部分完成用 `blocked` 并列出剩余义务；未能排除在途执行或外部写入的项用 `unknown`，不能用 `blocked` 掩盖未知。
5. 保存输出快照和证据并回读后结束本轮。父任务每次唤醒最多推进一项实际任务，实际操作仍受原预算与停止要求约束，不在脚本或会话中常驻等待。只有收到下一次真实触发才继续；通知单独记录、核验和补发，不重做已完成业务。

任何非零退出、输出丢失、写入异常均先回读本地及持久副本，核对原动作。`os.replace` 前失败保留旧文件；替换后目录同步或 stdout 失败可能已改变文件，不能依据退出码盲重试。状态以私有临时文件写入、文件 fsync、同目录原子替换和目录 fsync 保存；`.lock` 是永久侧文件，不删除或以过期时间抢锁。运行目录仅供可信执行者使用，禁止手工并发覆盖、硬链接别名或绕开锁改写。

## 命令与数据

所有命令要求可信的预期版本和完整快照摘要，例如（占位符由父任务提供）：

```sh
python3 scripts/dot-todo.py --state "$STATE" --revision "$REVISION" --checksum "$CHECKSUM" show
python3 scripts/dot-todo.py --state "$STATE" --revision "$REVISION" --checksum "$CHECKSUM" --format text show
python3 scripts/dot-todo.py --state "$STATE" --revision "$REVISION" --checksum "$CHECKSUM" claim
python3 scripts/dot-todo.py --state "$STATE" --revision "$REVISION" --checksum "$CHECKSUM" start --id example-1
python3 scripts/dot-todo.py --state "$STATE" --revision "$REVISION" --checksum "$CHECKSUM" result --id example-1 --result "$RESULT"
```

每步成功后使用已保存并回读确认的新版本，不能复制上一条命令的版本。JSON 输出包含 `snapshot`、`selected` 和所选项的 `text_checksum`；文本输出供人查看，不作为持久化格式。

快照字段固定为 `schema: 1`、非负整数 `revision`、`tasks` 和 `checksum`。每项包含唯一稳定 `id`、完整义务 `text`、`status`、追加式 `history`。初始任务为 `pending` 且历史为空。`checksum` 是除自身之外全部快照字段的 SHA-256：UTF-8 JSON，键排序，`ensure_ascii=False`，分隔符 `(',', ':')`，拒绝 NaN；内容摘要同样计算，但输入为原 `text` 字符串。`revision` 等于全部历史条数，每次转换加一。更改内容或添加义务须由父任务在核对来源后显式维护新快照并独立保存新摘要；不得删除历史、更换已有 ID、改写原义务或重置在途状态来恢复执行。新义务用新的稳定 ID，原义务继续保留。

结果示例仅为合成结构：

```json
{
  "id": "example-1",
  "text_checksum": "<claim 输出的内容摘要>",
  "status": "blocked",
  "evidence": ["已核实合成来源无可用结果，未发起外部操作"],
  "remaining": ["等待来源恢复后核对原义务"]
}
```

状态转换为 `pending → claimed → running → completed / blocked / unknown`；领取后未执行也可转 `blocked / unknown`。存在任何 `claimed / running / unknown` 时拒绝新领取。`blocked` 保留义务，允许领取独立的其他项，不自动回到 pending。`unknown` 经核实原操作已结束且结果明确后才可由 dot 提交证据转 completed / blocked；blocked 也只能在原义务确已满足后转 completed。无自动重试或重置命令。需要继续实际执行的阻塞义务须由父任务核实原操作、取得必要条件后显式安排新的后续项，保留原 ID 关联；不能用于绕过未知或重复执行已完成步骤。完成必须有证据且 `remaining` 为空，blocked / unknown 必须有证据及未完成义务。

## 验证范围

运行 `python3 scripts/dot-todo.test.py`。测试使用临时合成文件，覆盖正常完成、重复领取、进行中恢复、未知结果阻止重试、缺失/损坏/旧 schema/版本及内容冲突、双进程竞争、原子写前后失败、阻塞保留和恢复往返。测试证明本地转换行为，不证明父任务持久化工具的跨会话恢复、跨机器互斥、原生派工查询、GitHub 写入权限或 Sites CAS 已接通；真实接入应另行核对，不能以单测成功恢复暂停任务。
