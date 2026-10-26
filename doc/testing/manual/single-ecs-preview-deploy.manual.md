# 单机 ECS Preview 快速部署

适用：一台已配置好 SSH、systemd、世界目录和身份配置的 Linux ECS，更新 Preview 的 chain、launcher / Viewer，保留现有世界。首次开机配置只做一次；新世界初始化另见 [初始化手册](fresh-world-preview-bootstrap.manual.md)。正式多节点网络使用 [测试网运行手册](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md)。

已完成首次配置的主机，推荐使用下面的单命令入口。后续章节保留首次接管、接口合同和异常处理说明；不会再要求逐条手工执行部署步骤。

## 单命令更新

复制 [配置示例](../../../config/ecs-preview-deploy.example.json) 到本机私有配置目录，填入现有主机、服务路径、world/genesis、服务与 reader 公钥 pin、目标机 reader key 路径。`python_path` 仅用于已安装且受信任的目标机 crypto 模块；部署不运行 pip。配置不放密码或私钥内容；SSH 沿用 agent / config / 正常交互认证。可选 `SSHPASS` 使用已安装的 sshpass，密码只经环境交给 SSH，不写 profile、argv 或 GitHub 子进程；认证只尝试一次，不自动重试。脚本适用于本地 macOS/Linux 的 Python 3 与 OpenSSH，目标为本机已有的 Linux/systemd DevLocal Preview。

ECS Server Package 成功后执行一次：

```bash
python3 scripts/deploy-ecs-preview.py \
  --config ~/.config/oasis7/preview-deploy.json \
  --run <ECS-Server-Package-run-id>
```

入口自动核对 GitHub main ECS run 与同 source 的 main Rust workflow / required-gate，下载并验证最终包，通过**一个 SSH 进程**传包、预检、停止写入、一致备份、比对备份、切换两个服务、读取签名世界结果并观察完整 60 秒推进、复查公开 API，再调用同包 Skill 发布工具。配置变更、锁占用、低空间或身份校验失败即停止，不扩大公网白名单，不重建世界。各部署阶段只选同一完整 SHA，main 后续前进不重启整轮。

默认 `backup_mode=remote`：完整停机备份保存在目标机本轮 `ops_root` 部署目录（目录 0700，归档 0600），在原机运行 `tar --compare` 后才切换；本机 `local_backup_root` 保存备份身份和部署结果，不跨公网往返传输世界。预检在停服务前为完整备份、新版本和至少 528 MiB 余量保留空间；不足时应先在服务在线期间迁移已验证的旧备份或扩容。只有明确选择 `backup_mode=local` 才将完整备份传到本机并回传比对，该模式需承担两次整库网络传输的停机时间，不适合低带宽大世界。远端安装采用已在本地校验的归档流，避免同时占用压缩包和新版本的空间；仍保留至少 528 MiB 余量及原 release。新 chain 开始写入后失败或断连会隔离候选并保留历史，不能将旧程序直接指向可能变化的数据。成功输出 source、run、包/备份哈希和实际验收结果。该部署验收不代替后文的外部浏览器、真实登录及玩法验证。

Trace refs: O7-REQ-54717f75-7a50-43ae-a713-c5d89272b3e4；实现边界见[单命令部署设计](../../scripts/deployment/ecs-preview-deployment.design.md)。


## 1. 选定一个可用版本

使用同一完整 commit SHA 的已成功 CI 和 Linux Testnet 包。已有合格包就直接复用；main 后续前进本身不要求重打包、重测或重新追赶。遇到明确相关的安全、协议或存储兼容问题再处理。

当前单机云端更新优先使用 **ECS Server Package**，在 GitHub Actions 选择源码分支 / tag 后运行，或执行：

```bash
gh workflow run ecs-packages.yml --ref main
```

该入口固定本次选定 ref 的完整 SHA，Web 与原生在不同 runner 并行；编译 `oasis7_chain_runtime`、`oasis7_game_launcher`、`oasis7_viewer_live` 和用于可信冷入场 / 外部执行的 `oasis7_game_client`，不生成桌面安装器、client launcher、独立 web launcher 或维护工具。原生使用 `ecs` profile（release 优化级别 3、关闭跨 crate LTO、codegen-units 16），优先缩短构建时间；Cargo 并发默认 2。冷构建仍需编译核心运行时依赖，实际耗时以 run 为准；部署前重新核对真实世界推进 / 吞吐，不沿用其他 profile 的性能结论。完整发行包的 `packaging` 配置不变。候选分支仅恢复缓存，main 同版本工作流才写入独立原生缓存。

需要验证并发收益或回退单并发时，手动入口的 `native_build_jobs` 可选 `1` / `2`，CLI 追加 `-f native_build_jobs=1`。同一 SHA 分别运行两个值，并核对缓存恢复 key、Cargo fresh / dirty 数量和 runner 条件。`ecs-native-timings` artifact 包含 Cargo timing HTML、`ecs-time.txt`（耗时 / CPU / 最大单进程 RSS）和 `ecs-resources.json`（每秒采样的可用内存、swap、适用的 cgroup 内存 / OOM 计数）。并行总内存压力以可用内存与 cgroup 观测共同判断；缺失字段为未知，cgroup 历史峰值不可当作本次独占峰值。记录失败样本及条件差异，实测结论留在 PR / run。

成功后下载 `oasis7-ecs-linux-x64` artifact，里面包含版本化 `.tar.gz` 和外部 `.sha256`。先用 `sha256sum -c <包名>.sha256` 校验归档，再解压到新版本目录并执行 `sha256sum -c SHA256SUMS`。`BUILDINFO` 的 source 必须等于本次 run 的 SHA。包内 `bin/` 包含三个服务程序及同版本 `oasis7_game_client`（不会自动启动客户端或 Runtime），`web-dist/` 是可直接发布的完整 Viewer Web 根目录（包含可选 WASM），`manifest.json` 记录来源 / profile / 平台。

正式游戏 Skill 随 ECS 包固定到同一个 source SHA：组装直接读取该提交下 `skills/oasis7-play/` 的受跟踪 Markdown 对象，拒绝符号链接及未发布的相对引用，不吸收工作区改动或未跟踪文件。输出为 `web-dist/skills/oasis7-play/`（含引用及 `manifest.json`）、`web-dist/skills/index.html` 和随包 `deployment/ecs-skill-release.py`。Skill 清单记录 source、每个源文件的 SHA-256 / 字节长度，清单及发现页本身由整包 `SHA256SUMS` 覆盖。不得把旧 `site/skills/oasis7.md` 覆盖到此路径。

这是服务端目录包，不是旧 Testnet `.deb` 的替代安装器；旧安装脚本若只接收 `.deb`，不能直接传入此归档。配置三个服务程序及 Web 根目录路径后沿下文切换，`oasis7_game_client` 作为可信外部接入客户端使用。包不含世界、密钥、systemd unit 或自动初始化 / 公网开放操作，始终复用本机现有配置。包内 chain 的 `devlocal-participant prepare/finalize` 与 launcher 的 `--identity-snapshot-export` 是同版本内置命令；其他独立维护工具仍按需保留兼容版本。

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

### 公网 API 路由核对

沿用旧 Nginx 配置不代表白名单完整。首次接管及每次升级后，核对当前 Viewer 实际使用的公开接口与公网代理一致；接口以所部署提交的 [公开端点合同](../../../crates/oasis7_hosted_access/src/lib.rs)和 [HTTP 处理器](../../../crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/static_http.rs)为依据。端点合同用于核对，不是可直接全部放行的 Nginx 清单；各路径仍须按本机部署模式、认证和请求方法逐项配置。

`hosted_public_join` Viewer 会查询 `/api/public/player-session/admission`。在现有 HTTPS `server` 内添加以下精确路由，替换本机实际的 launcher 静态 HTTP 监听端口（示例 `4173`）；该端口不是 Runtime WebSocket 或 chain status 端口。已有相同路由时核对并更新，不能重复添加：

```nginx
location = /api/public/player-session/admission {
    if ($request_method !~ "^(GET|HEAD)$") { return 405; }
    proxy_pass http://127.0.0.1:4173;
    proxy_set_header Host $host;
    proxy_set_header Origin $http_origin;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For "";
    proxy_set_header X-Forwarded-Host "";
    proxy_set_header X-Forwarded-Proto https;
    proxy_cache off;
    proxy_read_timeout 60s;
}
```

该片段依赖现有站点的可信 Host、Origin 拒绝规则及安全响应头，不能作为独立安全配置使用。保留 `/api/` 的默认拒绝规则；不要用整个 `/api/` 或 `/api/public/` 的泛代理修复遗漏，不顺带开放 `issue`、`refresh` 或管理接口。准入查询返回席位、限流和 Runtime 探测状态，不签发凭据；查询会触发现有 presence 校准和过期席位清理。它也不代表世界 participant / owner / executor 授权已建立。

修改前备份实际站点配置，在同一维护锁内修改，执行 `nginx -t` 成功后才 reload；语法检查、reload 或公网验证失败时恢复备份并重新检查 / reload。不重启世界服务来修复代理路由。

在 ECS 上分别检查本机和公网，两个响应均须为 JSON，`ok=true`、`deployment_mode=hosted_public_join` 且 `admission.runtime_probe_status=ok`；动态计数和时间戳不要求逐字节相等：

```bash
curl --fail --silent --show-error --max-time 15 http://127.0.0.1:4173/api/public/player-session/admission
curl --fail --silent --show-error --max-time 15 https://<public-host>/api/public/player-session/admission
```

| 公网检查 | 预期 |
| --- | --- |
| GET 准入查询 | 200，JSON 字段符合上述要求 |
| HEAD 同一路径 | 200，空正文 |
| POST 同一路径 | 405 |
| GET 同一路径追加 `/unexpected` | 404 |
| GET `/api/public/player-session/issue` | 404，保留既有默认拒绝 |
| GET 准入查询，携带非本站 Origin | 403 |

本机 200 而公网返回 Nginx HTML 404 时，先检查请求命中的 `server` 和精确 location / include；不是世界准入策略缺失的证据。本机也失败时，再检查 launcher 路由、监听端口和日志。公网返回 JSON 但 Runtime 探测非 `ok` 时，排查后端与 Runtime 的连接，不通过扩大代理白名单或关闭认证解决。刷新实际页面，核对该请求不再失败；首页 200、Skill 校验和本机接口成功均不能替代公网 API 验证。

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

工具将精确路径 alias 到此 release 的文件，稳定 URL 使用 `no-cache`；仅 GET / HEAD。正式入口为 `/skills/oasis7-play/SKILL.md`，引用保留相对路径；`/skills/` 和 `/skills/oasis7-play/` 转至发现页，旧 `/skills/oasis7.md` 转至正式入口。它不开放任意文件目录或 API。写入使用原子替换，旧 include 保存在同目录 `.previous`；随后 `nginx -t`、reload 并从公网逐字节核对 Skill、全部引用、清单和发现页。reload 返回时旧 worker 可能仍在响应，因此激活验证最多等待 20 秒收敛；仅对版本字节不匹配或 HTTP 502 / 503 / 504 重试，每次重新验证整套文件，不能拼接多次局部成功。重定向、其他 HTTP 错误或 TLS / 连接错误立即失败，慢响应同样受总时限约束；独立 `verify` 命令仍单次严格校验。任一步失败都恢复旧 include、检查并 reload；回滚失败保留实际错误，不能宣称发布成功。失败期间不把旧 Skill 视为新游戏版本验收通过。

独立复核可运行只读命令：

```bash
python3 "$release/deployment/ecs-skill-release.py" verify \
  --release "$release" --expected-source "$source_sha" --origin https://<public-host>
```

同时检查原入口跳转、HEAD 空正文、POST 405、未知文件 404 以及主页发现头。回滚游戏时，用已校验的旧 release 重跑 `activate`，使页面和游戏一起回退；旧包若无本工具，使用原部署备份恢复对应静态发布配置并核对哈希，不能复用新版本 Skill。生成包和发布 Skill 不代表 Runtime 冷入场、游戏权限或真实试玩已通过。

### DevLocal 世界的参与者准入授权

官方客户端与外部 Agent 使用相同的身份、participant、owner 和 executor 接入协议。世界的准入策略由运营者预先建立；客户端不负责补造治理提案或 finality epoch。正式网络仍按治理初始化和治理授权流程执行，不能使用本节替代。

实际使用 `DevLocal` storage profile、经现有启动配置解析为 development runtime、非 hardened 的隔离执行节点，可在原 chain 启动参数后显式追加下列参数。该入口拒绝 network tier / manifest、formal、guarded、deployment authority、外部 bootstrap、历史 capture 及 gossip / replication 网络组合，不提供多节点本地信任根传播。

```text
--devlocal-participant-operator-public-key <operator-public-key-64hex>
--devlocal-participant-policy-file /etc/oasis7-preview/participant-policy.json
```

公钥 pin 是本机运营者的信任配置，不从客户端请求、策略文件或公网 discovery 自动接受。策略文件是 `SignedReadRequest<DevLocalParticipantPolicyAuthorizationV1>`；其 `request` 包含完整 `policy` 和 issuer 签名的 `manifest`。外层签名主体必须匹配 operator pin，首次策略使用 `participant_devlocal_policy_install_v1` domain，更新使用 `participant_devlocal_policy_update_v1`；内层 manifest 使用 `participant_migration_manifest_v1`。该文件保存签名材料，不保存私钥；必须是 canonical 绝对路径的普通文件、无符号链接、单链接、当前服务用户所有、权限 `0600`，且不超过 1 MiB。

签发使用同版本内置的只读 `devlocal-participant prepare`、真实 SQLite `--identity-snapshot-export` 和 `finalize --prepared`；完整配置、操作顺序和拒绝边界见 [DevLocal participant 政策签发与安装](devlocal-participant-policy.manual.md)。全程冻结世界与身份两侧写入，工具核对独立 pin、原始签名身份、完整 identity snapshot、真实 registry / legacy owner 前态和已有 owner 迁移签名；只输出签名授权材料，由下述原 chain 正常提交。不能把空映射、猜测的摘要或测试签名当成现有世界授权材料。保留原世界目录、身份、资产和绑定，不运行新世界初始化器。原 Preview 即使使用 loopback `--replication-network-listen` 也与本地 operator 授权冲突，须按隔离配置显式移除并核对，不能只追加政策参数。

旧 Preview 若在后续提交中丢失 resource manifest 的 `genesis_ref`，同时提供 `--devlocal-participant-origin-root /var/lib/oasis7-preview/<existing-root>`，其中 `world/` 必须就是当前 execution world。节点使用原 node key 派生的独立 service 公钥验证原 `initialization-receipt.json` 签名及 `genesis/initial-artifact.json` 的原 digest，核对当前 identity 文件，再与 operator 签署的 policy/world/genesis 和 issuer 前态证明绑定；仅补回缺失的原引用，不替换已有非空 genesis。该绑定与策略在同一 canonical candidate 中提交，失败不落盘；不能只凭普通 identity JSON 或 policy 自报生成创世身份。缺原签名收据、原 artifact 或原 node key 时保持阻塞。旧收据可用于这个有限身份核验，不代表缺失的初始 publication archive 已补齐，也不放宽正式恢复流程。

节点在原有世界写锁和正常 canonical commit 中安装策略。相同已提交授权在重启时幂等；冲突或过期前态失败，不直接改 snapshot / journal。后续读取和恢复需要继续提供相同的运营者 pin；首次安装后可移除策略文件及 origin-root 参数，保留历史验证所需的 pin。已绑定 genesis 的普通提交和重启持续保留原引用。local 与 governed 授权历史分别验证，不能仅切换 profile 把本地历史解释成正式网络历史；回退程序前也要确认其理解新增历史格式。

验收先核对签名策略读取中的授权来源与配置一致，再检查普通 participant 注册、owner 控制、executor 授权和重启后读取。配置世界准入不会自动打开身份服务的注册、owner / executor 开关，也不等于 Runtime profile、受限执行或真实试玩已通过。DevLocal 的提交结果不宣称正式网络 finality。

## 4. 马上验证

- 完成上述公网 API 路由核对，准入查询及拒绝边界检查通过；发现 API 404 时先定位来源并处理，不把它当作配置世界授权的理由。
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

显式 native executor 的 task context 使用 revision 2，并将 native 授权根写入公共上下文身份和持久原件；旧 provider revision 1 的编码兼容不代表旧程序能读取新的 native 历史。首次接受 native 操作前，保留一致的升级前数据备份和新旧 release。产生 native 历史后，只能切换到理解该格式的兼容程序；不将旧二进制直接指向当前世界，也不为回退而删除上下文或改写历史。恢复旧快照属于数据恢复，应单独确认其覆盖范围及之后的真实写入，不能把它算作普通版本回滚。

成功后再删除未使用的历史安装目录，只保留当前和上一个可用版本；混合版本运行时保留所有实际被进程、unit 或配置引用的版本。世界数据、密钥、备份、journal 和失败证据不属于旧版本清理范围。

部署记录放在实际任务 / Issue：`候选SHA、包SHA256、更新组件、前后版本、真实验证结果、失败/回滚结果`。手册保持通用步骤，不写入当轮 PID、密码、临时目录或审批流水。

### ECS workspace cache validation

Retain actual trusted main native producer and compatible consumer run/job links, requested/restored keys, cache-hit, Cargo unit Fresh/rebuilt, transfer durations, and raw/compressed bytes. Inspect cache reports in `ecs-native-timings`; a cache miss or rejection must still perform the full native build. Exercise added, removed and renamed source inputs for conservative invalidation. Limits are 6 GiB logical staging, 6 GiB remote main family and two compatible generations; inventory failures or exhausted limits skip saving.

Inspect the live quota, configured and effective budgets, capacity basis, complete usage across all refs/families, measured archive bytes and conservative outer upload bound. A live positive integer quota is converted conservatively to decimal bytes. Only recognized quota-access denial without rate limiting may use an explicitly configured, independently verified repository budget; the report must still identify the live quota as unknown. Other quota failures, an absent verified fallback or insufficient capacity skip saving without changing settings or deleting caches. See the [capacity policy and configuration source](../ci/ci-tiered-execution.design.md#ecs-workspace-artifact-cache). Concurrent writers and later dependency-cache post-job saves can change capacity after this point-in-time guard, so it does not reserve space atomically.

Candidate native jobs run archive/capacity preview with `save=false`. Confirm actual trusted main uploads through the separate `observe-save` report and a subsequent compatible consumer; preview, a successful action or local tests alone do not establish cross-run reuse. Record pending remote evidence explicitly.

### 备份轮换与清理

正式备份、增量恢复链、旧 release 与安装缓存分别处理。远端优先保留最近一次已验证的部署前备份和仍被引用的回滚版本；旧备份连同基线、增量及恢复记录完整迁移到私有备份目录，逐文件 SHA256 比对、落盘并复读验证后，才能释放对应远端文件。恢复链依赖未核实前不拆删增量或旧世界副本。正在运行、配置或 symlink 引用的版本保留；无引用的安装缓存和已结束用途的诊断临时输出才可清理。部署入口不自动删除身份未知的历史备份，也不以省空间为由省略本轮一致备份。
