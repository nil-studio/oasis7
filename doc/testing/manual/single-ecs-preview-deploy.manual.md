# 单机 ECS Preview 快速部署

适用：一台已配置好 SSH、systemd、世界目录和身份配置的 Linux ECS，更新 Preview 的 chain、launcher / Viewer，保留现有世界。首次开机配置只做一次；新世界初始化另见 [初始化手册](fresh-world-preview-bootstrap.manual.md)。正式多节点网络使用 [测试网运行手册](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md)。

正常路径只有五步：**选版本 → 预检 → 部署 → 验证 → 保留回滚版本**。失败才排障或回滚，不为一次普通部署新增审批系统。

## 1. 选定一个可用版本

使用同一完整 commit SHA 的已成功 CI 和 Linux Testnet 包。已有合格包就直接复用；main 后续前进本身不要求重打包、重测或重新追赶。遇到明确相关的安全、协议或存储兼容问题再处理。

当前单机云端更新优先使用 **ECS Server Package**，在 GitHub Actions 选择源码分支 / tag 后运行，或执行：

```bash
gh workflow run ecs-packages.yml --ref main
```

该入口固定本次选定 ref 的完整 SHA，Web 与原生在不同 runner 并行；只编译 `oasis7_chain_runtime`、`oasis7_game_launcher`、`oasis7_viewer_live`，不生成桌面安装器、client launcher、独立 web launcher 或维护工具。原生使用优化的 `packaging` profile；冷构建仍需编译核心运行时依赖。候选分支仅恢复缓存，main 同版本工作流才写入独立原生缓存。

成功后下载 `oasis7-ecs-linux-x64` artifact，里面包含版本化 `.tar.gz` 和外部 `.sha256`。先用 `sha256sum -c <包名>.sha256` 校验归档，再解压到新版本目录并执行 `sha256sum -c SHA256SUMS`。`BUILDINFO` 的 source 必须等于本次 run 的 SHA。包内 `bin/` 是三个服务程序，`web-dist/` 是可直接发布的完整 Viewer Web 根目录（包含可选 WASM），`manifest.json` 记录来源 / profile / 平台。

这是服务端目录包，不是旧 Testnet `.deb` 的替代安装器；旧安装脚本若只接收 `.deb`，不能直接传入此归档。配置三个程序及 Web 根目录路径后沿下文切换。包不含世界、密钥、systemd unit 或自动初始化 / 公网开放操作，始终复用本机现有配置。维护工具单独沿已有兼容版本保留，按需更新。

需要桌面发行包时才使用原 `testnet-packages.yml`。`dev` 仅用于开发调试，不能用其吞吐表现判断 ECS 是否达标。

## 2. 部署前看四件事

- **服务**：确认本机实际的 chain、launcher / Viewer 服务名、启动路径和当前版本。launcher 可能启动 Viewer 子进程，不能只看 systemd MainPID。
- **数据**：沿用现有世界目录、密钥、world ID、genesis 和服务公钥；确认已有可恢复的数据备份。更新 chain 前确认新旧程序对现有持久格式的兼容性；只更新 player 时，保持 chain 的二进制、参数和进程不变。
- **回滚**：保留当前可用版本及其 unit / 配置 / 版本指针。不要在新版本验证前删除它。
- **空间**：空间应覆盖包、解包临时文件、新版本和回滚副本，并留至少 528 MiB；复用已经校验过的安装版本时不重复上传或安装。

以下命令在 ECS 上执行；占位符替换为实际值：

```bash
df -h /opt /var/lib
systemctl show <player-service> -p ActiveState -p SubState -p MainPID
systemctl show <chain-service> -p ActiveState -p SubState -p MainPID
readlink -f <player-version-link>
```

不要把密钥、完整环境变量或含凭据的启动参数贴到日志。无需为每次部署生成多层签发收据；记录候选 SHA、包哈希、原版本和实际结果即可。适用的代码安全评审沿仓库正常流程完成。

## 3. 安装到新目录，再切换

1. 把已校验的包安装 / 暂存到独立版本目录，完整保留随包资源。按 [包安装与来源规范](../../engineering/workflow/package-source-security.md#installed-package-boundary)使用受支持安装方式；先校验归档，再解包，不能只复制一个 binary。
2. 已有同版本目录时验证内容一致后复用，不覆盖正在运行的目录。现有部署脚本可直接完成这一步，不再套新的审批包装器。
3. 在同一维护锁中保存旧 unit、配置和指针；只将本次更新组件的二进制路径切至新版本，保留原参数、世界路径和私有配置。各服务使用明确版本路径，避免修改仍被其他组件引用的共享指针。
4. 重启 player 服务。unit 改过才需要 daemon-reload：

```bash
sudo systemctl daemon-reload   # 仅 unit 有改动时
sudo systemctl restart <player-service>
systemctl show <player-service> -p ActiveState -p SubState -p MainPID
```

同时更新 chain 时，先停止 player，切换并重启 chain 后再启动 player；沿用原世界目录，不运行初始化器。

版本路径修改采用本机已有的服务配置 / 部署脚本；本手册不假设所有 ECS 都有同一个升级命令。首次接管机器时确认一次路径即可，后续复用。

## 4. 马上验证

- 最多 15 秒确认 launcher / Viewer 实际新进程启动，运行的文件哈希匹配候选。只更新 player 时确认 chain 未重启；更新 chain 时确认新链进程及其哈希。
- 做一次真实签名世界读取，核对原 world ID、genesis、服务公钥和读身份。端口打开、HTTP 200 或 `--version` 不能替代这个检查。
- 连续观察完整 60 秒，至少看到 3 次有效世界推进；只有最初快照不算推进。不反复发送初始快照请求凑数。
- 打开实际页面，确认世界可见、持续刷新；检查桌面和窄屏。涉及账号 / Agent 功能时继续对应功能验收，不把部署成功当成试玩成功。

trace session ID 默认每次启动变化。验证它属于当前 launcher 的 PID 和启动时间，或与显式配置按程序 trim 后一致；不要要求重启前后 trace 字符串相同。其余稳定身份和配置仍应一致。

全部通过后确认新版本选择生效，记录结果，继续使用；没有失败就不再增加长测、重打包或额外流程。

## 5. 失败时回滚，成功后清理

失败先看具体阶段及已有远端结果，不盲目重跑切换。恢复保存的 player unit、配置和版本路径，按上节重启，再核对旧版本进程、原身份及真实读取。受控脚本必须在失败时执行回滚并验证；需要日志采集时最多 3 秒，随后给回滚独立 6 秒预算。回滚未核实就如实报告，不能写成功。

回滚二进制不会回滚世界历史。若发生了 chain 升级、数据迁移或未知数据写入，保留现场，按相应恢复流程处理；不要重跑初始化器、重生世界或重封存 genesis。

成功后再删除未使用的历史安装目录，只保留当前和上一个可用版本；混合版本运行时保留所有实际被进程、unit 或配置引用的版本。世界数据、密钥、备份、journal 和失败证据不属于旧版本清理范围。

部署记录放在实际任务 / Issue：`候选SHA、包SHA256、更新组件、前后版本、真实验证结果、失败/回滚结果`。手册保持通用步骤，不写入当轮 PID、密码、临时目录或审批流水。
