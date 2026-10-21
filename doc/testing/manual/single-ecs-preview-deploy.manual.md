# 单机 ECS Preview 快速部署

适用：一台已配置好 SSH、systemd、世界目录和身份配置的 Linux ECS，更新 Preview 的 chain、launcher / Viewer，保留现有世界。首次开机配置只做一次；新世界初始化另见 [初始化手册](fresh-world-preview-bootstrap.manual.md)。正式多节点网络使用 [测试网运行手册](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md)。

正常路径只有五步：**选版本 → 预检 → 部署 → 验证 → 保留回滚版本**。失败才排障或回滚，不为一次普通部署新增审批系统。

## 1. 选定一个可用版本

使用同一完整 commit SHA 的已成功 CI 和 Linux Testnet 包。已有合格包就直接复用；main 后续前进本身不要求重打包、重测或重新追赶。遇到明确相关的安全、协议或存储兼容问题再处理。

当前单机云端更新优先使用 **ECS Server Package**，在 GitHub Actions 选择源码分支 / tag 后运行，或执行：

```bash
gh workflow run ecs-packages.yml --ref main
```

该入口固定本次选定 ref 的完整 SHA，Web 与原生在不同 runner 并行；只编译 `oasis7_chain_runtime`、`oasis7_game_launcher`、`oasis7_viewer_live`，不生成桌面安装器、client launcher、独立 web launcher 或维护工具。原生使用 `ecs` profile（release 优化级别 3、关闭跨 crate LTO、codegen-units 16），优先缩短构建时间；Cargo 并发自动为云端 2、自托管 8。冷构建仍需编译核心运行时依赖，实际耗时以 run 为准；部署前重新核对真实世界推进 / 吞吐，不沿用其他 profile 的性能结论。完整发行包的 `packaging` 配置不变。候选分支仅恢复缓存，main 同版本工作流才写入独立原生缓存。

默认 `native_runner=auto`：main 优先使用在线、空闲的 scc-ubuntu，否则自动使用已有 GitHub-hosted 构建。候选分支 / tag 始终使用云端。可直接运行：

```bash
gh workflow run ecs-packages.yml --repo nil-studio/oasis7 --ref main
```

组织管理员需先给 repository secret `ECS_RUNNER_STATUS_TOKEN` 配置一个仅具组织 Self-hosted runners read 权限的细粒度 token；查询使用 runner ID 2984。缺 token、查询失败、离线、忙碌或标签不匹配均回退云端。该 token 仅用于 main 的选择步骤，不传入编译任务。Default runner group 需允许 oasis7 公开仓库及该 workflow 使用 runner。机器需安装 rustup、Python 3、基础编译工具，支持非交互 sudo apt-get，且不持有生产凭据或世界目录。在线状态只是检查时快照，不能保证机器随后不会断线或被占用；本流程不自动重跑已启动但失败的构建。

并发默认随 runner 选择：自托管 8、云端 2；可用 `-f native_build_jobs=16` 指定上限，回退云端时自动限制为 2。需要固定云端对照时用 `-f native_runner=github-hosted`。Web 与最终组装继续在 GitHub-hosted 执行。此入口只产出部署包，实际部署沿下面的预检、切换和回滚步骤执行。

需要验证并发收益或回退单并发时，手动入口的 `native_build_jobs` GitHub-hosted 可选 `1` / `2`，自托管另可选 `4` / `8` / `16`，CLI 追加 `-f native_build_jobs=1`。同一 SHA 分别运行两个值，并核对缓存恢复 key、Cargo fresh / dirty 数量和 runner 条件。`ecs-native-timings` artifact 包含 Cargo timing HTML、`ecs-time.txt`（耗时 / CPU / 最大单进程 RSS）和 `ecs-resources.json`（每秒采样的可用内存、swap、适用的 cgroup 内存 / OOM 计数）。并行总内存压力以可用内存与 cgroup 观测共同判断；缺失字段为未知，cgroup 历史峰值不可当作本次独占峰值。记录失败样本及条件差异，实测结论留在 PR / run。

成功后下载 `oasis7-ecs-linux-x64` artifact，里面包含版本化 `.tar.gz` 和外部 `.sha256`。先用 `sha256sum -c <包名>.sha256` 校验归档，再解压到新版本目录并执行 `sha256sum -c SHA256SUMS`。`BUILDINFO` 的 source 必须等于本次 run 的 SHA。包内 `bin/` 是三个服务程序，`web-dist/` 是可直接发布的完整 Viewer Web 根目录（包含可选 WASM），`manifest.json` 记录来源 / profile / 平台。

正式游戏 Skill 随 ECS 包固定到同一个 source SHA：组装直接读取该提交下 `skills/oasis7-play/` 的受跟踪 Markdown 对象，拒绝符号链接及未发布的相对引用，不吸收工作区改动或未跟踪文件。输出为 `web-dist/skills/oasis7-play/`（含引用及 `manifest.json`）、`web-dist/skills/index.html` 和随包 `deployment/ecs-skill-release.py`。Skill 清单记录 source、每个源文件的 SHA-256 / 字节长度，清单及发现页本身由整包 `SHA256SUMS` 覆盖。不得把旧 `site/skills/oasis7.md` 覆盖到此路径。

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

### 同版本发布游戏 Skill

每次 player 升级和回滚均执行本步骤，使用实际运行的完整 ECS release；发布成功是本次部署验收的一部分。不要保留另一提交的手工静态 Skill，也不要单独从 main 获取文档覆盖运行版本。

首次接管 Nginx 时，在对应 HTTPS `server` 内加入一个明确的托管 include；路径可按本机约定选择，例如：

```nginx
include /etc/nginx/snippets/oasis7-skills.conf;
```

首次创建该普通文件为空（不要清空已有配置），移除先前手工维护的 `/skills/` 精确 location，避免重复路由。保留现有 Origin / 安全响应头 / API allowlist。在同一 `server` 的现有 `add_header` 层级增加发现头，避免在子 location 覆盖已有安全头：

```nginx
add_header Link '</skills/oasis7-play/SKILL.md>; rel="describedby"; type="text/markdown"' always;
```

先备份站点配置，执行 `nginx -t` 和 reload，失败恢复原配置；此 bootstrap 只做一次。后续发布工具只修改该 include，不改主站点、API、服务环境、权限开关或世界。HTTPS `server` 的 `/skills/` 路由只归此 include 管理；不能被另一 location 或其他 include 遮蔽。

切换服务并核对新进程后，释放外层维护锁，再执行随包工具（工具会非阻塞获取同一维护锁；不要在持有它的父进程中嵌套调用）：

```bash
release=/opt/oasis7-preview/releases/<verified-release>
source_sha=<full-deployed-commit-sha>
sudo python3 "$release/deployment/ecs-skill-release.py" activate \
  --release "$release" --expected-source "$source_sha" \
  --origin https://<public-host> \
  --player-service <player-service> \
  --nginx-include /etc/nginx/snippets/oasis7-skills.conf \
  --lock-file /run/lock/oasis7-preview-deploy.lock
```

命令先验证 release / Skill 的 source、整包校验和中的发布文件和各源文件哈希，再核对正在运行的 launcher 字节与此 release 相同。选择另一个版本、缺文件、篡改清单、未接入的 include 或锁被占用都会停止，不重启游戏服务。发布文件必须为 root 所有、目录可遍历、文件可读，且不能是符号链接。

工具将精确路径 alias 到此 release 的文件，稳定 URL 使用 `no-cache`；仅 GET / HEAD。正式入口为 `/skills/oasis7-play/SKILL.md`，引用保留相对路径；`/skills/` 和 `/skills/oasis7-play/` 转至发现页，旧 `/skills/oasis7.md` 转至正式入口。它不开放任意文件目录或 API。写入使用原子替换，旧 include 保存在同目录 `.previous`；随后 `nginx -t`、reload 并从公网逐字节核对 Skill、全部引用、清单和发现页。任一步失败都恢复旧 include、检查并 reload；回滚失败保留实际错误，不能宣称发布成功。失败期间不把旧 Skill 视为新游戏版本验收通过。

独立复核可运行只读命令：

```bash
python3 "$release/deployment/ecs-skill-release.py" verify \
  --release "$release" --expected-source "$source_sha" --origin https://<public-host>
```

同时检查原入口跳转、HEAD 空正文、POST 405、未知文件 404 以及主页发现头。回滚游戏时，用已校验的旧 release 重跑 `activate`，使页面和游戏一起回退；旧包若无本工具，使用原部署备份恢复对应静态发布配置并核对哈希，不能复用新版本 Skill。生成包和发布 Skill 不代表 Runtime 冷入场、游戏权限或真实试玩已通过。

## 4. 马上验证

- 确认上述 Skill 同版本发布和公网校验成功；失败时本次部署尚未完成，不以首页 HTTP 200 代替。
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

### ECS workspace cache validation

Retain actual trusted main native producer and compatible consumer run/job links, requested/restored keys, cache-hit, Cargo unit Fresh/rebuilt, transfer durations, and raw/compressed bytes. Inspect cache reports in `ecs-native-timings`; a cache miss or rejection must still perform the full native build. Exercise added, removed and renamed source inputs for conservative invalidation. Limits are 6 GiB staging, 6 GiB remote main family and two compatible generations; inventory failures or exhausted limits skip saving. Also inspect the live repository quota (positive integer GB converted conservatively to decimal bytes), observed usage across all refs/families and projected bytes in the cache report; missing quota or insufficient repository capacity skips saving without changing settings or deleting caches. Concurrent writers can change capacity after this point-in-time guard, so it does not reserve space atomically. Record pending remote evidence explicitly rather than treating local tests as cross-run reuse proof.
