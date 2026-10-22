# 身份完整快照导出

适用：DevLocal participant 首次安装 / 更新政策时，向受信任 issuer 提供真实 SQLite 身份库全量前态。导出不是身份授权，也不直接安装世界政策；正式模型见 [外部 Agent API 设计](../../world-simulator/llm/external-agent-game-api.design.md)。

## 操作

先取得部署维护锁，停止 launcher、身份维护任务及所有其他身份写入者；维护窗口内不恢复写入，直到 issuer 核对快照、签发并完成世界安装。保留原库（含 WAL）的可恢复备份。事务快照本身允许 SQLite 并发写入，但签发需要停写避免导出后前态变化；锁的存在不等于所有写入者已停。

以数据库实际所属运行用户执行，同一发布版本的 launcher 支持独立命令，不启动 HTTP、Viewer 或游戏：

```bash
<release>/bin/oasis7_game_launcher --identity-snapshot-export \
  /absolute/private/identity.sqlite \
  /absolute/private/snapshot-request.json \
  /absolute/private/new-snapshot.json
```

`request.json` 为已有可信部署配置中的 namespace 和 RP，禁止临时猜测：

```json
{
  "identity_namespace": {
    "identity_service_id": "<deployed service ID>",
    "environment_id": "<deployed environment ID>"
  },
  "rp_config": {
    "rp_id": "<deployed RP ID>",
    "allowed_origins": ["<deployed exact origin>"],
    "version": 1,
    "identity_instance_epoch": 1
  }
}
```

示例数字必须替换为部署真实值。库和 request 必须为当前用户持有的普通文件、单链接、权限 `0600`，使用绝对 canonical 路径；父目录须由当前用户或 root 持有且不可被组 / 其他用户写入。输出父目录必须由当前有效 UID 持有且为 `0700`。输出必须不存在，以不可覆盖的原子发布写为 `0600`；不会覆盖已有证据。不要复制数据库、request 或密钥到公开目录。

## 证据与拒绝条件

导出以 SQLite 只读连接的**同一个事务**读取 schema、namespace、RP 和全量 records；不会调用会初始化库的 open/migration 流程，不依赖分页读取到的局部结果。全表超过 100 万条或单条 payload 超过 16 MiB、解码失败、schema / namespace / RP 不符、重复账号 / 绑定、悬空绑定均拒绝，失败不写部分输出。非 SQLite（包括尚无一致快照导出能力的 Tablestore）明确不支持，不能提供空快照代替。

输出仅包含账号 ID、状态、安全 epoch，以及相应 player binding、版本、计数、来源和摘要；不输出 display name、user handle、credential、session、token 或私钥。账户和绑定按稳定键排序。`source_snapshot_epoch` 是数据库真实 RP `identity_instance_epoch`，并非人为递增的快照号；同 epoch 多次快照由 watermark 区分。

`source_watermark` 为 `sqlite-content-v1:<全表记录数>:<BLAKE3>`，摘要覆盖 namespace、RP 及每条 records 的 namespace / key / kind / account ID / row version / expires / payload 摘要。凭据等非导出记录也参与完整性承诺，但不输出单条摘要。`source_snapshot_digest` 由共享强类型 `IdentitySnapshot::computed_digest()` 计算：将自身 digest 置空后按固定字段顺序序列化 JSON，再做 BLAKE3。消费端调用共享 `verify()`，不能自行用另一种字段排序计算。

digest 只能检出修改，不能证明来源可信或证明快照仍为当前态；issuer 必须在上述维护窗口内亲自从指定权威库取得并核对快照。身份 instance epoch、水位或世界前态变化后重新导出和签发，不复用旧快照。非空迁移仍须核对实际原 owner 签名；此命令既不生成 owner 签名，也不改变 registry、snapshot、journal 或 records。
