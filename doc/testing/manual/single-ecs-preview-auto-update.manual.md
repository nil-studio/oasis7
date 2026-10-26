# 单机 ECS Preview 自动更新

- 状态：active
- owner_role：blockchain_ops_engineer
- 适用：已有世界、身份、systemd 和 Nginx 的 Linux x64 单机 Preview。
- 合同：[来源、安全与自动部署](../../engineering/workflow/package-source-security.md#ecs-preview-automatic-deployment)。
- Trace refs: O7-REQ-5ef8b941-eaa3-4975-a020-59b4d74fba89。

本流程替代[原单机手册](single-ecs-preview-deploy.manual.md)的日常人工下载、转传、切换和逐次 SSH；首次机器接管、世界初始化、授权、迁移与正式多节点部署仍由对应流程负责。代码上线不等于云资源已配置或真实更新已验收。现有部署暂停或环境限制仍适用。

## 一次配置云端

创建专用 OSS bucket/prefix，默认私有、禁止匿名读写。不要把真实账号 ID、私有 bucket、角色 ARN 或凭据提交仓库。公网 CI 上传 endpoint 必须符合 GitHub runner 网络能力；ECS 同地域优先使用对应 OSS 内网 HTTPS endpoint。

CI 使用 [阿里云 OIDC Action](https://github.com/aliyun/configure-aliyun-credentials-action)换取短期 STS。RAM trust 必须固定 GitHub issuer、audience、仓库 `nil-studio/oasis7`、main ref 和 Preview 发布入口；GitHub Environment 同时限制 main。不可把所有分支、PR 或任意 workflow 纳入角色。角色额外允许专用 bucket 的 `oss:GetBucketVersioning` 核对其从未启用版本化（Enabled/Suspended 均拒绝，因为该模式的禁止覆盖头不生效）；仅允许所选 prefix 下 release 对象的创建/读取和 `channels/preview.json` 的读取/更新；不要授予 DeleteObject、全 bucket 管理或 ECS 权限。该发布工作流是频道的唯一写者，其他写者不得绕过同一串行域；OSS 普通 PutObject 不提供本工具所需的 If-Match CAS。

ECS 挂载只读实例 RAM 角色，仅 `GetObject` 所选 release 前缀与频道，无发布写权限、不配置长期 access key。实例元数据使用 IMDSv2；更新器不需要 ListObjects。云侧最小策略和 trust 必须经具备权限能力的独立审阅，并分别验证允许/拒绝路径后启用。

GitHub Preview environment / variables 的准确名称和动作参数以 [ECS 工作流](../../../.github/workflows/ecs-packages.yml)的 `publish-preview` job 为准，配置 OSS endpoint、bucket、prefix、OIDC provider/role、program/persistence/identity 三个兼容契约。契约名只描述版本条件，不授予主机自动升级权限。

## 一次接管已有主机

1. 核对[原手册预检](single-ecs-preview-deploy.manual.md#2-部署前看四件事)、原世界与读身份、服务参数和当前组件。先备份现有配置，保留已校验的当前 ECS release。安装器不初始化世界，也不自动改写已有游戏 unit。
2. 为 chain、player、Web 和 Skill 分别建立独立的绝对 symlink；unit 的 ExecStart / Web 根目录分别消费相应路径。保持参数、数据/身份目录和私有环境不变。不要让组件共用一个可切换的 `current` 祖先路径。
3. Nginx Skill include 及 HTTPS discovery header 按[同版本 Skill 发布](single-ecs-preview-deploy.manual.md#同版本发布游戏-skill)完成一次 bootstrap；更新器不扩大 API allowlist。
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
    "allowed_inputs": ["crates/oasis7_viewer_runtime/src/viewer/render_hotspot_core.rs"],
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

公共 API 与[原手册](single-ecs-preview-deploy.manual.md#公网-api-路由核对)一致，Web 与 Skill 须与 player 同版本；HTTP 200、unit active 和 CI cold-entry 均不能单独代签部署。具体主机的签名世界读取、鉴权和私有参数通过一次性验证配置提供，不写入公共日志。

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
