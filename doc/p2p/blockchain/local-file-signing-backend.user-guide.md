# 本地签名工具使用手册

本手册面向已安装工具的管理员和调用者。安装、版本绑定及部分失败恢复见 [operator runbook](local-file-signing-backend.runbook.md)，安全设计见 [design](local-file-signing-backend.design.md)。业务方决定何时签名和如何使用结果；工具负责校验授权、保管私钥和记录请求。

## 支持范围

源码已支持 Ed25519 的 `rollback_strict_audit_v1` 和 `file_ed25519_v1`：前者签署经过协议校验的 canonical rollback payload，后者签署绑定文件摘要、长度和完整上下文的专用 envelope。文件签名不自动成为交易或发布授权，其他算法及协议用途仍需专门实现。

这些新增能力随 [#4539](https://github.com/eng-cc/oasis7/pull/4539) 合入源码，但本会话原先安装的 `signer-recovery-20261010-7d32b77f` 未因此升级。先核对安装绑定和所选 release 的 `oasis7_local_signer_admin --help`。旧 release 仍只有 M0 rollback 能力；新版命令须通过单独批准的 release 安装后使用。新增命令和恢复合同详见 [密钥管理手册](local-file-key-management.manual.md)。

安装成功不等于签名已启用。无密钥安装可以完成权限边界验收，但缺少 policy 时 `doctor` 返回 `AUTHORIZATION_DENIED`、`ready: false` 是预期状态。管理员后续按需创建密钥、安装 policy 和 grant、启用 purpose；本手册中的命令不会自动执行。

## 角色和入口

| 角色 | 入口 | 职责 |
| --- | --- | --- |
| 已绑定调用者 | `oasis7_local_signer` | doctor、准备 job、提交、查询 |
| 主机管理员 | `oasis7_local_signer_admin`，以 root 执行 | 创建密钥、安装授权、启停、撤销 |
| 隔离 signer | `oasis7_local_signer_worker` | 由调用者 CLI 通过固定 sudo 规则调用 |

从 `/private/etc/oasis7/local-signer-installation.json` 确认当前 release、caller 和 work_dir。使用已安装 release 的绝对路径，不使用工作区编译产物替换管理员入口。下文 `$RELEASE` 是经核对的 `/usr/local/libexec/oasis7-local-signer/<release_id>`；`$JOB` 是本次唯一 job ID。普通 CLI 必须以绑定 caller 的真实身份运行，不能以 root 代替。不要直接调用 worker 或给 caller 增加 admin/shell 权限。

## 管理员：首次启用

先以实际 caller 执行 `$RELEASE/oasis7_local_signer doctor`，核对安装状态。随后由管理员在自己的终端执行以下各步；每个候选的内容和 SHA-256 应先核对，不能从示例复制占位值后直接应用。

```sh
sudo "$RELEASE/oasis7_local_signer_admin" key-new --signer-id "$SIGNER_ID" --purpose rollback_strict_audit_v1 --name "回滚审计签名"
sudo "$RELEASE/oasis7_local_signer_admin" key-public --signer-id "$SIGNER_ID" --format hex
sudo "$RELEASE/oasis7_local_signer_admin" policy-install --candidate "$POLICY_JSON" --expected-sha256 "$POLICY_SHA256"
sudo "$RELEASE/oasis7_local_signer_admin" approve-batch --candidate "$GRANT_JSON" --expected-sha256 "$GRANT_SHA256"
sudo "$RELEASE/oasis7_local_signer_admin" enable --purpose rollback_strict_audit_v1
```

新版使用 `key-new` 登记受管理密钥，默认不可导出；旧 release 使用 `key-create`，它仍只允许 rollback purpose。已有身份不能当作新密钥重复创建。公钥指纹是原始 32 字节公钥的 SHA-256，不是 hex 文本的 SHA-256。Policy/grant 候选文件必须位于已绑定 caller 的 work root 下的合法 job ID 子目录中（如 work_dir/admin-job/policy.json），归对应 caller 所有，为无符号链接、单硬链接、group/other 不可写的普通文件，大小不超过 1 MiB。管理员命令检查固定安装后再读取候选。加密密钥及备份输入使用另一套 root 保护路径规则，见下文。

### Policy 和 grant 的内容

精确字段与校验规则见 [types.rs](../../../crates/oasis7_signer_protocol/src/types.rs)。JSON 不接受未知字段；所有 SHA-256 使用 64 位小写 hex。`policy-install` 与 `approve-batch` 的候选文件必须逐字节等于 `canonical_json` 对已解析类型的输出：先转为 `serde_json::Value`，再用 `serde_json::to_vec` 编码为紧凑 UTF-8 JSON；对象键按字典序排列，没有缩进、额外空格、BOM 或末尾换行，字符串转义也必须与该序列化器一致。可选字段按类型序列化规则输出（包括显式 null）。先生成最终 canonical 字节，再计算其 SHA-256；仅摘要正确并不能使普通格式化 JSON 被接受。

| 候选 | 必须绑定的内容 |
| --- | --- |
| Policy `oasis7.local_signer_policy.v2` | installation_id、deployment_id、policy_revision、key_bindings、limits；安装时 enabled_purposes 必须为空 |
| key binding | purpose、signer_id、公钥指纹及允许的 protocol_authorities、provider_ids、deployment_ids、network_ids |
| Grant `oasis7.local_signing_batch_grant.v1` | grant_id、installation/deployment、caller_uid、network_id、task_uid、source_head_oid、policy_revision、有效时间、approval_record_ref、items |
| grant item | purpose、provider_id（当前为显式 null）、signer_id、公钥指纹、operation_key、payload_sha256、protocol_bindings、max_distinct_requests |

`not_before` 和 `expires_at` 是 UTC Unix 毫秒，前者必须小于后者。source_head_oid 是 40 位小写 Git OID。policy 上限为 64 个 batch item、每项 16 个 distinct request、payload 16 MiB；实际使用可设更小值。

`operation_key` 必须用 [identity::operation_key](../../../crates/oasis7_signer_protocol/src/identity.rs) 的算法生成：SHA-256(domain `oasis7.local-operation.v2\0` + installation/deployment/network/purpose/signer 的 UTF-8 字节，分别加 4 字节大端长度前缀 + 原始 32 字节 payload SHA-256)。不能用 job ID 或自行拼接字符串替代。协议绑定包含 authority_id、rollback_ticket、receipt_id、nonce；它们必须与 payload 和请求一致。

启用后以实际 caller 再运行 `doctor`。新版同时检查启用 purpose 的绑定密钥是否 active；inactive、archived、deleted 密钥会使其返回未就绪。`ready: true` 仍不保证某个请求拥有有效 grant、正确上下文或剩余额度；提交时逐项校验。

## 调用者：一次签名

1. 执行 `"$RELEASE/oasis7_local_signer" prepare --job-id "$JOB"`。空 job 返回 `JOB_DIRECTORY_READY` 和真实 work_dir。
2. 在该目录放入 `input.json` 和 `payload.bin`，使用 caller 所有的普通文件和私有目录，不使用符号链接。Rollback payload 来自业务协议生成器；[rollback.rs](../../../crates/oasis7_signer_protocol/src/rollback.rs) 校验其 canonical 编码和协议绑定。文件签名使用下面的 `file-payload` 生成器。
3. 再执行同一 `prepare`，预期 `PREPARED`。它生成 `request.json` 与 `request.payload.sha256` 并固定 request_id；保存这些文件。
4. 执行 `"$RELEASE/oasis7_local_signer" submit --job-id "$JOB"`。检查退出码和完整 JSON；成功必须是 `status: committed` 且 error_code 为 null。
5. 保存公开结果，核对 request_id、operation_key、公钥、signature_base64 和 audit_digest；业务消费者使用协议规定的消息及公钥验证签名，不以 CLI 成功代替业务验签。

`input.json` 结构如下，占位值由业务方填入；null 字段不能省略：

```json
{
  "schema_version": "oasis7.local_signer_job_input.v1",
  "installation_id": "INSTALLATION_ID",
  "purpose": "rollback_strict_audit_v1",
  "provider_id": null,
  "signer_id": "SIGNER_ID",
  "grant_id": "GRANT_ID",
  "context": {
    "deployment_id": "DEPLOYMENT_ID",
    "network_id": "NETWORK_ID",
    "task_uid": "TASK_UID",
    "source_head_oid": "REPLACE_WITH_40_LOWERCASE_HEX",
    "protocol_context": {
      "authority_id": "AUTHORITY_ID",
      "rollback_ticket": "ROLLBACK_TICKET",
      "receipt_id": "RECEIPT_ID",
      "nonce": "NONCE"
    }
  }
}
```

ID 一般为 2–128 字节 ASCII，首字符为字母或数字，其余允许字母、数字、点、下划线、连字符。不要修改已准备 job 的 input、payload 或生成元数据；input 派生字段与已准备请求不一致或 payload 变化会触发 `ID_CONFLICT`。工具不保证检测生成元数据的所有修改：改变合法的 request_id 会成为独立请求并消耗另一个额度，仅重新格式化 request.json 也不一定报错；必须由操作者保留原始生成文件和 request_id，不能把此检查视为元数据完整性保护。新业务请求使用新 job，但不能通过新 job 绕过 grant 的请求额度。

## 查询、停用和撤销

提交超时或结果不明时，保留原 job，先执行 `"$RELEASE/oasis7_local_signer" inspect --job-id "$JOB"` 查询已有 request。不要删除元数据或立即创建新请求重试。恢复前核对服务端状态、同一请求身份及当前授权；grant 过期或撤销后也不能假设旧请求必然可重放。

```sh
sudo "$RELEASE/oasis7_local_signer_admin" disable --purpose rollback_strict_audit_v1
sudo "$RELEASE/oasis7_local_signer_admin" revoke-grant --grant-id "$GRANT_ID"
```

disable 关闭 purpose；revoke-grant 保留授权记录并阻止后续使用。已发出的签名不会因此消失，业务侧仍需处理自己的撤销语义。

## 错误处理与能力边界

| 错误 | 下一步 |
| --- | --- |
| INVALID_INPUT / ID_CONFLICT | 核对 schema、显式 null、canonical payload 及原 job；保留原请求 |
| AUTHORIZATION_DENIED / KEY_OR_AUTHORITY_UNAVAILABLE / BUDGET_EXHAUSTED | 核对 purpose、key binding、grant、时间和额度；由管理员处理 |
| LOCK_BUSY | 保留 job，等待正在执行的操作结束 |
| PERSISTENCE_FAILED / RECOVERY_REQUIRED / INSTALLATION_DRIFT | 停止依赖动作，保留安装、journal、receipt 和 job，按 operator runbook 诊断 |
| WORKER_TIMEOUT / IPC_PROTOCOL_ERROR / WORKER_FAILED | 结果可能不明；先 inspect，不盲目重试 |
| UNSUPPORTED_PLATFORM_OR_FS | 核对受支持平台、文件系统和受保护路径，不放宽权限绕过 |

旧 `backup-prepare` 等 M0 占位入口不等于新版备份；新版使用 `backup-create`、`restore-plan` 和 `restore-apply`。主机安装走已批准 packaged installer，admin 的 install 命令不执行安装。私钥只允许在明确授权的加密导出或灾备中流出，不在 caller jobs、日志或仓库保存私有 custody/control 数据。部署状态、业务批准记录和签名结果应分别保留；不要修改历史 journal 来制造成功状态。

## 管理员：语义化密钥目录和生命周期

下列为新版 release 的命令模板，变量必须由操作者核对后填入。显示名、描述和 tags 便于查找；授权始终绑定不可变 signer ID、purpose 和公钥，而不是名称。

```sh
sudo "$RELEASE/oasis7_local_signer_admin" key-list
sudo "$RELEASE/oasis7_local_signer_admin" key-show --signer-id "$SIGNER_ID"
sudo "$RELEASE/oasis7_local_signer_admin" key-update --signer-id "$SIGNER_ID" --name "发布产物" --description "离线审批后的文件签名" --tags release,production
sudo "$RELEASE/oasis7_local_signer_admin" key-state --signer-id "$SIGNER_ID" --state inactive
sudo "$RELEASE/oasis7_local_signer_admin" key-state --signer-id "$SIGNER_ID" --state archived
sudo "$RELEASE/oasis7_local_signer_admin" key-audit
sudo "$RELEASE/oasis7_local_signer_admin" signing-audit
sudo "$RELEASE/oasis7_local_signer_admin" control-show
sudo "$RELEASE/oasis7_local_signer_admin" grant-list
sudo "$RELEASE/oasis7_local_signer_admin" grant-show --grant-id "$GRANT_ID"
```

`active` 可签名，`inactive` 停用，`archived` 归档；后两者阻止签名和已提交请求的授权重放。恢复 active 需要完整有效的密钥文件和另行有效的 policy/grant。旧密钥目录会以保守的 rollback-only、不可导出属性显示；`key-update` 可将其登记到受管理目录。

轮换使用 `key-rotate --signer-id "$SIGNER_ID" --new-signer-id "$NEW_SIGNER_ID"`。它先停用旧密钥，再创建同用途的新密钥并保存 lineage，不自动更改 policy/grant。核对新公钥后，由管理员按现有流程重新批准授权；不要假设轮换后业务请求立即可签。

删除是不可逆的显式操作：先停用或归档，再执行 `key-state --signer-id "$SIGNER_ID" --state deleted --confirm-delete`。工具移除 seed，保留公钥和历史 tombstone；不能复用删除后的 ID 或公钥。操作中断可能留下已停用或待恢复状态，保留文件和维护记录，不通过重建 ID 绕过恢复。

## 管理员：加密密钥导出和导入

新建时明确使用 `key-new ... --exportable` 才允许单密钥导出；默认不可导出，不能事后通过改名或状态切换提升为可导出。导入只接受工具产生的加密 envelope，导入后始终 inactive，并默认不可导出；需要将来再导出时，在导入时明确添加 `--exportable`。不支持明文 seed、任意 PEM 或其他算法的导入。

| 操作 | 新版管理员参数 |
| --- | --- |
| 导出 | `key-export --signer-id ID --output ABSOLUTE --passphrase-stdin` |
| 导入 | `key-import --signer-id NEW_ID --input ABSOLUTE --expected-sha256 SHA256 --passphrase-stdin [--exportable]` |

导入后用 `key-show` 核对公钥、用途和元数据，再通过 `key-state ... --state active` 显式激活。已有 ID、重复公钥和删除历史中的公钥均拒绝导入。单密钥 envelope 最多 32 MiB，使用固定 Argon2id 和 XChaCha20-Poly1305；口令丢失无法解密。

口令为 12–1024 字节，以一行加换行的非终端 stdin 输入，不允许作为命令行参数或环境变量。可使用密码管理器输送 stdin；终端 stdin 会被拒绝。下面是 macOS zsh 的单次导出模板，先完成 sudo 认证，使其不争用口令管道：

```zsh
sudo -v || exit
read -rs 'KEY_PASSWORD?加密口令: '
printf '\n'
printf '%s\n' "$KEY_PASSWORD" | sudo -n "$RELEASE/oasis7_local_signer_admin" key-export --signer-id "$SIGNER_ID" --output "$ENCRYPTED_OUTPUT" --passphrase-stdin
EXPORT_STATUS=$?
unset KEY_PASSWORD
test "$EXPORT_STATUS" -eq 0
```

不要启用 shell tracing，不将 `KEY_PASSWORD` export，不把实际口令写进脚本。对其他加密命令使用相同 stdin 方式，并检查退出码。输入必须是绝对路径的 root-owned `0600` 单链接普通文件，`--expected-sha256` 绑定已独立核对的文件摘要。输出父目录须受 root 保护；文件 create-only `0600`，已有输出不覆盖。密文和口令分开保管。

## 管理员：加密备份与恢复

先 disable 所有启用 purpose，确认 policy 的 enabled_purposes 为空。使用 `backup-create --output ABSOLUTE --passphrase-stdin` 创建加密快照。若其中存在不可导出 seed，必须明确加 `--include-nonexportable`；这是管理员灾备例外，软件不可导出属性不是硬件防提取保护。

快照包含密钥、控制配置和保留的签名请求/结果；不包含主机账户、sudo 配置或安装的可执行文件。上限为 450,000 文件、1 GiB 原始数据、单成员 1 MiB，备份密文输入最多 2 GiB。处理在内存中进行，接近上限可能需要约 8–12 GiB 内存；超出容量须另行设计归档流程，工具不静默丢弃历史记录。

恢复目标必须是单独安装、installation/deployment ID 相同的空 custody store，不覆盖现有生产库：

1. 使用 `restore-plan --input ABSOLUTE --expected-sha256 BACKUP_SHA256 --output ABSOLUTE --passphrase-stdin` 生成计划。
2. 独立核对并批准计划的安装绑定、备份摘要、恢复树摘要、文件数及计划文件 SHA-256。
3. 使用 `restore-apply --input ABSOLUTE --expected-sha256 BACKUP_SHA256 --plan ABSOLUTE --expected-plan-sha256 PLAN_SHA256 --passphrase-stdin`。
4. 检查密钥目录、历史和 control-show；恢复后的 policy 关闭签名，所有旧 grant 被撤销。重新审批新授权后才使用。

恢复保留快照时已记录的已消耗请求，不能借恢复重置这些历史额度或重用旧 grant。快照之后发生的请求不会自动恢复，业务方须另行保存和对账；旧快照不代表最新完整历史。中断时仅能用原密文、原口令和原已批准计划接续；不删除 receipt、maintenance 或其他部分文件。历史结果保存 payload digest，不包含原 payload；快照会核验 hash-linked 记录和绑定，但独立历史验签仍需要业务方保存原消息。

## 调用者：通用文件签名

管理员先通过 `key-new --signer-id ID --purpose file_ed25519_v1` 创建受管理密钥，并按前述流程批准 file purpose 的 policy/grant。旧 `key-create` 不能创建此用途。

普通 caller 在私有 `0700` job 目录准备与 input.json 一致的 context.json，并核对原文件及 context 的摘要：

```sh
"$RELEASE/oasis7_local_signer" file-payload \
  --context "$CONTEXT_JSON" --expected-context-sha256 "$CONTEXT_SHA256" \
  --file "$FILE_TO_SIGN" --expected-file-sha256 "$FILE_SHA256" \
  --output "$JOB_DIR/payload.bin"
```

所有路径为绝对路径，输出 create-only `0600`。此命令不签名、不调用 sudo，只生成 domain `oasis7:file-signature:v1\0` 后的 canonical envelope，绑定 purpose、完整 context、文件 SHA-256 和长度。context 中沿用 `rollback_ticket` 字段名，在 file purpose 下它只是审批上下文标识，不执行 rollback。

用输出的 payload_sha256 生成准确的 operation_key 和 grant item；input.json 的 purpose 改为 `file_ed25519_v1`，provider_id 仍为显式 null，context 必须与生成器一致。随后按前面的 prepare/submit/inspect 流程操作。验签方使用原文件与同一 context 重建 envelope，验证预期公钥的 Ed25519 签名；不能直接对原文件字节验这个签名。

## 受管理操作中断恢复

`RECOVERY_REQUIRED` 时先停下依赖操作、保留 custody 和维护记录。`management-recover --expected-operation-key SHA256 --expected-target-sha256 SHA256` 只在操作者批准精确 maintenance 绑定、目标已发布且完整验证通过时完成持久化，不生成缺失目标或重新创建密钥。Restore 使用自己的精确计划接续，不能用 management-recover 代替。旧占位 recover 命令和直接修改 journal 都不是恢复方式。
