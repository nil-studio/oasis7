# 单机 ECS Preview 自动更新

- 状态：active
- owner_role：blockchain_ops_engineer
- 适用：已有世界、身份、systemd 和 Nginx 的 Linux x64 单机 Preview。
- 合同：[来源、安全与自动部署](../../engineering/workflow/package-source-security.md#ecs-preview-automatic-deployment)。
- Trace refs: O7-REQ-5ef8b941-eaa3-4975-a020-59b4d74fba89。

这是单机 Preview 日常部署的唯一操作入口：触发可信 ECS 发包并启用 Preview 发布，由 CI 上传 OSS、主机协调器完成下载、校验、组件切换和验收。首次世界初始化见[初始化手册](fresh-world-preview-bootstrap.manual.md)，正式多节点部署见[测试网运行手册](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md)。代码上线不等于云资源已配置或真实更新已验收。现有部署暂停或环境限制仍适用。

## 一次配置云端

创建专用 OSS bucket/prefix，默认私有、禁止匿名读写。不要把真实账号 ID、私有 bucket、角色 ARN 或凭据提交仓库。公网 CI 上传 endpoint 必须符合 GitHub runner 网络能力；ECS 同地域优先使用对应 OSS 内网 HTTPS endpoint。

CI 使用 [阿里云 OIDC Action](https://github.com/aliyun/configure-aliyun-credentials-action)换取短期 STS。RAM trust 必须固定 GitHub issuer、audience、仓库 `nil-studio/oasis7`、main ref 和 Preview 发布入口；GitHub Environment 同时限制 main。不可把所有分支、PR 或任意 workflow 纳入角色。角色额外允许专用 bucket 的 `oss:GetBucketVersioning` 核对其从未启用版本化（Enabled/Suspended 均拒绝，因为该模式的禁止覆盖头不生效）；仅允许所选 prefix 下 release 对象的创建/读取和 `channels/preview.json` 的读取/更新；不要授予 DeleteObject、全 bucket 管理或 ECS 权限。该发布工作流是频道的唯一写者，其他写者不得绕过同一串行域；OSS 普通 PutObject 不提供本工具所需的 If-Match CAS。

ECS 挂载只读实例 RAM 角色，仅 `GetObject` 所选 release 前缀与频道，无发布写权限、不配置长期 access key。实例元数据使用 IMDSv2；更新器不需要 ListObjects。云侧最小策略和 trust 必须经具备权限能力的独立审阅，并分别验证允许/拒绝路径后启用。

GitHub Preview environment / variables 的准确名称和动作参数以 [ECS 工作流](../../../.github/workflows/ecs-packages.yml)的 `publish-preview` job 为准，配置 OSS endpoint、bucket、prefix、OIDC provider/role、program/persistence/identity 三个兼容契约。契约名只描述版本条件，不授予主机自动升级权限。

## 一次接管已有主机

1. 核对[主机接管预检](#主机接管预检)、原世界与读身份、服务参数和当前组件。先备份现有配置，保留已校验的当前 ECS release。安装器不初始化世界，也不自动改写已有游戏 unit。
2. 为 chain、player、Web 和 Skill 分别建立独立的绝对 symlink；unit 的 ExecStart / Web 根目录分别消费相应路径。保持参数、数据/身份目录和私有环境不变。不要让组件共用一个可切换的 `current` 祖先路径。
3. Nginx Skill include 及 HTTPS discovery header 按[同版本 Skill 发布](#nginx-与-skill-首次接管)完成一次 bootstrap；更新器不扩大 API allowlist。
4. 安装专用 Python 环境及固定 OSS SDK，更新器源码、配置、兼容许可、验证入口及所有父目录由 root 所有，不可被其他用户写入。不要从远端 manifest 下载执行脚本。

```bash
sudo python3 -m venv /opt/oasis7-preview-updater-venv
sudo /opt/oasis7-preview-updater-venv/bin/pip install oss2==2.19.1
sudo /opt/oasis7-preview-updater-venv/bin/python scripts/ecs-preview-install.py \
  --config /etc/oasis7-preview/updater.json
```

安装前将候选受信 updater 工具放入固定 root 所有目录，并在配置中设置 `verifier_command` 为该目录的 `ecs-preview-verify.py --config <本配置> --device <已有设备配置>`（通过专用 Python 调用）。安装后可改为安装器打印的版本目录；不要指向会被游戏 release 切换的脚本。签名读设备配置沿用原 world/genesis/service key/agent 身份和 signer，不新建身份。内置验证器检查实际进程、公开准入 API、Web 全部资源、Skill 和四次签名 canonical tick；对 chain 写入兼容性返回 unknown，chain 更新失败必须按恢复流程核对，不自动回退未知写入。

配置结构见 [配置示例](../../../scripts/ecs-preview-config.example.json)。所有示例路径、身份和契约均须替换为本机实际值；配置不含密钥，签名读取使用现有本机身份。`verifier_command` 是 root 配置的固定命令，接收 stdin JSON、输出严格健康证据；接口见下节，不能从频道接收命令。安装 preflight 失败时不启用 timer、不初始化或替换世界。首次接管旧手工布局需先在维护锁内完成独立路径适配；不能让安装器猜测旧机器布局。

`compatibility_file` 保存管理员独立审核的精确 from/to 组件 source、三个契约和非空依据；例如批准的版本差异/持久格式及身份格式兼容测试、允许的写入与回退条件。只写 `safe=true`、只复制远端 contract 或使用通配符均不构成许可。`rollback_allowed` 只允许已知兼容的程序回退；chain 已不兼容写入、迁移或写入未知时仍禁止普通回退。

对只读 Viewer 的日常兼容升级，可在首次配置的兼容文件中加入 `viewer_policy`，无需每个新 SHA 修改主机许可：

```json
{
  "schema_version": 1,
  "permissions": [],
  "viewer_policy": {
    "schema_version": 1,
    "allowed_inputs": ["crates/pixel_world_bridge/src/render_hotspot_core.rs"],
    "evidence": "替换为独立审核结论及只读Viewer配置依据",
    "read_only_viewer": true
  }
}
```

`allowed_inputs` 必须替换为本机实际已审阅的精确文件路径，不是通配符或默认信任所有 viewer 代码。先确认既有 Viewer 连接外部 chain、不直接持有/迁移世界与身份存储，管理员明确接受这些文件后续显示代码的变更风险。主机重建包内完整 Git commit/tree 证明，核对全部受跟踪输入；新增/删除输入、其他文件变化以及 chain/launcher/game_client 任一程序字节变化均拒绝，只允许 viewer native 程序改变。完整输入树同时包含文档、CI、Web/Skill，修改它们也不会自然获准，必须在适用独立授权后更新；此策略不自动忽略所谓非代码文件。它提供一个真实可用、范围保守的 player-only 自动兼容通道，不保证所有程序更新可自动批准。初次接管旧包若没有输入证明，可先用一次精确许可升级至有证明的基线；后续符合已审核范围的升级无需逐次 SSH。超出范围的变化须补独立兼容依据，timer 保持旧服务。

确认本机配置及验证入口已完成后启用：

```bash
sudo systemctl enable --now oasis7-preview-update.timer
systemctl list-timers oasis7-preview-update.timer
```

timer 默认每次结束后 60 秒再运行独立 oneshot；它不随 player 重启，不编译源码。

## 健康验证接口

固定验证命令接收目标组件和原稳定 `identity` 的 JSON 请求。每次请求带新 nonce，返回必须原样绑定本次 nonce，并使用当前机器真实观察，不能复用旧证据。组件条目包括 source、绝对实际路径和文件/确定性目录树摘要；实际 launcher/Viewer/chain 进程字节必须与对应目标一致。目录摘要按排序的相对路径、NUL、文件 SHA256、换行计算。

返回的 `identity` 必须与配置中的 world ID、genesis、服务公钥和签名读身份完全一致；`components` 必须与请求一致；`checks` 的 `api/web/skill/signed_world` 均为真实成功。返回 `samples` 至少包含基线和三次不同的有效世界推进，每项有严格递增 `timestamp`、不回退的 `world_tick`、原 `identity` 和 `signed_world:true`，覆盖完整 60 秒；需来自持续真实签名世界观察，不能用三次同一初始快照或定时 sleep 伪造。协调者同时校验调用实际墙钟窗口。`chain_write_status` 明确为 `unchanged`、`compatible`、`unknown` 或 `incompatible`；只有受信读取依据才能确定兼容写入，未知保持未知。

公共 API 与[代理接管合同](#公网-api-路由核对)一致，Web 与 Skill 须与 player 同版本；HTTP 200、unit active 和 CI cold-entry 均不能单独代签部署。具体主机的签名世界读取、鉴权和私有参数通过一次性验证配置提供，不写入公共日志。

## 日常发包与观察

在 GitHub `ECS Server Package` 入口选择可信 main，明确打开 `publish_preview`。保留原 `native_build_jobs` 入口；`measure_builds` 仅测量，不能推进频道。未显式启用发布时仍只输出 Actions artifact。发布不新增每次 main 合入重构建触发。

```bash
gh workflow run ecs-packages.yml --ref main -f publish_preview=true
```

CI 先验证已有归档、上传不可变 release 对象、回读确认，最后更新频道。主机预检并安全安装到独立目录，再在统一维护锁中切换选定组件。player-only 不改变 chain 进程/参数/版本，chain 更新按先停 player、更新 chain、再起 player 的顺序。未知兼容条件保持旧服务并报告原因。

本机管理命令使用安装器打印的固定 updater 目录和 Python，以下 `<updater>` 替换成该目录：

```bash
sudo /opt/oasis7-preview-updater-venv/bin/python <updater>/ecs-preview-update.py --config /etc/oasis7-preview/updater.json status
sudo journalctl -u oasis7-preview-update.service --since today
sudo /opt/oasis7-preview-updater-venv/bin/python <updater>/ecs-preview-update.py --config /etc/oasis7-preview/updater.json pause
sudo /opt/oasis7-preview-updater-venv/bin/python <updater>/ecs-preview-update.py --config /etc/oasis7-preview/updater.json resume
sudo /opt/oasis7-preview-updater-venv/bin/python <updater>/ecs-preview-update.py --config /etc/oasis7-preview/updater.json retry
```

状态分别记录观察目标、每组件实际版本、上一可用版本、当前阶段、最近失败和回退结果；观察到频道不表示部署成功。同一成功意图不重启；失败切换不被 timer 反复尝试。暂停不停止游戏服务。retry 不能绕过未知写入/中断恢复的人工核对。

## 回退、恢复与清理

在同一可信入口，以新的 dispatch 选择保留的 release，明确 `rollback_preview`；`retained_release` 使用此前发布的完整 release ID。复用已合格归档，不覆盖旧对象或重复构建。普通前进发布同时核对意图序号和源码祖先关系，旧 run 迟到或重跑不能让频道倒退；显式回退仍需主机兼容许可。

```bash
gh workflow run ecs-packages.yml --ref main \
  -f publish_preview=true -f retained_release=<source-sha/run-id-attempt> -f rollback_preview=true
```

自动验证失败时，只对明确允许兼容回退的组件恢复前一版本及其 Web/Skill并再次验证；回退失败保存实际状态和原因。中断发生在切换/验证/恢复阶段时先读状态、实际链接/进程及日志，停止自动破坏性操作，按现有恢复流程核对，不直接编辑状态冒充成功。chain 写入未知、不兼容或已迁移时不让旧程序读当前数据，不重置世界，不回放旧快照伪装普通回退。

手工维护先暂停 timer，在相同 `lock_path` 上使用 `flock`，全过程涵盖组件、Web/Skill、验收和恢复。协调者调用 Skill 时传递已持锁 fd；单独运行 Skill 工具自行获取同一锁。不要持有外层锁又独立执行一个不传 fd 的取锁工具，也不要通过释放锁产生维护窗口。

保守保留当前、上一可用、失败现场和所有被进程/unit/配置引用的版本；无法证明无引用时不删。清理仅限安装的程序版本，世界、身份、密钥、备份与 journal 永不属于版本清理。

真实验收需在授权的一台现有 Preview ECS 记录一次真实 CI→OSS→自动更新：source/run/intent、归档 SHA256、各组件前后版本、原世界身份、公开 API/Web/Skill、连续世界推进与最终状态。破坏性故障在隔离环境测试。缺 bucket/role/机器配置或授权时保留待验收，不能用本地测试或发包成功代签。


安装器中断后若只写入部分 updater unit，先确认 timer 未运行并暂停它，在相同维护锁内核对两个 unit 的内容/来源。只移除本次未完成安装生成的 `oasis7-preview-update.service` 和 `.timer`，执行 daemon-reload，再重跑安装；不删除游戏 unit、世界或 release。已有安装需要升级时先停 timer并保留旧 updater 版本与单位配置，同一锁内替换确认的 updater unit，然后安装新版本，失败恢复原单位配置。安装器不会覆盖未知已有 unit。


## 主机接管预检

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

接管时暂停并停用原有日常部署 cron/timer、发布 webhook 或维护调度器，确认它们不再运行；只处理经核对属于旧部署流程的任务，不停用游戏 unit。已有人持有维护锁时先完成交接，不强制抢锁。新更新器安装并通过首次验收后，以其 timer 作为唯一日常更新调度器。旧工具/配置若仍被其他环境使用，保留其实际引用，不按名字批量删除主机文件。

## 公网 API 路由核对

沿用旧 Nginx 配置不代表白名单完整。首次接管时核对当前 Viewer 实际使用的公开接口与公网代理一致；接口以所部署提交的 [公开端点合同](../../../crates/oasis7_hosted_access/src/lib.rs)和 [HTTP 处理器](../../../crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/static_http.rs)为依据。端点合同用于核对，不是可直接全部放行的 Nginx 清单；各路径仍须按本机部署模式、认证和请求方法逐项配置。

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

首次接管在 ECS 上分别检查本机和公网，两个响应均须为 JSON，`ok=true`、`deployment_mode=hosted_public_join` 且 `admission.runtime_probe_status=ok`；动态计数和时间戳不要求逐字节相等：

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

## Nginx 与 Skill 首次接管

本节只配置一次 Nginx 托管 include 与发现头。之后 player 升级、Skill 发布及回退均由更新协调器在同一维护锁中完成，操作者不再逐次运行独立 activate 命令。不要保留另一提交的手工静态 Skill，也不要单独从 main 获取文档覆盖运行版本。

首次接管 Nginx 时，在对应 HTTPS `server` 内加入一个明确的托管 include；路径可按本机约定选择，例如：

```nginx
include /etc/nginx/snippets/oasis7-skills.conf;
```

首次创建该普通文件为空（不要清空已有配置），移除先前手工维护的 `/skills/` 精确 location，避免重复路由。保留现有 Origin / 安全响应头 / API allowlist。在同一 `server` 的现有 `add_header` 层级增加发现头，避免在子 location 覆盖已有安全头：

```nginx
add_header Link '</skills/oasis7-play/SKILL.md>; rel="describedby"; type="text/markdown"' always;
```

先备份站点配置，执行 `nginx -t` 和 reload，失败恢复原配置；此 bootstrap 只做一次。后续发布工具只修改该 include，不改主站点、API、服务环境、权限开关或世界。HTTPS `server` 的 `/skills/` 路由只归此 include 管理；不能被另一 location 或其他 include 遮蔽。


更新器在切换前先验证当前版本，所以不能把空 include 作为已完成接管。完成更新器安装、尚未启用 timer 时，仅对**原来正在运行且已核验的当前 release** 初始化一次 Skill alias；不选新包、不切换组件、不重启游戏。以下命令中的 `<updater>` 是安装器打印的受信工具目录；其余占位符取本机已核验值：

```bash
sudo bash -euc '
  exec 9>>"$1"
  flock -n 9
  exec "$2" "$3/package-ecs-skills.py" activate \
    --release "$4" --expected-source "$5" --origin "$6" \
    --player-service "$7" --nginx-include "$8" \
    --lock-file "$1" --lock-fd 9
' bootstrap /run/lock/oasis7-preview-deploy.lock \
  /opt/oasis7-preview-updater-venv/bin/python <updater> \
  <existing-verified-release> <full-current-source-sha> https://<public-host> \
  <player-service> /etc/nginx/snippets/oasis7-skills.conf
```

锁路径与更新器配置一致；shell 持有同一独占锁并将 fd 9 传入工具，不嵌套独立取锁。命令应直接运行，不能再套一个已持锁但未传 fd 的父进程。工具核对正在运行的 launcher 字节与此 release 一致，拒绝用待升级版本代替当前版本。初始化及公网校验成功后，才能启用 timer；失败时恢复首次接管前保存的站点配置/include，检查并 reload，保留原游戏版本且不启用 timer。这是一次性接管步骤，后续升级和回退不再单独调用 activate。

工具将精确路径 alias 到此 release 的文件，稳定 URL 使用 `no-cache`；仅 GET / HEAD。正式入口为 `/skills/oasis7-play/SKILL.md`，引用保留相对路径；`/skills/` 和 `/skills/oasis7-play/` 转至发现页，旧 `/skills/oasis7.md` 转至正式入口。它不开放任意文件目录或 API。写入使用原子替换，旧 include 保存在同目录 `.previous`；随后 `nginx -t`、reload 并从公网逐字节核对 Skill、全部引用、清单和发现页。reload 返回时旧 worker 可能仍在响应，因此激活验证最多等待 20 秒收敛；仅对版本字节不匹配或 HTTP 502 / 503 / 504 重试，每次重新验证整套文件，不能拼接多次局部成功。重定向、其他 HTTP 错误或 TLS / 连接错误立即失败，慢响应同样受总时限约束；独立 `verify` 命令仍单次严格校验。任一步失败都恢复旧 include、检查并 reload；回滚失败保留实际错误，不能宣称发布成功。失败期间不把旧 Skill 视为新游戏版本验收通过。

后续每次自动升级由固定验证器检查公开准入接口、运行程序、Web/Skill 和真实签名世界推进。代理策略变更仍在相同维护锁内独立验证，不借部署扩大 API 白名单。

Skill 文件由同 source 的 Git 对象组装到 `web-dist/skills/`，清单、发现页和文件哈希受整包 `SHA256SUMS` 约束。协调器调用受信安装的 Skill 工具并继承已持锁 fd，原子更新精确 GET/HEAD alias、检查 Nginx、reload 并核对公网文件，失败恢复原 include。正式入口为 `/skills/oasis7-play/SKILL.md`，引用保持相对路径；旧 URL 的跳转只服务现有玩家链接，不构成旧部署入口。独立只读诊断可运行已安装工具的 `verify --release <已核验release> --expected-source <完整SHA> --origin <公开HTTPS源>`，不能用其成功代替完整部署验收。

## 授权、身份与恢复边界

DevLocal participant 的 prepare、身份快照、finalize、原 genesis 绑定、安装及历史验证统一见 [DevLocal participant 政策签发与安装](devlocal-participant-policy.manual.md)。自动更新不执行这些授权或世界初始化操作，也不开放注册、owner 或 executor 开关。

显式 native executor task context 使用 revision 2，并将 native 授权根写入公共上下文身份及持久原件；旧 provider revision 1 的编码兼容不表示旧程序可读取新的 native 历史。首次接受 native 操作前保留一致的数据备份及适用 release；产生历史后，只能切换到理解该格式的程序，不能删除上下文、改写历史或恢复旧快照伪装普通回退。

验收同时核对原稳定身份与配置。trace session ID 默认每次启动变化，只核对它属于当前 launcher PID/启动时间或与显式配置 trim 后一致，不要求重启前后相等。图形交互、账号或 Agent 功能按对应功能手册验收，部署健康检查不能代签真实试玩。

## 打包与缓存诊断

ECS 包保留同 source 的四个运行程序、Web、Skill、BUILDINFO、manifest、兼容输入证明和完整校验和，不包含世界、身份密钥或自动初始化操作。Actions artifact 作为构建诊断及来源证据保留，不再作为日常人工转包入口。`native_build_jobs=1/2` 与受控测量仅用于同 SHA 构建诊断，测量任务不推进频道。`ecs-native-timings` 记录 Cargo timing、耗时、CPU、RSS、主机/cgroup 内存及缓存观测；缺失指标保持未知，不能把最大单进程 RSS 当总内存峰值。

Retain actual trusted main native producer and compatible consumer run/job links, requested/restored keys, cache-hit, Cargo unit Fresh/rebuilt, transfer durations, and raw/compressed bytes. Inspect cache reports in `ecs-native-timings`; a cache miss or rejection must still perform the full native build. Exercise added, removed and renamed source inputs for conservative invalidation. Limits are 6 GiB logical staging, 6 GiB remote main family and two compatible generations; inventory failures or exhausted limits skip saving.

Inspect the live quota, configured and effective budgets, capacity basis, complete usage across all refs/families, measured archive bytes and conservative outer upload bound. A live positive integer quota is converted conservatively to decimal bytes. Only recognized quota-access denial without rate limiting may use an explicitly configured, independently verified repository budget; the report must still identify the live quota as unknown. Other quota failures, an absent verified fallback or insufficient capacity skip saving without changing settings or deleting caches. See the [capacity policy and configuration source](../ci/ci-tiered-execution.design.md#ecs-workspace-artifact-cache). Concurrent writers and later dependency-cache post-job saves can change capacity after this point-in-time guard, so it does not reserve space atomically.

Candidate native jobs run archive/capacity preview with `save=false`. Confirm actual trusted main uploads through the separate `observe-save` report and a subsequent compatible consumer; preview, a successful action or local tests alone do not establish cross-run reuse. Record pending remote evidence explicitly.
