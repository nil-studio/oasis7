# DevLocal participant 政策签发与安装

适用：已有、可验证原始身份的隔离 DevLocal 世界，需要支持从新身份账号开始的 participant / owner / executor 接入。正式网络仍走治理授权；本流程不生成治理提案、finality 证明或 Runtime 验收。官方客户端和外部 Agent 使用同一接入协议。

工具随相同源码版本的 `oasis7_chain_runtime` 和 `oasis7_game_launcher` 提供。发行包存在这些命令不表示某台 ECS 已配置或验收；[ECS 部署手册](single-ecs-preview-deploy.manual.md)负责切换、代理与上线验收。冷入场实际二进制集成见 [smoke 操作说明](../../../scripts/package-ecs-cold-entry-smoke.md)，该 fixture 只能用于新隔离目录，不能对已有世界运行初始化器。

## 1. 固定信任配置并冻结两侧写入

持有部署维护锁，先停 launcher / Viewer、身份维护任务和其他身份写入者，再停 chain 及其他世界写入者，核实进程退出，保留世界、identity SQLite（含 WAL）、原 node key、unit 和环境配置的可恢复备份。直到签发及 canonical 安装完成，不恢复身份写入。chain 工具另持有与 live host 相同的世界 writer lock；它不能替代身份写入者停写，也不能证明其他进程已遵守部署锁。

使用实际运行用户创建私密工作目录 `0700`；输入文件和私钥须普通、单链接、无符号链接、属当前有效 UID、`0600`，路径 canonical 且绝对。输出文件必须尚不存在；不要覆盖原证据。不同服务属主时通过受控私密交接复制必要材料，按消费方有效 UID 设置属主 / 权限，保留原件。

从可信部署记录独立取得原 world ID、genesis、node ID / node key 目录和 service 公钥，以及运营者 operator pin。issuer 公钥必须是身份服务实际 issuer 对应公钥。不能从待签 policy、公网 discovery 或客户端输入自动接受这些 pin；运营者与 issuer 的私钥仅通过私密文件输入，不能写入 argv、聊天或日志。

## 2. 准备原世界前态

创建 `operator-config.json`，所有尖括号占位符换成可信现值。`identity_namespace.instance_epoch` 使用十进制字符串；身份导出 request 的 RP epoch 使用 JSON 整数。

```json
{
  "world": {"world_id": "<existing world ID>", "genesis_digest": "<original digest>"},
  "world_directory": "/absolute/existing-root/world",
  "origin_root": "/absolute/existing-root",
  "node_id": "<original node ID>",
  "node_key_directory": "/absolute/existing-node-config",
  "service_public_key": "<independently pinned 64hex>",
  "operator_public_key": "<independently pinned 64hex>",
  "identity_namespace": {
    "service_id": "<deployed identity service ID>",
    "environment_id": "<deployed environment ID>",
    "instance_epoch": "1"
  },
  "world_identity_scope": "<deployed exact identity world scope>"
}
```

`origin_root/world` 必须就是 `world_directory`。工具使用原 node key 派生的 service key 检查 pin，再验证原 `initialization-receipt.json`、`genesis/initial-artifact.json` 及当前世界身份。缺任一原始依据时停止；不能重签 genesis 或仅凭普通 identity JSON 恢复信任。

```bash
<release>/bin/oasis7_chain_runtime devlocal-participant prepare \
  --config /absolute/private/operator-config.json \
  --output /absolute/private/prepared.json
```

工具在 writer lock 下把原世界复制到私密 scratch，以正常 restore 读取，再脱离持久化附件计算真实 world / branch / reorg、registry / legacy owner / snapshot 摘要及 Agent / task / goal / active grant 计数。恢复产生的 cognition / audit 写入仅在 scratch；不会保存原世界。旧 Preview 仅缺 `genesis_ref` 时，使用已验证的原始身份在内存候选补回引用；不会替换已有非空 genesis。

## 3. 导出真实身份快照，收集 owner 迁移签名

仍保持两侧停写，按 [身份完整快照导出](identity-snapshot-export.manual.md) 从真实权威 SQLite 导出：

```bash
<release>/bin/oasis7_game_launcher --identity-snapshot-export \
  /absolute/private/identity.sqlite \
  /absolute/private/snapshot-request.json \
  /absolute/private/identity-snapshot.json
```

导出固定 namespace / RP，真实读取所有账号及绑定。`source_snapshot_epoch` 是实际 identity instance epoch；同 epoch 新快照以完整内容 watermark 区分。Tablestore 尚无对应一致导出能力时明确不支持；不能以空数组代替真实全量快照。

按实际当前 owner 收集 `SignedReadRequest<ParticipantMigrationBindingV1>`，存为 `entries.json` 数组。每项绑定实际 registration、Agent、owner binding / generation，由**原 owner 私钥**使用 `participant_migration_owner_v1` domain 签名；工具不持有 owner 私钥，也不替 owner 签名。每项账号和世界 scope / player binding 必须存在于完整 identity snapshot，并与原世界身份和所有权一致。`[]` 仅用于经真实前态证明无需迁移的世界，不能为了绕过缺 owner 签名而清空已有映射、资产、Agent、registry 或 journal。

## 4. 签发并内存验证

创建 `policy.json`，使用实际参数：

```json
{
  "api_rev": 1,
  "world_id": "<same world ID>",
  "genesis_digest": "<same original digest>",
  "identity_namespace": {
    "service_id": "<same identity service ID>",
    "environment_id": "<same environment ID>",
    "instance_epoch": "1"
  },
  "issuer_public_key": "<actual issuer 64hex>",
  "generation": 1,
  "max_grant_ticks": 16,
  "max_participants": 10,
  "registry_complete": true,
  "enabled": true
}
```

容量和有效 tick 是运营政策，不照抄示例。首次 `generation=1`；更新采用当前策略的合法下一代，保持 namespace / issuer / 前代等既有约束，不重新迁移已有账号。issuer 与 operator key 文件分别是原始 32 字节 seed，禁止把十六进制文本冒充 raw32。

```bash
<release>/bin/oasis7_chain_runtime devlocal-participant finalize \
  --config /absolute/private/operator-config.json \
  --prepared /absolute/private/prepared.json \
  --identity-snapshot /absolute/private/identity-snapshot.json \
  --policy /absolute/private/policy.json \
  --entries /absolute/private/entries.json \
  --issuer-key /absolute/private/issuer.raw32 \
  --operator-key /absolute/private/operator.raw32 \
  --output /absolute/private/participant-authorization.json
```

`--prepared` 是必填前态，finalize 再次在世界锁下读取并逐项比较；前态变化则重新 prepare、导出和收集签名，不只改摘要。issuer 对 manifest 使用 `participant_migration_manifest_v1` domain；operator 外层首次使用 `participant_devlocal_policy_install_v1`，更新使用 `participant_devlocal_policy_update_v1`。工具核对真实 snapshot 完整性、namespace、issuer/operator 公钥与 entries 后，在脱离持久化的内存候选调用正式 `install_devlocal_participant_policy` 校验；失败不发布结果、不写世界。

成功输出 `world_written=false`、`canonical_install_required=true`，只表示签名材料可交给正式安装，不代表策略已上链、冷入场通过或 Runtime 可执行。授权输出超过 live reader 的 1 MiB 上限会拒绝。

## 5. 由原 chain 正常提交安装

在原 chain 参数中保留原 node / world / keys / storage 路径，并加入：

```text
--devlocal-participant-operator-public-key <independent operator 64hex>
--devlocal-participant-policy-file /absolute/private/participant-authorization.json
--devlocal-participant-origin-root /absolute/existing-root
```

必须为实际 DevLocal、解析为 development runtime、非 hardened 的隔离执行节点，不能携带 formal / guarded / deployment authority / network tier、外部 bootstrap、历史 capture、gossip 或 replication 网络配置。**即使原配置为 `--replication-network-listen /ip4/127.0.0.1/tcp/0` 也会冲突，须显式移除此 listen 参数及其他 replication peers / fetch requester 配置**；不能修改校验放行组合，也不能改成公网 listen。保留其他必要原始参数，并在重启前核对解析结果与隔离用途。

节点仅在原世界锁和正常 canonical candidate 中安装；失败保持阻塞，不手改 snapshot / journal / records。启动后以真实签名读取核对同 world / genesis、策略 generation、issuer、授权来源 `dev_local_operator` 和持续世界推进。已提交相同授权重启幂等；首次提交确认后可移除 policy-file / origin-root，继续保留 operator pin 以验证历史。更新同样先冻结、读取和签发；回退旧程序前确认理解历史格式，二进制回退不撤销世界提交。

## 6. 身份服务绑定、精确代理与接入验收

完整冷入场还需要 chain 的正式私有目标 / task-context 存储，不能只开启 identity 的三个接入开关。在 chain 服务环境同时配置 `OASIS7_PRIVATE_GOAL_DIR` 和 `OASIS7_PRIVATE_GOAL_KEY_FILE`：目录为独立 `0700` 私密命名空间，位于公开 world / records / CAS 根之外；密钥是属运行用户、无符号链接、单链接、`0600` 的精确 32 字节随机原始文件，不能用 64hex 文本，也不能复用 issuer / operator seed。保持实际监听为 loopback，通过已鉴权的 owner / executor gateway 使用，禁止把私有路由直接代理到公网。两项缺失时私有端点默认关闭；gateway 的 `202` / unknown 不证明目标已经准备成功，不能靠重新生成 operation 或目标来恢复。存储、原密钥和验证记录须独立备份，重启恢复沿用原件，不为排障换钥或清空私有存储。合同见[私有目标准备与回读](../../world-simulator/llm/external-agent-game-api.design.md)。

canonical 安装后，身份服务复用原 namespace / RP / epoch 和 SQLite，再配置实际值：`OASIS7_HOSTED_PARTICIPANT_ENTRY_ENABLED=1`、`OASIS7_HOSTED_OWNER_CONTROL_ENABLED=1`、`OASIS7_HOSTED_EXECUTOR_CONTROL_ENABLED=1`。机器注册还需已有 identity v2、account registration 与 `OASIS7_HOSTED_MACHINE_REGISTRATION_ENABLED=1`。`OASIS7_HOSTED_PARTICIPANT_ENTRY_TRUST` 是 `{endpoint, service_public_key, world:{world_id,genesis_digest}}`，endpoint 必须为实际 chain service 的精确 loopback HTTP origin（不能是公开站点或带路径 URL）；`OASIS7_HOSTED_PARTICIPANT_ISSUER_PRIVATE_KEY` 通过私密服务环境配置提供原 issuer seed 的 64hex，必须对应政策 issuer 公钥（与 finalize 的 raw32 文件为同一密钥的不同表示）。不要打印环境变量。

现有 HTTPS 站点须逐条配置以下 **完整前缀 `/api/public/hosted-account/v2` + 后缀** 的精确 location，转发至实际 launcher 静态 HTTP listener；保留可信 Host / Origin、真实源地址与安全响应头、CSRF / Cookie 处理及默认 `/api/` 拒绝。沿用 [部署手册的代理头及拒绝检查](single-ecs-preview-deploy.manual.md#公网-api-路由核对)，不要放行整个 prefix，更不要公开 chain loopback `/v1/world/*`。

| 精确后缀 | 请求方法 | 用途 |
| --- | --- | --- |
| `/entry/discovery` | GET | 新鲜分阶段能力及绑定世界发现 |
| `/capabilities`、`/session` | GET | 身份能力、CSRF 初始化及会话恢复 |
| `/machine-registration/options`、`/machine-registration/complete` | POST | 真实机器身份注册 |
| `/native-key/options`、`/native-key/complete` | POST | 已有机器身份重新认证 |
| `/participant/authorize`、`/participant/submit`、`/participant/lookup` | POST | participant 授权、提交、原操作查询 |
| `/participant/owner-view`、`/participant/starter-eligibility` | POST | owner 视图及 starter eligibility |
| `/entry-owner/control` | POST | owner 签名的目标 / task / executor 控制 |
| `/entry-executor/control` | POST | 当前 task fence 下 executor 操作 |

每个后缀独立 `location = <完整路径>`，按该行方法拒绝其他方法，不能使用通配代理。`nginx -t` / reload 后核对本机和公网的同路由；未认证的受保护 POST 应由身份服务拒绝，不能用伪造有效操作探测。未知路径仍拒绝，外域仍拒绝，禁止把代理 404 误判为世界政策缺失。浏览器玩家席位接口 `/api/public/player-session/admission` 另按部署手册配置，它不替代本表。

恢复 launcher 和身份写入后，使用全新外部客户端目录从 discovery 开始，验证注册、participant receipt、owner/task/goal、独立 executor 委托、观察与真实游戏动作及原操作重试恢复，核对 canonical 世界结果。分阶段 capability 不能代签完整冷入场；可信客户端测试通过也不能代签 OpenClaw/Codex 受限 Runtime 验收。后者继续按 [Runtime 合同验收手册](external-agent-runtime-contract-validation.manual.md)验证 broker、profile、隔离和预算。
