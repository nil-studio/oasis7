# oasis7 Runtime：WASM Docker 确定性构建与工件治理管线设计

- 专业 owner：`wasm_platform_engineer`；生命周期：active professional authority，目标合同不等于已实现。
- 文档与静态源码核对：`eng-cc/oasis7`，`f97f2253cdcb163bf984e3c90d59d792fe1c9b19`，2026-10-15；本次没有运行 Cargo、Docker、runtime、跨宿主或发布验证。

## 1. 问题、目标与非目标

目标是让发布级 WASM 的构建输入、canonical bytes 和 receipt 可以独立核对，并区分 Linux stable、真实跨宿主和生产 release 三层证据。配对 PRD 的原 SC/AC/NFR、阈值和责任不变；下方原详细设计继续约束构建、manifest、source compile 与证据流程。不改变代码、CI、ABI、安全政策或产品验收，不将 recipe digest 等同执行镜像身份。

## 2. 上游约束与相关角色

配对 PRD 拥有构建专业义务；runtime/release owner 拥有生产工件接收和策略；QA 拥有同候选环境及组合结论。DWE-001 的确定性结果需要稳定工件输入，但跨宿主 build hash 不是权威结果；DWE-004 的历史版本需要可验证工件，但工件 receipt 不决定 canonical block 的 governing version。Docker 编排与 env 命名仅在相应机制范围不独立新增玩家要求。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [SC-1](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-1) | SC-1: 同一 commit 在 macOS 与 Linux 上通过同一 pinned Docker builder image 构建时，得到的 canonical packaged wasm hash 一致率为 `100%`。 | [跨宿主 summary 与候选证据](#build-cross-host-contract) | qa_engineer；真实 Docker-capable runner owner | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 |
| [SC-2](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-2) | SC-2: 发布级模块工件只产生一个 canonical publish hash，来源固定为 `linux-x86_64` 容器构建；宿主平台不再写入独立发布 hash。 | [单 canonical 发布 hash](#build-publish-hash-contract) | 构建与 identity sync owner | 不证明治理激活、签名或历史工件长期可用。 |
| [SC-3](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-3) | SC-3: build receipt 必须能追溯 `builder_image_ref + builder_image_digest + container_platform + source_hash(含模块本地 path 依赖闭包) + build_manifest_hash + wasm_hash + canonicalizer_version`。 | [Receipt、源码闭包与工件摘要](#build-receipt-contract) | wasm_platform_engineer；identity sync owner | 仅构建身份子范围；receipt 的 recipe digest 不独立证明执行镜像字节恒等。 |
| [SC-4](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-4) | SC-4: runtime 与节点执行路径默认只接受 Docker canonical build 产生的 wasm binary 与其 identity/release evidence，不要求节点重新编译源码。 | [Builtin materialization 与 binary 消费](#build-materialization-contract) | runtime artifact loader；identity owner | hash 校验不等于 release authorization；本地 fallback 与缓存不能选择 governing version。 |
| [SC-5](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-5) | SC-5: `ModuleSourcePackage` 的生产发布路径不得继续依赖 runtime 进程在宿主机直接编译；必须迁移到同一 Docker builder 或显式 gated 为 dev/test only。 | [生产 source compile 边界](#build-source-policy-contract) | runtime_engineer；生产入口 owner | 未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。 |
| [SC-6](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-6) | SC-6: 发布候选要宣称“跨宿主 determinism 已收口”时，必须归档至少一条 `linux-x86_64` 与一条 Docker-capable `darwin-arm64` 的 canonical summary / release evidence；Linux-only gate 只能代表稳定基线，不能代表跨宿主 closure。 | [跨宿主 summary 与候选证据](#build-cross-host-contract) | qa_engineer；真实 Docker-capable runner owner | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 |
| [SC-7](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-7) | SC-7: 生产 runtime / node 入口必须默认关闭 builtin manifest fallback、本地 identity hash 签名、本地 finality signing 与 runtime source compile，保证 binary-only policy 是生产默认行为而不是测试显式开关。 | [生产 release policy 与授权](#build-release-policy-contract) | runtime_engineer；release/trust owner | 未重新核实全部生产入口、签名/最终性或发布状态。 |
| [SC-8](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-8) | SC-8: 跨宿主 Docker release evidence 必须可被包装成 node-side attestation proof payload，并进入 `ModuleReleaseSubmitAttestation.proof_cid`；仅存在于 CI artifact 的 report 不能单独视为生产 closure。 | [Node-side proof assembly](#build-node-proof-contract) | 发布节点 proof assembler；release/trust owner | 仅验证设计；本次没有运行 proof assembly、签名、提交或共识验证。 |
| [SC-9](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-9) | SC-9: Docker-first canonical build 链路的 operator env key 必须统一到 `OASIS7_WASM_*` 当前入口；不得再保留任何旧品牌前缀作为有效运行入口，避免 host wrapper、builder image、sync/check 与 build receipt 采集口径分叉。 | [Operator env 命名](#build-env-contract) | 构建维护者；runtime materializer owner | 命名检查不证明构建、可用性或发布权限。 |
| [PRD-WORLD_RUNTIME-020](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-020) | PRD-WORLD_RUNTIME-020: As a `wasm_platform_engineer`, I want publishable WASM to be built only inside a pinned Docker builder image, so that host platform differences stop influencing release hashes. | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | wasm_platform_engineer；容器构建维护者 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 |
| [PRD-WORLD_RUNTIME-021](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-021) | PRD-WORLD_RUNTIME-021: As a 发布节点运营者, I want each artifact to carry a build receipt that binds builder image digest, source hash, build manifest hash, and canonical wasm hash, so that social verification no longer depends on “which laptop built it”. | [Receipt、源码闭包与工件摘要](#build-receipt-contract) | wasm_platform_engineer；identity sync owner | 仅构建身份子范围；receipt 的 recipe digest 不独立证明执行镜像字节恒等。 |
| [PRD-WORLD_RUNTIME-022](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-022) | PRD-WORLD_RUNTIME-022: As a `runtime_engineer` / `qa_engineer`, I want runtime to consume only Docker-canonical binaries and CI to compare Docker outputs across hosts, so that drift is blocked before execution. | [Builtin materialization 与 binary 消费](#build-materialization-contract) | runtime artifact loader；identity owner | hash 校验不等于 release authorization；本地 fallback 与缓存不能选择 governing version。 |
| [AC-1](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-1) | AC-1 (PRD-WORLD_RUNTIME-020): 必须新增并固定一份 WASM builder Docker image，镜像引用必须以 digest pin；所有 publishable wasm 构建都通过 `docker run` 进入该镜像。 | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | wasm_platform_engineer；容器构建维护者 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 |
| [AC-2](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-2) | AC-2 (PRD-WORLD_RUNTIME-020): builder image 必须封装 Rust toolchain、`rust-src`、`wasm32-unknown-unknown` 目标、linker/canonicalizer 所需依赖，并把这些版本信息收敛到 `build_manifest_hash`。 | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | wasm_platform_engineer；容器构建维护者 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 |
| [AC-3](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-3) | AC-3 (PRD-WORLD_RUNTIME-020): `scripts/build-wasm-module.sh` 的 canonical path 必须改为 Docker wrapper；publishable 构建不再保留 host-native cargo fallback。 | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | wasm_platform_engineer；容器构建维护者 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 |
| [AC-4](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-4) | AC-4 (PRD-WORLD_RUNTIME-021): 发布级 hash manifest 目标态只允许写入单个 canonical token：`linux-x86_64=<sha256>`；`darwin-arm64` 不再作为发布 hash 来源。 | [单 canonical 发布 hash](#build-publish-hash-contract) | 构建与 identity sync owner | 不证明治理激活、签名或历史工件长期可用。 |
| [AC-5](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-5) | AC-5 (PRD-WORLD_RUNTIME-021): build receipt 至少绑定 `builder_image_digest + container_platform + source_hash(含本地 path 依赖闭包) + build_manifest_hash + wasm_hash + canonicalizer_version`，并进入 identity/release evidence。 | [Receipt、源码闭包与工件摘要](#build-receipt-contract) | wasm_platform_engineer；identity sync owner | 仅构建身份子范围；receipt 的 recipe digest 不独立证明执行镜像字节恒等。 |
| [AC-6](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-6) | AC-6 (PRD-WORLD_RUNTIME-022): multi-runner CI 必须比较“相同 Docker builder 在不同宿主上产出的 canonical hash”，而不是继续比较 host-native cargo 输出。 | [跨宿主 summary 与候选证据](#build-cross-host-contract) | qa_engineer；真实 Docker-capable runner owner | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 |
| [AC-7](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-7) | AC-7 (PRD-WORLD_RUNTIME-022): runtime 与节点执行路径默认只接受 canonical Docker build 产物；节点不通过重新编译源码参与执行合法性判断。 | [Builtin materialization 与 binary 消费](#build-materialization-contract) | runtime artifact loader；identity owner | hash 校验不等于 release authorization；本地 fallback 与缓存不能选择 governing version。 |
| [AC-8](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-8) | AC-8 (PRD-WORLD_RUNTIME-022): `compile_module_artifact_from_source` 的生产路径必须迁移到外部 Docker builder 或直接禁用；runtime 进程内 host 直编只允许在 dev/test 模式下显式开启。 | [生产 source compile 边界](#build-source-policy-contract) | runtime_engineer；生产入口 owner | 未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。 |
| [AC-9](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-9) | AC-9 (PRD-WORLD_RUNTIME-021/022): 若 GitHub-hosted CI 因 runner 能力不足只能保留 Linux-only stable gate，PRD / project / evidence 报告必须把跨宿主 evidence 标记为 pending，直到导入 Docker-capable macOS summary 为止。 | [跨宿主 summary 与候选证据](#build-cross-host-contract) | qa_engineer；真实 Docker-capable runner owner | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 |
| [AC-10](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-10) | AC-10 (PRD-WORLD_RUNTIME-022): production 运行入口必须提供 release security policy 绑定证据，证明 fallback / 本地签名 / runtime source compile 默认关闭；仅在测试里调用 `enable_production_release_policy()` 不足以视为完成。 | [生产 release policy 与授权](#build-release-policy-contract) | runtime_engineer；release/trust owner | 未重新核实全部生产入口、签名/最终性或发布状态。 |
| [AC-11](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-11) | AC-11 (PRD-WORLD_RUNTIME-021/022): release evidence 除了 CI/report 汇总外，还必须存在 node-side proof payload 打包与 attestation submit 入口，使 `builder_image_digest/container_platform/canonicalizer_version` 可作为发布节点提交的正式证明字段进入共识链路。 | [Node-side proof assembly](#build-node-proof-contract) | 发布节点 proof assembler；release/trust owner | 仅验证设计；本次没有运行 proof assembly、签名、提交或共识验证。 |
| [AC-12](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-12) | AC-12 (PRD-WORLD_RUNTIME-020/021): `scripts/build-wasm-module.sh`、`scripts/sync-m1-builtin-wasm-artifacts.sh`、`scripts/ci-m1-wasm-summary.sh`、`tools/wasm_build_suite` 与 `docker/wasm-builder/Dockerfile` 必须只写入或读取 `OASIS7_WASM_*`；wrapper usage、错误提示、容器注入 env 与 build receipt 元数据采集不得再接受任何旧品牌前缀作为有效运行入口。 | [Operator env 命名](#build-env-contract) | 构建维护者；runtime materializer owner | 命名检查不证明构建、可用性或发布权限。 |
| [AC-13](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-13) | AC-13 (PRD-WORLD_RUNTIME-021/022): builtin wasm materializer、release manifest fallback 与 DistFS root override 的 runtime env key 必须只读取 `OASIS7_BUILTIN_WASM_*`；Docker-first build 已完成品牌迁移后，runtime 取件/抓取/编译 fallback 不得继续接受任何旧品牌前缀作为有效运行入口。 | [Operator env 命名](#build-env-contract) | 构建维护者；runtime materializer owner | 命名检查不证明构建、可用性或发布权限。 |
| [NFR-WDBP-1](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-1) | NFR-WDBP-1: 同一源码、同一 builder image digest、同一 `linux-x86_64` container platform 的 canonical wasm hash 可复现率必须为 `100%`。 | [跨宿主 summary 与候选证据](#build-cross-host-contract) | qa_engineer；真实 Docker-capable runner owner | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 |
| [NFR-WDBP-2](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-2) | NFR-WDBP-2: Docker canonical build 失败日志必须在一次执行内定位到 `builder_image_digest`、`module_id`、`expected`、`actual` 或容器入口失败原因。 | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | wasm_platform_engineer；容器构建维护者 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 |
| [NFR-WDBP-3](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-3) | NFR-WDBP-3: 发布级 hash manifest 不再依赖宿主平台集合扩容；宿主平台差异不应影响发布 hash 空间。 | [单 canonical 发布 hash](#build-publish-hash-contract) | 构建与 identity sync owner | 不证明治理激活、签名或历史工件长期可用。 |
| [NFR-WDBP-4](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-4) | NFR-WDBP-4: 本地默认路径在无显式授权时不得修改 tracked manifest / identity；CI 也不得写入。 | [单 canonical 发布 hash](#build-publish-hash-contract) | 构建与 identity sync owner | 不证明治理激活、签名或历史工件长期可用。 |
| [NFR-WDBP-5](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-5) | NFR-WDBP-5: 生产态 `ModuleSourcePackage` 不得要求 runtime 节点本地具备 Docker 才能执行共识路径。 | [生产 source compile 边界](#build-source-policy-contract) | runtime_engineer；生产入口 owner | 未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。 |
| [NFR-WDBP-6](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-6) | NFR-WDBP-6: 发布证据报告若缺少 `darwin-arm64` Docker canonical summary，必须在机器可读输出中明确标示 `cross_host_evidence_pending=true` 或同等阻断状态，避免把 Linux-only 结果误判为 full closure。 | [跨宿主 summary 与候选证据](#build-cross-host-contract) | qa_engineer；真实 Docker-capable runner owner | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 |
| [NFR-WDBP-7](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-7) | NFR-WDBP-7: 生产入口 hardened release policy 的默认值必须可由自动化验证，不允许依赖人工约定或测试专用辅助调用。 | [生产 release policy 与授权](#build-release-policy-contract) | runtime_engineer；release/trust owner | 未重新核实全部生产入口、签名/最终性或发布状态。 |
| [BUILD-01](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-1) | 固定 image/platform/source/tooling/wasm 身份与单 token；分项检验，不以 scope fixture 代签 | [Receipt、源码闭包与工件摘要](#build-receipt-contract) | wasm_platform_engineer；identity sync owner | 仅构建身份子范围；receipt 的 recipe digest 不独立证明执行镜像字节恒等。 |
| [BUILD-02](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-2) | 同候选实际 Linux+Darwin Docker summaries；Linux-only 为 partial | [跨宿主 summary 与候选证据](#build-cross-host-contract) | qa_engineer；真实 Docker-capable runner owner | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 |
| [BUILD-03](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-3) | node proof、production source compile 与 materializer fallback 分别核对，不能互相代签 | [生产 release policy 与授权](#build-release-policy-contract) | runtime_engineer；release/trust owner | 未重新核实全部生产入口、签名/最终性或发布状态。 |
| [BUILD-01](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-1) | 固定 Docker/platform，拒绝不可用 Docker 与自定义错误镜像；recipe 与执行 ID 分离 | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | wasm_platform_engineer；容器构建维护者 | 仅 BUILD-01 的镜像/平台编排子范围；recipe provenance 不代签 image bytes；工件关系按本表其他行独立保留。 |
| [BUILD-01](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-1) | 只写一个 canonical linux-x86_64 token，拒绝输出/清单错配 | [单 canonical 发布 hash](#build-publish-hash-contract) | 构建与 identity sync owner | 不证明治理激活、签名或历史工件长期可用。 |
| [BUILD-03](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-3) | production source compile 拒绝与 dev/test opt-in 独立验证 | [生产 source compile 边界](#build-source-policy-contract) | runtime_engineer；生产入口 owner | 未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。 |
| [BUILD-03](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-3) | 缺失/伪造 node proof 不得用 CI artifact 代签 | [Node-side proof assembly](#build-node-proof-contract) | 发布节点 proof assembler；release/trust owner | 仅验证设计；本次没有运行 proof assembly、签名、提交或共识验证。 |
| [BUILD-03](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-3) | materializer fetch/compile/cache 与 action-level policy 分开核对 | [Builtin materialization 与 binary 消费](#build-materialization-contract) | runtime artifact loader；identity owner | hash 校验不等于 release authorization；本地 fallback 与缓存不能选择 governing version。 |
| [PRD-WORLD_RUNTIME-022](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-022) | 同候选真实跨宿主 Docker compare，缺 Darwin 为 pending | [跨宿主 summary 与候选证据](#build-cross-host-contract) | qa_engineer；真实 Docker-capable runner owner | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 |
| [PRD-WORLD_RUNTIME-022](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-022) | runtime 消费合法 binary；生产源码编译必须外移或关闭 | [生产 source compile 边界](#build-source-policy-contract) | runtime_engineer；生产入口 owner | 未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。 |
| [PRD-WORLD_RUNTIME-022](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-022) | 生产入口有效策略与拒绝行为不可由测试辅助调用代签 | [生产 release policy 与授权](#build-release-policy-contract) | runtime_engineer；release/trust owner | 未重新核实全部生产入口、签名/最终性或发布状态。 |

## 3. 当前状态、目标状态与差距

| 对象 | 已静态核实的当前实现 | 目标/剩余差距 |
| --- | --- | --- |
| wrapper / 镜像 | Docker-only canonical wrapper 捕获 `SELECTED_BUILDER_IMAGE_ID`，执行该不可变 local ID；默认镜像特定 variance/rebuild 分支保留配置 recipe digest | 原 pinned-image 目标仍有效；receipt 字段不足以单独证明执行镜像字节恒等，不能把差异静默解释为目标完成 |
| source / receipt | build suite 调用共享 source closure，计算 canonical packaged wasm SHA-256；dry-run 不产出这些摘要 | 真实模块依赖闭包、镜像、tooling、输出和 receipt 需同候选核对；本次仅源码与测试定义审读 |
| production policy | `ReleaseSecurityPolicy::production_hardened()` 四项 false；`Default` 四项 true；生产入口须明确绑定 hardened | 构造器存在不证明所有实际入口；原阶段证据不代签新候选 |
| materializer | 本地 CAS/远程候选校验，fetch miss 后仍可编译/缓存；不接收 receipt、identity 或 release policy | action source compile 门禁不覆盖该 fallback；完整接收合法性由外围 authority 判断 |
| cross-host / proof | verifier 比较 summaries 字段和 hashes；workflow 定义真实 Docker 环境；历史 run 另保留 | 未运行当前 candidate 双宿主、proof assembly 或实际发布；字段一致不是独立镜像字节证明 |

## 4. 边界与结构

源码与锁文件进入 build suite；wrapper 选择执行镜像；packager 产生 wasm bytes/hash；sync/identity 将这些材料关联至发布清单；runtime/materializer 获取字节，release/trust 决定可否接收。DistFS/CAS 只提供 hash-bound 材料，不能取得最终写入权或世界可服务状态；此边界沿用 [DCS 不变量](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#2-边界与不变量)。版本选择、activation、历史 replay 由 [builtin identity 接收器](../../p2p/consensus/builtin-wasm-identity-consensus.design.md#builtin-artifact-version-binding)、[interface 生命周期](wasm-interface.md#5-安装升级禁用和迁移)及 runtime authority 承担。

## 5. 关键运行流程

1. 固定模块/source closure、tooling、builder 配置与 canonical platform；Docker 不可用或自定义镜像不匹配即失败，不生成 native 发布替代物。
2. wrapper 捕获 immutable local image ID 后运行容器；build suite 在非 dry-run 路径编译、canonicalize、hash 并写 receipt。
3. sync/check 对照适用模块 manifest/identity/receipt；错误保留诊断，不能自动写第二个宿主发布 token。
4. 同候选双宿主 summary 对账后形成相应构建结论；缺 Darwin 只留 Linux stable / cross-host pending。
5. 发布节点独立装配 proof，生产 runtime 独立验证 policy/trust；失败停在该层，built 或 receipted 不意味着激活/最终化。重新构建应重核全部输入；本设计不新增构建事务、自动重试或发布幂等保证。

## 6. 接口与数据合同

<a id="sr2-build-contract"></a>
### 6.1 Build identity 与 proof join

保留旧接收锚点作为分项入口，避免已有消费者失联。原 BuildReceiptIdentityV1 target projection 保留 `{builder_image_ref:Id,builder_image_digest:Id,container_platform:"linux-x86_64",source_hash:Hash32,build_manifest_hash:Hash32,wasm_hash:Hash32,canonicalizer_version:Id}`；digest 的原目标格式为 `sha256:` 加 64 lowercase hex，source_hash 覆盖本地 path dependency closure。该 projection 不替换现行 JSON schema，也不证明其字段已签名或独立可信。

- Recipe provenance：当前 `builder_image_digest()` 在配置值非空且不同于 actual ID 时返回配置 recipe digest；它进入 receipt 与 build_manifest_hash。
- 执行镜像身份：wrapper 先捕获 `SELECTED_BUILDER_IMAGE_ID`，验证其格式后直接传给 docker run，不在执行时重新解析 mutable tag。默认镜像允许指定 variance，或重建后产生不同 local ID；自定义镜像不匹配仍拒绝。
- Artifact identity：canonical packaged bytes 的 SHA-256 是独立输出。recipe 相等、local image ID 和 artifact hash 三者不能互相替代；receipt 不独立证明所执行镜像字节恒等。

原 pinned image/同镜像目标与上述 recipe 行为的差距由 builder/release owner 另行裁决；本次不选择新的镜像政策、不改变安全要求，也不凭静态核读宣布该目标满足。

<a id="build-wrapper-contract"></a>
### Docker wrapper 与固定构建环境

产品适用性：N/A；reason=Docker调用与容器路径组织本身不独立定义玩家结果；scope=wrapper编排、工具封装和运行错误定位；不排除receipt与实际工件关系。

具体流程、字段和原约束见 [原详细设计](#build-wrapper-detail)。wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。

固定 `linux/amd64` 容器与 `linux-x86_64` canonical token；路径映射、只读源码/可写输出、locked toolchain 与无 native fallback 仍按原 §5.1/5.2；实际 local image ID 与 recipe digest 按 §6.1 分离。

<a id="build-receipt-contract"></a>
### Receipt、源码闭包与工件摘要

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)。仅承接本节下述局部范围，完整产品义务与未证明边界不变。

具体流程、字段和原约束见 [原详细设计](#build-suite-detail)。仅构建身份子范围；receipt 的 recipe digest 不独立证明执行镜像字节恒等。

源码摘要复用共享算法，覆盖模块本地 path dependency closure 和适用 Cargo.lock；build manifest 绑定 profile/target/tooling/canonicalizer/recipe 等输入；wasm_hash_sha256 只取 canonical packaged bytes。`dry_run` 的 source/hash/receipt 输出为 None，不是构建证据。

<a id="build-publish-hash-contract"></a>
### 单 canonical 发布 hash

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)。仅承接本节下述局部范围，完整产品义务与未证明边界不变。

具体流程、字段和原约束见 [原详细设计](#build-manifest-detail)。不证明治理激活、签名或历史工件长期可用。

单 `linux-x86_64=<sha256>` 表示 canonical container 发布 hash，不要求宿主为 Linux；本地默认与 CI 不得擅写 tracked manifest/identity。三套模块与 identity 对账不能仅验 m1 代签所有适用集合。

<a id="build-cross-host-contract"></a>
### 跨宿主 summary 与候选证据

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)。仅承接本节下述局部范围，完整产品义务与未证明边界不变。

具体流程、字段和原约束见 [原详细设计](#build-cross-host-detail)。跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。

同 source/image/tooling 的实际 Linux 与 Docker-capable Darwin 结果按原 100% 一致目标核对；receipt_evidence_consistent 只证明输入报告的一致性，不补足独立镜像字节证明。历史 run 28297899706 只属于其原候选，不能填当前缺失的 Darwin 证据。

<a id="build-source-policy-contract"></a>
### 生产 source compile 边界

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)。仅承接本节下述局部范围，完整产品义务与未证明边界不变。

具体流程、字段和原约束见 [原详细设计](#build-source-detail)。未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。

生产默认关闭 Action::CompileModuleArtifactFromSource 的目标保留；外部 Docker builder 方案与 dev/test 明确 opt-in 的源码包路径分开。dev/test 路径/大小/timeout guardrail 不等于生产 sandbox 或发布授权。

<a id="build-release-policy-contract"></a>
### 生产 release policy 与授权

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)。仅承接本节下述局部范围，完整产品义务与未证明边界不变。

具体流程、字段和原约束见 [原详细设计](#build-policy-detail)。未重新核实全部生产入口、签名/最终性或发布状态。

原 production 四项 false 与入口自动绑定、status/evidence 输出的要求全部保留；不要把 ReleaseSecurityPolicy::default() 误写为 hardened。必须核真实入口有效配置和拒绝行为，source compile-off 不等于 materializer compile fallback-off。

<a id="build-node-proof-contract"></a>
### Node-side proof assembly

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)。仅承接本节下述局部范围，完整产品义务与未证明边界不变。

具体流程、字段和原约束见 [原详细设计](#build-proof-detail)。仅验证设计；本次没有运行 proof assembly、签名、提交或共识验证。

CI summaries/report 仅作为输入材料；node canonicalize 去掉时间、路径等非语义字段后，生成 proof_inputs，再 package/submit。proof bytes/hash、issuer epoch/stake/min-independent-signers 与 canonical finality 仍由 release receiver 独立验证，receipt 不产生正式提交或激活。

<a id="build-env-contract"></a>
### Operator env 命名

产品适用性：N/A；reason=运行入口命名自身不独立定义世界执行或发布权限；scope=OASIS7_WASM与OASIS7_BUILTIN_WASM前缀的接口统一；不代签工件身份。

具体流程、字段和原约束见 [原详细设计](#build-wrapper-detail)。命名检查不证明构建、可用性或发布权限。

构建 wrapper/suite/sync/summary/Dockerfile 的 OASIS7_WASM_* 与 runtime materializer/release 的 OASIS7_BUILTIN_WASM_* 分别核验；名字迁移不得悄悄改变参数、权限、默认值或 fallback 语义。

<a id="build-materialization-contract"></a>
### Builtin materialization 与 binary 消费

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)。仅承接本节下述局部范围，完整产品义务与未证明边界不变。

具体流程、字段和原约束见 [原详细设计](#build-materialization-detail)。hash 校验不等于 release authorization；本地 fallback 与缓存不能选择 governing version。

远程候选必须验证实际 bytes/hash；本地 compile/cache 兼容行为仍按现行 materializer，不能声称所有获取成功都命中 canonical manifest。materializer 本身不验证 identity/receipt/release policy；外围 artifact selection、identity 和授权不可省略。

## 7. 状态、事务与持久化

requested/container_started/built/receipted 是构建过程，不是世界提交。receipt、manifest 与 identity 分别保存构建材料和绑定；CI artifact 受 retention 管理，不保证长期可用。失败或输入变化必须重新核对；本次未增加跨文件原子落盘、crash recovery 或并发发布保证。历史已确认工件不得因新构建被改写成另一版本；世界恢复与事务由 runtime 拥有。

## 8. 部署、安全与运行约束

Docker daemon 只属于受控 builder 或开发主机，普通 runtime binary consumer 不以 Docker 为执行共识前提。私钥、发布证书和签名分片不进入镜像、日志或 receipt。Docker Desktop/engine 版本只作诊断；实际 receipt、hash、production policy 与 trust 检查分别成立，不以本地签名或缓存绕过。无真实环境则保留未证明，不调整安全政策以迁就当前实现。

## 9. 质量与容量

质量阈值沿配对 PRD NFR-WDBP-1..7：100% canonical hash 一致、单次日志可定位、单 canonical hash 空间、默认不写 tracked 清单、runtime 无 Docker 前提、缺 Darwin 机器可读 pending、production effective policy 可自动验证。每项在下表绑定其真实输入、环境与 oracle；本次没有测量跨宿主可复现率、构建耗时或运行资源预算，不新造性能承诺。

## 10. 兼容、迁移与回滚

原多 token 读兼容、单 canonical token 写迁移与 M0..M5 设计保留；不把历史计划重启为当前排期。旧 receipt/manifest、历史 artifact 与 interface 原版本不被本次编辑替换。回滚构建工具不能撤销已治理激活的世界事实；缺少可信工件或证据时停止发布，不恢复 host-keyed 发布 hash 兜底。

## 11. 验证设计与可追溯性

下表是逐义务验证计划，全部技术执行均未在本次运行。实际执行必须记录同一 candidate/source、配置、模块集合、host/container identity、命令/退出码及输出材料；required 局部 fixture、Linux stable、full 跨宿主和正式 node release proof 分开。scope planner 只验证路径选择与模块范围，不证明 Docker、构建输出、production policy 或 node proof。产品组合沿 [QA 层级](../../testing/prd.md#qa-infrastructure-tier-boundaries)与 [QA 设计](../../testing/design.md) 的「QA infrastructure evidence method」节；不改原 tier。

进一步进入世界组合证据时，原 exact source/integration/tested tree、execution/protocol/test-contract、world branch/window 的一致性要求仍由 [DC5 接收器](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-attachment)及其 [schema](../../testing/schemas/world-execution-state-sync-attachment.schema.json)拥有；构建局部结果不裁剪 SC9/DC5 原有 full、provider 或 headed 义务。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [SC-1](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-1) | [跨宿主 summary 与候选证据](#build-cross-host-contract) | SC-1: 同一 commit 在 macOS 与 Linux 上通过同一 pinned Docker builder image 构建时，得到的 canonical packaged wasm hash 一致率为 `100%`。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；同候选运行 `../../../.github/workflows/wasm-determinism-gate.yml` 与 `../../../.github/workflows/wasm-darwin-docker-evidence.yml`；用 `../../../scripts/ci-verify-m1-wasm-summaries.py` 比对适用模块及 receipt/identity/hash，保留缺 Darwin 和错配失败样例；Linux-only 不得标 full。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 本次未运行。 |
| [SC-2](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-2) | [单 canonical 发布 hash](#build-publish-hash-contract) | SC-2: 发布级模块工件只产生一个 canonical publish hash，来源固定为 `linux-x86_64` 容器构建；宿主平台不再写入独立发布 hash。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；对同候选适用 m1/m4/m5 执行既有 sync --check，核对 --check 前后 tracked manifest/identity 未被改写、每个发布 manifest 仅有 linux-x86_64 token，并比对实际 packaged wasm、receipt 和 identity；`../../../scripts/ci-verify-m1-wasm-summaries.py` 核对 module_hashes/manifest_platform_hashes/receipt_evidence。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 不证明治理激活、签名或历史工件长期可用。 本次未运行。 |
| [SC-3](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-3) | [Receipt、源码闭包与工件摘要](#build-receipt-contract) | SC-3: build receipt 必须能追溯 `builder_image_ref + builder_image_digest + container_platform + source_hash(含模块本地 path 依赖闭包) + build_manifest_hash + wasm_hash + canonicalizer_version`。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；`../../../tools/wasm_build_suite/src/lib.rs` 的 source_hash_tracks_local_path_dependency_changes、canonicalize_wasm_bytes_drops_all_custom_sections；`../../../crates/oasis7_wasm_build/src/lib.rs` 的 source_hash_changes_when_workspace_ancestor_lockfile_changes；同候选核对真实 receipt、输入与输出 hash，不用 fixture 代签真实工件。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 仅构建身份子范围；receipt 的 recipe digest 不独立证明执行镜像字节恒等。 本次未运行。 |
| [SC-4](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-4) | [Builtin materialization 与 binary 消费](#build-materialization-contract) | SC-4: runtime 与节点执行路径默认只接受 Docker canonical build 产生的 wasm binary 与其 identity/release evidence，不要求节点重新编译源码。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；按 `../../p2p/consensus/builtin-wasm-identity-consensus.design.md#builtin-identity-contract` 核对 source/build/identity 与实际 wasm hash；读取 `../../../crates/oasis7_wasm_materializer/src/lib.rs` 的 fetch/compile/cache 分支并在同候选验证远程错 hash 拒绝与本地 fallback；identity 与 release authority 另验。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | hash 校验不等于 release authorization；本地 fallback 与缓存不能选择 governing version。 本次未运行。 |
| [SC-5](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-5) | [生产 source compile 边界](#build-source-policy-contract) | SC-5: `ModuleSourcePackage` 的生产发布路径不得继续依赖 runtime 进程在宿主机直接编译；必须迁移到同一 Docker builder 或显式 gated 为 dev/test only。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；在同候选 production 入口检查 effective ReleaseSecurityPolicy 并提交源码编译 action，要求拒绝；dev/test 显式放宽另测；`../../../crates/oasis7_runtime/src/world/mod.rs` 的 production_hardened 与 `../../../crates/oasis7_runtime/src/module_source_compiler.rs` 仅提供静态定位。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。 本次未运行。 |
| [SC-6](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-6) | [跨宿主 summary 与候选证据](#build-cross-host-contract) | SC-6: 发布候选要宣称“跨宿主 determinism 已收口”时，必须归档至少一条 `linux-x86_64` 与一条 Docker-capable `darwin-arm64` 的 canonical summary / release evidence；Linux-only gate 只能代表稳定基线，不能代表跨宿主 closure。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；同候选运行 `../../../.github/workflows/wasm-determinism-gate.yml` 与 `../../../.github/workflows/wasm-darwin-docker-evidence.yml`；用 `../../../scripts/ci-verify-m1-wasm-summaries.py` 比对适用模块及 receipt/identity/hash，保留缺 Darwin 和错配失败样例；Linux-only 不得标 full。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 本次未运行。 |
| [SC-7](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-7) | [生产 release policy 与授权](#build-release-policy-contract) | SC-7: 生产 runtime / node 入口必须默认关闭 builtin manifest fallback、本地 identity hash 签名、本地 finality signing 与 runtime source compile，保证 binary-only policy 是生产默认行为而不是测试显式开关。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；依原详细设计 §5.9 在真实 production/candidate 入口读取四个 effective policy 布尔值、验证应拒绝的 action/签名/fallback；`../../../crates/oasis7_runtime/src/world/mod.rs` 与 `../../../crates/oasis7_runtime/src/world/release_manifest.rs` 静态核对，不用构造器存在替代全部入口绑定。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 未重新核实全部生产入口、签名/最终性或发布状态。 本次未运行。 |
| [SC-8](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-8) | [Node-side proof assembly](#build-node-proof-contract) | SC-8: 跨宿主 Docker release evidence 必须可被包装成 node-side attestation proof payload，并进入 `ModuleReleaseSubmitAttestation.proof_cid`；仅存在于 CI artifact 的 report 不能单独视为生产 closure。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；按原详细设计 §5.8.5 的 module-release-node-attestation-flow.sh 生成 canonical proof_inputs/release_evidence_summary.json，再 package proof_payload.json / submit_request.json；固定同候选真实 summaries，检查缺失/错配 proof_cid 拒绝；`../module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust` 独立接收。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 仅验证设计；本次没有运行 proof assembly、签名、提交或共识验证。 本次未运行。 |
| [SC-9](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-9) | [Operator env 命名](#build-env-contract) | SC-9: Docker-first canonical build 链路的 operator env key 必须统一到 `OASIS7_WASM_*` 当前入口；不得再保留任何旧品牌前缀作为有效运行入口，避免 host wrapper、builder image、sync/check 与 build receipt 采集口径分叉。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；静态检查 wrapper、build suite、Dockerfile 的 OASIS7_WASM_* 读写；逐 sync/summary 入口核对旧前缀拒绝；`../../../crates/oasis7_wasm_materializer/src/lib.rs` 的 builtin_wasm_env_non_empty_reads_oasis7_prefix / rejects_removed_old_brand_prefix 仅对应 OASIS7_BUILTIN_WASM_*。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 命名检查不证明构建、可用性或发布权限。 本次未运行。 |
| [PRD-WORLD_RUNTIME-020](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-020) | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | PRD-WORLD_RUNTIME-020: As a `wasm_platform_engineer`, I want publishable WASM to be built only inside a pinned Docker builder image, so that host platform differences stop influencing release hashes. | `../../../scripts/build-wasm-module.sh`、`../../../docker/wasm-builder/Dockerfile` 与 [wrapper fake-Docker 回归](../../../scripts/build-wasm-module.test.sh)：检查缺 Docker、缺镜像、自定义镜像不匹配拒绝、retag 后仍使用选中 image ID；真实构建另在同候选 Docker 环境验证；核对 toolchain/rust-src/target/canonicalizer 版本及 build_manifest_hash 输入，失败日志在一次执行内包含 module/image/expected/actual 或容器入口原因。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 本次未运行。 |
| [PRD-WORLD_RUNTIME-021](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-021) | [Receipt、源码闭包与工件摘要](#build-receipt-contract) | PRD-WORLD_RUNTIME-021: As a 发布节点运营者, I want each artifact to carry a build receipt that binds builder image digest, source hash, build manifest hash, and canonical wasm hash, so that social verification no longer depends on “which laptop built it”. | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；`../../../tools/wasm_build_suite/src/lib.rs` 的 source_hash_tracks_local_path_dependency_changes、canonicalize_wasm_bytes_drops_all_custom_sections；`../../../crates/oasis7_wasm_build/src/lib.rs` 的 source_hash_changes_when_workspace_ancestor_lockfile_changes；同候选核对真实 receipt、输入与输出 hash，不用 fixture 代签真实工件。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 仅构建身份子范围；receipt 的 recipe digest 不独立证明执行镜像字节恒等。 本次未运行。 |
| [PRD-WORLD_RUNTIME-022](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-022) | [Builtin materialization 与 binary 消费](#build-materialization-contract) | PRD-WORLD_RUNTIME-022: As a `runtime_engineer` / `qa_engineer`, I want runtime to consume only Docker-canonical binaries and CI to compare Docker outputs across hosts, so that drift is blocked before execution. | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；按 `../../p2p/consensus/builtin-wasm-identity-consensus.design.md#builtin-identity-contract` 核对 source/build/identity 与实际 wasm hash；读取 `../../../crates/oasis7_wasm_materializer/src/lib.rs` 的 fetch/compile/cache 分支并在同候选验证远程错 hash 拒绝与本地 fallback；identity 与 release authority 另验。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | hash 校验不等于 release authorization；本地 fallback 与缓存不能选择 governing version。 本次未运行。 |
| [AC-1](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-1) | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | AC-1 (PRD-WORLD_RUNTIME-020): 必须新增并固定一份 WASM builder Docker image，镜像引用必须以 digest pin；所有 publishable wasm 构建都通过 `docker run` 进入该镜像。 | `../../../scripts/build-wasm-module.sh`、`../../../docker/wasm-builder/Dockerfile` 与 [wrapper fake-Docker 回归](../../../scripts/build-wasm-module.test.sh)：检查缺 Docker、缺镜像、自定义镜像不匹配拒绝、retag 后仍使用选中 image ID；真实构建另在同候选 Docker 环境验证；核对 toolchain/rust-src/target/canonicalizer 版本及 build_manifest_hash 输入，失败日志在一次执行内包含 module/image/expected/actual 或容器入口原因。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 本次未运行。 |
| [AC-2](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-2) | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | AC-2 (PRD-WORLD_RUNTIME-020): builder image 必须封装 Rust toolchain、`rust-src`、`wasm32-unknown-unknown` 目标、linker/canonicalizer 所需依赖，并把这些版本信息收敛到 `build_manifest_hash`。 | `../../../scripts/build-wasm-module.sh`、`../../../docker/wasm-builder/Dockerfile` 与 [wrapper fake-Docker 回归](../../../scripts/build-wasm-module.test.sh)：检查缺 Docker、缺镜像、自定义镜像不匹配拒绝、retag 后仍使用选中 image ID；真实构建另在同候选 Docker 环境验证；核对 toolchain/rust-src/target/canonicalizer 版本及 build_manifest_hash 输入，失败日志在一次执行内包含 module/image/expected/actual 或容器入口原因。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 本次未运行。 |
| [AC-3](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-3) | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | AC-3 (PRD-WORLD_RUNTIME-020): `scripts/build-wasm-module.sh` 的 canonical path 必须改为 Docker wrapper；publishable 构建不再保留 host-native cargo fallback。 | `../../../scripts/build-wasm-module.sh`、`../../../docker/wasm-builder/Dockerfile` 与 [wrapper fake-Docker 回归](../../../scripts/build-wasm-module.test.sh)：检查缺 Docker、缺镜像、自定义镜像不匹配拒绝、retag 后仍使用选中 image ID；真实构建另在同候选 Docker 环境验证；核对 toolchain/rust-src/target/canonicalizer 版本及 build_manifest_hash 输入，失败日志在一次执行内包含 module/image/expected/actual 或容器入口原因。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 本次未运行。 |
| [AC-4](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-4) | [单 canonical 发布 hash](#build-publish-hash-contract) | AC-4 (PRD-WORLD_RUNTIME-021): 发布级 hash manifest 目标态只允许写入单个 canonical token：`linux-x86_64=<sha256>`；`darwin-arm64` 不再作为发布 hash 来源。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；对同候选适用 m1/m4/m5 执行既有 sync --check，核对 --check 前后 tracked manifest/identity 未被改写、每个发布 manifest 仅有 linux-x86_64 token，并比对实际 packaged wasm、receipt 和 identity；`../../../scripts/ci-verify-m1-wasm-summaries.py` 核对 module_hashes/manifest_platform_hashes/receipt_evidence。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 不证明治理激活、签名或历史工件长期可用。 本次未运行。 |
| [AC-5](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-5) | [Receipt、源码闭包与工件摘要](#build-receipt-contract) | AC-5 (PRD-WORLD_RUNTIME-021): build receipt 至少绑定 `builder_image_digest + container_platform + source_hash(含本地 path 依赖闭包) + build_manifest_hash + wasm_hash + canonicalizer_version`，并进入 identity/release evidence。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；`../../../tools/wasm_build_suite/src/lib.rs` 的 source_hash_tracks_local_path_dependency_changes、canonicalize_wasm_bytes_drops_all_custom_sections；`../../../crates/oasis7_wasm_build/src/lib.rs` 的 source_hash_changes_when_workspace_ancestor_lockfile_changes；同候选核对真实 receipt、输入与输出 hash，不用 fixture 代签真实工件。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 仅构建身份子范围；receipt 的 recipe digest 不独立证明执行镜像字节恒等。 本次未运行。 |
| [AC-6](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-6) | [跨宿主 summary 与候选证据](#build-cross-host-contract) | AC-6 (PRD-WORLD_RUNTIME-022): multi-runner CI 必须比较“相同 Docker builder 在不同宿主上产出的 canonical hash”，而不是继续比较 host-native cargo 输出。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；同候选运行 `../../../.github/workflows/wasm-determinism-gate.yml` 与 `../../../.github/workflows/wasm-darwin-docker-evidence.yml`；用 `../../../scripts/ci-verify-m1-wasm-summaries.py` 比对适用模块及 receipt/identity/hash，保留缺 Darwin 和错配失败样例；Linux-only 不得标 full。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 本次未运行。 |
| [AC-7](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-7) | [Builtin materialization 与 binary 消费](#build-materialization-contract) | AC-7 (PRD-WORLD_RUNTIME-022): runtime 与节点执行路径默认只接受 canonical Docker build 产物；节点不通过重新编译源码参与执行合法性判断。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；按 `../../p2p/consensus/builtin-wasm-identity-consensus.design.md#builtin-identity-contract` 核对 source/build/identity 与实际 wasm hash；读取 `../../../crates/oasis7_wasm_materializer/src/lib.rs` 的 fetch/compile/cache 分支并在同候选验证远程错 hash 拒绝与本地 fallback；identity 与 release authority 另验。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | hash 校验不等于 release authorization；本地 fallback 与缓存不能选择 governing version。 本次未运行。 |
| [AC-8](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-8) | [生产 source compile 边界](#build-source-policy-contract) | AC-8 (PRD-WORLD_RUNTIME-022): `compile_module_artifact_from_source` 的生产路径必须迁移到外部 Docker builder 或直接禁用；runtime 进程内 host 直编只允许在 dev/test 模式下显式开启。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；在同候选 production 入口检查 effective ReleaseSecurityPolicy 并提交源码编译 action，要求拒绝；dev/test 显式放宽另测；`../../../crates/oasis7_runtime/src/world/mod.rs` 的 production_hardened 与 `../../../crates/oasis7_runtime/src/module_source_compiler.rs` 仅提供静态定位。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。 本次未运行。 |
| [AC-9](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-9) | [跨宿主 summary 与候选证据](#build-cross-host-contract) | AC-9 (PRD-WORLD_RUNTIME-021/022): 若 GitHub-hosted CI 因 runner 能力不足只能保留 Linux-only stable gate，PRD / project / evidence 报告必须把跨宿主 evidence 标记为 pending，直到导入 Docker-capable macOS summary 为止。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；同候选运行 `../../../.github/workflows/wasm-determinism-gate.yml` 与 `../../../.github/workflows/wasm-darwin-docker-evidence.yml`；用 `../../../scripts/ci-verify-m1-wasm-summaries.py` 比对适用模块及 receipt/identity/hash，保留缺 Darwin 和错配失败样例；Linux-only 不得标 full。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 本次未运行。 |
| [AC-10](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-10) | [生产 release policy 与授权](#build-release-policy-contract) | AC-10 (PRD-WORLD_RUNTIME-022): production 运行入口必须提供 release security policy 绑定证据，证明 fallback / 本地签名 / runtime source compile 默认关闭；仅在测试里调用 `enable_production_release_policy()` 不足以视为完成。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；依原详细设计 §5.9 在真实 production/candidate 入口读取四个 effective policy 布尔值、验证应拒绝的 action/签名/fallback；`../../../crates/oasis7_runtime/src/world/mod.rs` 与 `../../../crates/oasis7_runtime/src/world/release_manifest.rs` 静态核对，不用构造器存在替代全部入口绑定。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 未重新核实全部生产入口、签名/最终性或发布状态。 本次未运行。 |
| [AC-11](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-11) | [Node-side proof assembly](#build-node-proof-contract) | AC-11 (PRD-WORLD_RUNTIME-021/022): release evidence 除了 CI/report 汇总外，还必须存在 node-side proof payload 打包与 attestation submit 入口，使 `builder_image_digest/container_platform/canonicalizer_version` 可作为发布节点提交的正式证明字段进入共识链路。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；按原详细设计 §5.8.5 的 module-release-node-attestation-flow.sh 生成 canonical proof_inputs/release_evidence_summary.json，再 package proof_payload.json / submit_request.json；固定同候选真实 summaries，检查缺失/错配 proof_cid 拒绝；`../module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust` 独立接收。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 仅验证设计；本次没有运行 proof assembly、签名、提交或共识验证。 本次未运行。 |
| [AC-12](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-12) | [Operator env 命名](#build-env-contract) | AC-12 (PRD-WORLD_RUNTIME-020/021): `scripts/build-wasm-module.sh`、`scripts/sync-m1-builtin-wasm-artifacts.sh`、`scripts/ci-m1-wasm-summary.sh`、`tools/wasm_build_suite` 与 `docker/wasm-builder/Dockerfile` 必须只写入或读取 `OASIS7_WASM_*`；wrapper usage、错误提示、容器注入 env 与 build receipt 元数据采集不得再接受任何旧品牌前缀作为有效运行入口。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；静态检查 wrapper、build suite、Dockerfile 的 OASIS7_WASM_* 读写；逐 sync/summary 入口核对旧前缀拒绝；`../../../crates/oasis7_wasm_materializer/src/lib.rs` 的 builtin_wasm_env_non_empty_reads_oasis7_prefix / rejects_removed_old_brand_prefix 仅对应 OASIS7_BUILTIN_WASM_*。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 命名检查不证明构建、可用性或发布权限。 本次未运行。 |
| [AC-13](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-13) | [Operator env 命名](#build-env-contract) | AC-13 (PRD-WORLD_RUNTIME-021/022): builtin wasm materializer、release manifest fallback 与 DistFS root override 的 runtime env key 必须只读取 `OASIS7_BUILTIN_WASM_*`；Docker-first build 已完成品牌迁移后，runtime 取件/抓取/编译 fallback 不得继续接受任何旧品牌前缀作为有效运行入口。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；静态检查 wrapper、build suite、Dockerfile 的 OASIS7_WASM_* 读写；逐 sync/summary 入口核对旧前缀拒绝；`../../../crates/oasis7_wasm_materializer/src/lib.rs` 的 builtin_wasm_env_non_empty_reads_oasis7_prefix / rejects_removed_old_brand_prefix 仅对应 OASIS7_BUILTIN_WASM_*。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 命名检查不证明构建、可用性或发布权限。 本次未运行。 |
| [NFR-WDBP-1](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-1) | [跨宿主 summary 与候选证据](#build-cross-host-contract) | NFR-WDBP-1: 同一源码、同一 builder image digest、同一 `linux-x86_64` container platform 的 canonical wasm hash 可复现率必须为 `100%`。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；同候选运行 `../../../.github/workflows/wasm-determinism-gate.yml` 与 `../../../.github/workflows/wasm-darwin-docker-evidence.yml`；用 `../../../scripts/ci-verify-m1-wasm-summaries.py` 比对适用模块及 receipt/identity/hash，保留缺 Darwin 和错配失败样例；Linux-only 不得标 full。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 本次未运行。 |
| [NFR-WDBP-2](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-2) | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | NFR-WDBP-2: Docker canonical build 失败日志必须在一次执行内定位到 `builder_image_digest`、`module_id`、`expected`、`actual` 或容器入口失败原因。 | `../../../scripts/build-wasm-module.sh`、`../../../docker/wasm-builder/Dockerfile` 与 [wrapper fake-Docker 回归](../../../scripts/build-wasm-module.test.sh)：检查缺 Docker、缺镜像、自定义镜像不匹配拒绝、retag 后仍使用选中 image ID；真实构建另在同候选 Docker 环境验证；核对 toolchain/rust-src/target/canonicalizer 版本及 build_manifest_hash 输入，失败日志在一次执行内包含 module/image/expected/actual 或容器入口原因。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 本次未运行。 |
| [NFR-WDBP-3](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-3) | [单 canonical 发布 hash](#build-publish-hash-contract) | NFR-WDBP-3: 发布级 hash manifest 不再依赖宿主平台集合扩容；宿主平台差异不应影响发布 hash 空间。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；对同候选适用 m1/m4/m5 执行既有 sync --check，核对 --check 前后 tracked manifest/identity 未被改写、每个发布 manifest 仅有 linux-x86_64 token，并比对实际 packaged wasm、receipt 和 identity；`../../../scripts/ci-verify-m1-wasm-summaries.py` 核对 module_hashes/manifest_platform_hashes/receipt_evidence。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 不证明治理激活、签名或历史工件长期可用。 本次未运行。 |
| [NFR-WDBP-4](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-4) | [单 canonical 发布 hash](#build-publish-hash-contract) | NFR-WDBP-4: 本地默认路径在无显式授权时不得修改 tracked manifest / identity；CI 也不得写入。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；对同候选适用 m1/m4/m5 执行既有 sync --check，核对 --check 前后 tracked manifest/identity 未被改写、每个发布 manifest 仅有 linux-x86_64 token，并比对实际 packaged wasm、receipt 和 identity；`../../../scripts/ci-verify-m1-wasm-summaries.py` 核对 module_hashes/manifest_platform_hashes/receipt_evidence。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 不证明治理激活、签名或历史工件长期可用。 本次未运行。 |
| [NFR-WDBP-5](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-5) | [生产 source compile 边界](#build-source-policy-contract) | NFR-WDBP-5: 生产态 `ModuleSourcePackage` 不得要求 runtime 节点本地具备 Docker 才能执行共识路径。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；在同候选 production 入口检查 effective ReleaseSecurityPolicy 并提交源码编译 action，要求拒绝；dev/test 显式放宽另测；`../../../crates/oasis7_runtime/src/world/mod.rs` 的 production_hardened 与 `../../../crates/oasis7_runtime/src/module_source_compiler.rs` 仅提供静态定位。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。 本次未运行。 |
| [NFR-WDBP-6](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-6) | [跨宿主 summary 与候选证据](#build-cross-host-contract) | NFR-WDBP-6: 发布证据报告若缺少 `darwin-arm64` Docker canonical summary，必须在机器可读输出中明确标示 `cross_host_evidence_pending=true` 或同等阻断状态，避免把 Linux-only 结果误判为 full closure。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；同候选运行 `../../../.github/workflows/wasm-determinism-gate.yml` 与 `../../../.github/workflows/wasm-darwin-docker-evidence.yml`；用 `../../../scripts/ci-verify-m1-wasm-summaries.py` 比对适用模块及 receipt/identity/hash，保留缺 Darwin 和错配失败样例；Linux-only 不得标 full。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 本次未运行。 |
| [NFR-WDBP-7](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-nfr-wdbp-7) | [生产 release policy 与授权](#build-release-policy-contract) | NFR-WDBP-7: 生产入口 hardened release policy 的默认值必须可由自动化验证，不允许依赖人工约定或测试专用辅助调用。 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；依原详细设计 §5.9 在真实 production/candidate 入口读取四个 effective policy 布尔值、验证应拒绝的 action/签名/fallback；`../../../crates/oasis7_runtime/src/world/mod.rs` 与 `../../../crates/oasis7_runtime/src/world/release_manifest.rs` 静态核对，不用构造器存在替代全部入口绑定。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 未重新核实全部生产入口、签名/最终性或发布状态。 本次未运行。 |
| [BUILD-01](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-1) | [Receipt、源码闭包与工件摘要](#build-receipt-contract) | 固定 image/platform/source/tooling/wasm 身份与单 token；分项检验，不以 scope fixture 代签 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；`../../../tools/wasm_build_suite/src/lib.rs` 的 source_hash_tracks_local_path_dependency_changes、canonicalize_wasm_bytes_drops_all_custom_sections；`../../../crates/oasis7_wasm_build/src/lib.rs` 的 source_hash_changes_when_workspace_ancestor_lockfile_changes；同候选核对真实 receipt、输入与输出 hash，不用 fixture 代签真实工件。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 仅构建身份子范围；receipt 的 recipe digest 不独立证明执行镜像字节恒等。 本次未运行。 |
| [BUILD-02](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-2) | [跨宿主 summary 与候选证据](#build-cross-host-contract) | 同候选实际 Linux+Darwin Docker summaries；Linux-only 为 partial | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；同候选运行 `../../../.github/workflows/wasm-determinism-gate.yml` 与 `../../../.github/workflows/wasm-darwin-docker-evidence.yml`；用 `../../../scripts/ci-verify-m1-wasm-summaries.py` 比对适用模块及 receipt/identity/hash，保留缺 Darwin 和错配失败样例；Linux-only 不得标 full。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 本次未运行。 |
| [BUILD-03](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-3) | [生产 release policy 与授权](#build-release-policy-contract) | node proof、production source compile 与 materializer fallback 分别核对，不能互相代签 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；依原详细设计 §5.9 在真实 production/candidate 入口读取四个 effective policy 布尔值、验证应拒绝的 action/签名/fallback；`../../../crates/oasis7_runtime/src/world/mod.rs` 与 `../../../crates/oasis7_runtime/src/world/release_manifest.rs` 静态核对，不用构造器存在替代全部入口绑定。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 未重新核实全部生产入口、签名/最终性或发布状态。 本次未运行。 |
| [BUILD-01](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-1) | [Docker wrapper 与固定构建环境](#build-wrapper-contract) | 固定 Docker/platform，拒绝不可用 Docker 与自定义错误镜像；recipe 与执行 ID 分离 | `../../../scripts/build-wasm-module.sh`、`../../../docker/wasm-builder/Dockerfile` 与 [wrapper fake-Docker 回归](../../../scripts/build-wasm-module.test.sh)：检查缺 Docker、缺镜像、自定义镜像不匹配拒绝、retag 后仍使用选中 image ID；真实构建另在同候选 Docker 环境验证；核对 toolchain/rust-src/target/canonicalizer 版本及 build_manifest_hash 输入，失败日志在一次执行内包含 module/image/expected/actual 或容器入口原因。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | wrapper 编排与固定平台本身不独立新增玩家要求；工件可验证性另由工件关系承接。 本次未运行。 |
| [BUILD-01](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-1) | [单 canonical 发布 hash](#build-publish-hash-contract) | 只写一个 canonical linux-x86_64 token，拒绝输出/清单错配 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；对同候选适用 m1/m4/m5 执行既有 sync --check，核对 --check 前后 tracked manifest/identity 未被改写、每个发布 manifest 仅有 linux-x86_64 token，并比对实际 packaged wasm、receipt 和 identity；`../../../scripts/ci-verify-m1-wasm-summaries.py` 核对 module_hashes/manifest_platform_hashes/receipt_evidence。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 不证明治理激活、签名或历史工件长期可用。 本次未运行。 |
| [BUILD-03](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-3) | [生产 source compile 边界](#build-source-policy-contract) | production source compile 拒绝与 dev/test opt-in 独立验证 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；在同候选 production 入口检查 effective ReleaseSecurityPolicy 并提交源码编译 action，要求拒绝；dev/test 显式放宽另测；`../../../crates/oasis7_runtime/src/world/mod.rs` 的 production_hardened 与 `../../../crates/oasis7_runtime/src/module_source_compiler.rs` 仅提供静态定位。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。 本次未运行。 |
| [BUILD-03](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-3) | [Node-side proof assembly](#build-node-proof-contract) | 缺失/伪造 node proof 不得用 CI artifact 代签 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；按原详细设计 §5.8.5 的 module-release-node-attestation-flow.sh 生成 canonical proof_inputs/release_evidence_summary.json，再 package proof_payload.json / submit_request.json；固定同候选真实 summaries，检查缺失/错配 proof_cid 拒绝；`../module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust` 独立接收。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 仅验证设计；本次没有运行 proof assembly、签名、提交或共识验证。 本次未运行。 |
| [BUILD-03](wasm-deterministic-build-pipeline.prd.md#sr2-domain-case-3) | [Builtin materialization 与 binary 消费](#build-materialization-contract) | materializer fetch/compile/cache 与 action-level policy 分开核对 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；按 `../../p2p/consensus/builtin-wasm-identity-consensus.design.md#builtin-identity-contract` 核对 source/build/identity 与实际 wasm hash；读取 `../../../crates/oasis7_wasm_materializer/src/lib.rs` 的 fetch/compile/cache 分支并在同候选验证远程错 hash 拒绝与本地 fallback；identity 与 release authority 另验。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | hash 校验不等于 release authorization；本地 fallback 与缓存不能选择 governing version。 本次未运行。 |
| [PRD-WORLD_RUNTIME-022](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-022) | [跨宿主 summary 与候选证据](#build-cross-host-contract) | 同候选真实跨宿主 Docker compare，缺 Darwin 为 pending | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；同候选运行 `../../../.github/workflows/wasm-determinism-gate.yml` 与 `../../../.github/workflows/wasm-darwin-docker-evidence.yml`；用 `../../../scripts/ci-verify-m1-wasm-summaries.py` 比对适用模块及 receipt/identity/hash，保留缺 Darwin 和错配失败样例；Linux-only 不得标 full。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 跨宿主工件一致性只是确定性执行输入子范围，不代签验证者重执行、finality 或 replay。 本次未运行。 |
| [PRD-WORLD_RUNTIME-022](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-022) | [生产 source compile 边界](#build-source-policy-contract) | runtime 消费合法 binary；生产源码编译必须外移或关闭 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；在同候选 production 入口检查 effective ReleaseSecurityPolicy 并提交源码编译 action，要求拒绝；dev/test 显式放宽另测；`../../../crates/oasis7_runtime/src/world/mod.rs` 的 production_hardened 与 `../../../crates/oasis7_runtime/src/module_source_compiler.rs` 仅提供静态定位。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 未逐入口运行拒绝验证；source compile-off 不覆盖 builtin materializer fallback。 本次未运行。 |
| [PRD-WORLD_RUNTIME-022](wasm-deterministic-build-pipeline.prd.md#sr2-obligation-prd-world_runtime-022) | [生产 release policy 与授权](#build-release-policy-contract) | 生产入口有效策略与拒绝行为不可由测试辅助调用代签 | [S11 验证与证据入口](../../../testing-manual.md#testing-decentralized-module-release-runtime-alerts)；依原详细设计 §5.9 在真实 production/candidate 入口读取四个 effective policy 布尔值、验证应拒绝的 action/签名/fallback；`../../../crates/oasis7_runtime/src/world/mod.rs` 与 `../../../crates/oasis7_runtime/src/world/release_manifest.rs` 静态核对，不用构造器存在替代全部入口绑定。 | 同候选命令/退出码、配置、真实输入与输出 artifact；本次仅静态核读 | 未重新核实全部生产入口、签名/最终性或发布状态。 本次未运行。 |

## 12. 决策、长期风险与未决问题

- 继续单 canonical 发布 hash 与 binary-first runtime；保留现有 source compile、release policy 与 materializer 的独立边界。
- recipe digest 与实际执行 image ID 的差异仍须 builder/release owner 在改变镜像或发布政策前处理；不能用配置相等、源码测试定义或本次文档检查裁定目标已满足。
- 新候选、镜像/toolchain/canonicalizer/source closure、production 入口或 module set 变化时，QA 与各 owner 重核适用证据。历史 run/阶段完成说明不移作新候选证明。
- DWE-001 的全部验证者/权威结果与 DWE-004 的 activation、历史 replay、工件长期可用性仍由既有 runtime/P2P/QA 组合合同接收；本文不扩大局部工件证据的结论。

## 原详细设计与历史记录

以下保留原流程、字段、策略矩阵和历史证据；当前静态差距和证据限制见前部，不以旧阶段“已完成”代签当前候选。

- 对应需求文档: `doc/world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md`
- 稳定证据入口: `doc/world-runtime/wasm/evidence.md`

审计轮次: 3

## 1. 设计定位
本设计把 oasis7 的 WASM 发布级构建从“宿主机护栏 + keyed hash 对账”升级为“Docker canonical builder”。设计目标不是否定现有脚本和 build suite，而是把它们放进同一个 pinned 容器镜像里，让宿主平台不再参与发布 hash 的生成。

实施任务、候选状态和 CI run 历史不在本文维护；它们由 GitHub task issue /
Project 和 Git history 追溯。可长期复用的验证入口收敛在
[`evidence.md`](evidence.md)。

## 2. 现状盘点

以下为原 2026-03-18 盘点及其后标注的历史补充；不是本次冻结候选的执行结论。当前静态差距以本文前部为准，历史 run 不移用为当前 candidate 的跨宿主或生产证明。

| 层级 | 当前实现事实（2026-03-18） | 与 Docker-first 目标的差距 |
| --- | --- | --- |
| 容器基础设施 | `docker/wasm-builder/Dockerfile`、`scripts/build-wasm-module.sh` 与 canonical builder digest 已落地。 | canonical builder 已存在，跨宿主证据已由 self-hosted Darwin Docker workflow 产出；repo 内保留 run/job 索引，不提交 workflow artifact 二进制。 |
| 构建入口 | `scripts/build-wasm-module.sh` 已强制 `docker run --platform linux/amd64`，且显式拒绝 host-native fallback。 | 入口已收敛，但仍需把“stable gate”和“cross-host full closure”严格区分。 |
| 构建工具 | `tools/wasm_build_suite` 已在容器内输出 `build receipt`、`source_hash`、`build_manifest_hash`、`builder_image_digest` 与 `container_platform`。 | receipt 语义基本完成，剩余重点在 evidence 汇总与 release gate 结论。 |
| manifest 策略 | builtin hash manifest 已改为单 canonical token `linux-x86_64=<sha256>`；identity 生成已切换为 receipt 驱动。 | 写路径已收敛，cross-host report 已拿到真实 `darwin-arm64` Docker evidence。 |
| source compile | `compile_module_artifact_from_source` 仍保留源码包编译能力，但 production `ReleaseSecurityPolicy` 已默认拒绝该 action；仅 dev/test 可显式使用。 | external builder worker 仍未落地，当前以 production default-disable 先收敛 runtime 权限面。 |
| CI 校验 | `.github/workflows/wasm-determinism-gate.yml` 当前已收敛为 GitHub-hosted `linux-x86_64` stable gate，并支持导入外部 summaries；`.github/workflows/wasm-darwin-docker-evidence.yml` 可手动调度 self-hosted Darwin Docker runner 产出真实 `darwin-arm64` bundle 并触发 Linux cross-host verify。 | `WDBP-3` 的真实 `linux-x86_64 + darwin-arm64` evidence 已在 main run `28297899706` 产出；GitHub-hosted macOS 仍不作为 Docker evidence producer。 |
| runtime 消费策略 | runtime 侧已支持 production policy 关闭 source compile / local signing / builtin fallback，且 production-facing 入口已补 hardened policy 绑定证据。 | WDBP-3.3 已完成；后续风险只保留入口扩展时的回归防护。 |

## 3. 设计原则
- 原则-1：publish hash 只来自 canonical container，不来自宿主机。
- 原则-2：builder image digest 是构建身份的一部分，必须进入 receipt 与审计链路。
- 原则-3：runtime 节点是 binary consumer，不是 source builder。
- 原则-4：CI 验证 Docker reproducibility，不拥有生产 manifest 写权限。
- 原则-5：source compile 要么走同一外部 Docker builder，要么明确降级为 dev/test only。

## 4. 目标态架构

```text
repo source / ModuleSourcePackage
          |
          v
host wrapper
scripts/build-wasm-module.sh
          |
          v
docker run --platform linux/amd64
builder-image@sha256:...
          |
          v
containerized wasm_build_suite
          |
          v
canonical packaged wasm
          |
          +------------------------------+
          |                              |
          v                              v
     build receipt                identity / release evidence
          |                              |
          +--------------+---------------+
                         v
        single canonical publish hash
      (manifest token: linux-x86_64=<sha256>)
                         |
                         v
               DistFS / release manifest
                         |
                         v
              runtime fetch -> verify -> execute
```

关键变化：
- 宿主平台不再直接生成发布 hash。
- `linux-x86_64` 容器平台成为唯一 canonical publish 平台。
- keyed manifest 从“多宿主发布 hash 集合”收敛为“单 canonical token”。

## 5. 详细设计

<a id="build-materialization-detail"></a>
### 5.0 Builtin 工件 materialization

- `m1/m4/m5` 的 canonical binary 经 sync/check 与 SHA-256 manifest 对账后，由 `hydrate_builtin_wasm` 写入按 SHA-256 配置的 `LocalCasStore`。
- materializer 读取 DistFS candidate 后复算 SHA-256；本地候选不可用时，当前实现仍会继续 fetch 与 builtin source-compile fallback。materializer 不接收 identity、receipt 或 release policy。
- DistFS 只承载 binary materialization/cache，不改变 canonical build/identity 的权威关系。identity/receipt/signature/release policy 由外围 artifact selection / release-gated pipeline 校验；`Action::CompileModuleArtifactFromSource` 的 production policy gate 不覆盖 builtin materializer fallback，不能据此声明生产入口已经关闭后者。

<a id="build-image-detail"></a>
### 5.1 Canonical Builder Image
目标新增一份固定 builder image，例如：
- `docker/wasm-builder/Dockerfile`
- 镜像通过 digest pin 引用，而不是仅用 tag。

镜像内必须固定：
- Rust toolchain 版本
- `rust-src`
- `wasm32-unknown-unknown`
- canonical packaging 所需依赖
- `tools/wasm_build_suite`
- 统一 workspace 路径，例如 `/workspace`

设计要求：
- builder image 是发布级构建环境的真源。
- `build_manifest_hash` 的输入至少包含：
  - `builder_image_digest`
  - `container_platform=linux-x86_64`
  - build profile
  - canonicalizer version
  - build suite version / contract version

<a id="build-wrapper-detail"></a>
### 5.2 Host Wrapper
`scripts/build-wasm-module.sh` 的职责要改写为：
- 校验 Docker 可用。
- 组装 `docker run` 参数。
- 绑定只读源码挂载与可写输出目录。
- 把 `module_id`、`manifest_path`、`profile` 转交给容器内 build suite。

约束改为：
- 不再保留 host-native fallback。
- Docker 不可用时直接失败，避免任何宿主直编结果进入发布链路。
- wrapper 只接受同一 workspace root 下的 `manifest_path + out_dir`，并统一映射到容器内 `/workspace`。

<a id="build-suite-detail"></a>
### 5.3 Containerized Build Suite
`tools/wasm_build_suite` 不需要被替换，但需要重新定位：
- 过去：host 工具。
- 未来：container-internal canonical builder。

保留能力：
- `cargo metadata/build --locked`
- workspace `build.rs` / `proc-macro` guard
- custom section stripping
- metadata 输出

新增能力：
- 输出 `build receipt`
- receipt 至少包含：
  - `builder_image_ref`
  - `builder_image_digest`
  - `container_platform`
  - `module_id`
  - `source_manifest_path`
  - `source_hash`
  - `build_manifest_hash`
  - `canonicalizer_version`
  - `wasm_hash_sha256`
  - `wasm_size_bytes`

<a id="build-manifest-detail"></a>
### 5.4 Manifest / Identity Migration
当前 manifest 的主要问题是把宿主平台差异直接放进发布清单。

Docker-first 目标态：
- 发布级 manifest 只保留一个 token：
  - `linux-x86_64=<sha256>`
- 这里的 `linux-x86_64` 表示 canonical builder container platform，而不是要求宿主机必须是 Linux。

迁移策略：
1. 读路径先兼容多 token。
2. 写路径改为只写 canonical token。
3. CI 阻止新的 `darwin-arm64` 发布 token 进入仓库。
4. release manifest / identity / attestation 逐步只引用 canonical token。

Identity manifest 需要扩展：
- 从“source hash + build manifest hash + hash tokens”
- 升级为“source hash + build manifest hash + builder image digest + canonical token”

<a id="build-source-detail"></a>
### 5.5 Source Package Compile Policy
这是本设计最大的边界调整。

当前问题：
- `compile_module_artifact_from_source` 仍存在于 runtime 代码面，但 production 路径已经不能再直接执行它。
- external builder worker 还未落地，因此当前阶段通过 production policy default-disable 来阻断 Docker daemon 进入 runtime 热路径。

当前落地态：
- 生产态 source compile 已被 `ReleaseSecurityPolicy` 默认禁用。
- runtime 生产路径只接收已经构建好的 wasm artifact；源码包编译 action 会被拒绝并要求使用 external Docker builder。
- dev/test 仍可显式使用该路径，便于保留现有回归与实验工作流。

推荐两段式流程：
1. runtime / 发布层提交 `ModuleSourcePackage`
2. external builder worker 调用 Docker canonical builder，返回：
   - `wasm_bytes`
   - `source_bundle_hash`
   - `build receipt`

这样 runtime 继续是 binary-first consumer。

<a id="build-ci-detail"></a>
### 5.6 CI And Release Verification
CI 需要改成比较容器输出，而不是比较 host-native 输出。

目标 workflow：
1. macOS runner 安装 Docker Desktop / 可用 Docker CLI。
2. Linux runner 安装 Docker。
3. 两边都运行同一个 wrapper script。
4. 两边 summary 都导出 canonical container hash。
5. compare job 要求两边 hash 完全一致。

release gate 需要新增的固定结论：
- `builder image digest matched`
- `canonical token matched`
- `docker-only path enforced`
- `cross-host evidence complete` 或 `cross-host evidence pending`
- `production release policy hardened by default`

<a id="build-consumption-detail"></a>
### 5.7 Runtime Consumption
runtime 的最终消费模型不变，仍是 binary-first：
1. 从 release manifest / DistFS 获取 binary。
2. 校验 canonical hash。
3. 校验 identity / signature / receipt 绑定。
4. 注册 artifact 并执行。

真正变化的是：
- canonical hash 现在来自 Docker builder。
- runtime 不再需要解释多个宿主平台发布 hash。

<a id="build-cross-host-detail"></a>
### 5.8 Cross-Host Evidence Closure Design
这是 `WDBP-3` 曾经的 P0 剩余切片；历史记录由 self-hosted Darwin Docker runner 与 main run `28297899706` 收口。本节保留 stable gate 与 full-tier evidence 的分层规则，防止后续把 GitHub-hosted Linux-only gate 误写成跨宿主 closure。

#### 5.8.1 双层 gate 模型
- `stable gate`
  - 运行环境：GitHub-hosted `ubuntu-24.04`
  - 目的：持续验证 canonical builder、receipt、identity、single token、report 脚本本身没有回退。
  - 结论上限：`linux-only stable`
- `full-tier cross-host evidence`
  - 运行环境：`linux-x86_64` + 至少一条 Docker-capable `darwin-arm64`
  - 目的：验证“相同 builder image digest + 相同源码输入”在真实跨宿主 Docker 环境下仍收敛到同一 canonical hash。
  - 结论上限：`cross-host closed`

设计约束：
- 任何缺少 `darwin-arm64` Docker canonical summary 的发布候选，都只能得到 `stable gate passed / cross-host pending`。
- 不允许把 `linux-x86_64` 单宿主结果写成 `SC-1 fulfilled`。

#### 5.8.2 Summary Import Contract
`scripts/wasm-release-evidence-report.sh` 的导入语义需要成为正式证据协议，而不是临时绕行。

外部 Docker-capable runner 推荐先产出一个标准 bundle，再进入 verify/report：
- `scripts/package-wasm-summary-bundle.sh`
  - 负责把 `m1/m4/m5` summary 规范化为同一 bundle 目录或 `.tar.gz`
- `scripts/stage-wasm-summary-imports.sh`
  - 负责把 GitHub-hosted Linux summary 与外部 bundle 合并到同一 verify 输入目录

每个导入 summary 必须至少包含：
- `runner_label`
- `host_platform`
- `canonical_platform=linux-x86_64`
- `builder_image_digest`
- `canonicalizer_version`
- `module_set`
- `module -> wasm_hash`
- `receipt_evidence`

外部 bundle manifest 至少包含：
- `schema_version`
- `runner_label`
- `host_platform`
- `module_sets`
- `summary_files`

report 聚合时必须输出：
- `expected_runners`
- `received_runners`
- `missing_runners`
- `cross_host_evidence_pending`
- `canonical_hash_consistent`
- `receipt_evidence_consistent`

推荐 machine-readable 结论：

```json
{
  "module_set": "m1",
  "stable_gate_passed": true,
  "cross_host_evidence_pending": false,
  "expected_runners": ["linux-x86_64", "darwin-arm64"],
  "received_runners": ["linux-x86_64", "darwin-arm64"],
  "gate_result": "cross-host-closed"
}
```

#### 5.8.3 External Evidence Dispatch Runbook
真实 Docker-capable `darwin-arm64` runner 可用后，full-tier 证据产出流程固定为：

优先使用手动 CI 路径：

1. 在 GitHub Actions 手动触发 `Wasm Darwin Docker Evidence`。
2. `runs_on_json` 填写真实 Docker-capable macOS ARM64 self-hosted runner labels，默认值为 `["self-hosted","macOS","ARM64","docker"]`。
3. `collect-darwin-docker-summaries` job 会做 `Darwin arm64 + docker linux/amd64` preflight，并产出 `darwin-arm64-wasm-summary-bundle` artifact。
4. `verify-with-linux-summaries` job 会下载该 artifact、收集 `linux-x86_64` summaries、导入 darwin bundle，并产出 `darwin-arm64-wasm-release-evidence-report` artifact。
5. 该 report 的 `summary.json` 满足下方 closure 条件后，才可把 `WDBP-3.2` 视为完成。

历史正式收口证据（不代表当前候选）：`Wasm Darwin Docker Evidence` main run `28297899706` 已通过；`collect-darwin-docker-summaries` job `83840926310` 在 self-hosted Darwin runner 上产出 bundle，`verify-with-linux-summaries` job `83843884654` 完成 Linux summary 收集、Darwin bundle 导入与 cross-host report 上传。

如果 self-hosted runner 不能直接接入仓库 workflow，则使用外部 dispatch 路径：

1. 在外部 runner 上收集 `m1/m4/m5` summary，并生成标准 bundle：
   - `./scripts/package-wasm-summary-bundle.sh --out-dir <bundle-dir> --archive <bundle.tar.gz> --runner-label darwin-arm64`
2. 将 `<bundle.tar.gz>` 上传到 GitHub Actions runner 可访问的 URL（例如 release asset、对象存储或受控静态下载地址）。
3. 在仓库内触发 `Wasm Determinism Gate` workflow_dispatch：
   - `./scripts/dispatch-wasm-determinism-gate.sh --bundle-url <https-url> --runner-label darwin-arm64`
4. verify job 会自动执行：
   - 下载 GitHub-hosted `linux-x86_64` summaries
   - `stage-wasm-summary-imports.sh` 合并本地 Linux summary 与外部 bundle
   - `wasm-release-evidence-report.sh` 生成 `summary.md/json`
5. 只有当 `summary.json` 同时满足以下条件时，才可把 `WDBP-3.2` 视为完成：
   - `received_runners` 覆盖 `linux-x86_64,darwin-arm64`
   - `cross_host_evidence_pending=false`
   - `gate_result=cross-host-closed`
   - `canonical_hash_consistent=true`
   - `receipt_evidence_consistent=true`

交付约束：
- bundle URL 必须可被 GitHub-hosted verify job 直接下载。
- 外部 runner 必须实际跑 Docker canonical builder，不能只转存 host-native 结果。
- 正式 closure 证据应记录 workflow run URL、job id 与对应 report artifact 名称；workflow artifacts 受 GitHub Actions retention 管理，不作为 repo-tracked binary 长期归档。

#### 5.8.4 证据来源分层
为避免把开发回归、CI 回归、生产候选证据混写，evidence source 需要分层：
- `ci-hosted-linux`
- `external-builder-macos`
- `release-node-attestation`

排序原则：
1. 先验证所有 source 的 `builder_image_digest`、`build_manifest_hash`、`canonicalizer_version` 一致。
2. 再比较 canonical wasm hash。
3. 最后才允许给出 `cross-host closed`。

若任一步失败：
- 保留已有 Linux stable gate 结果；
- 将 cross-host 状态标为 `blocked`；
- 不回滚到 host-keyed manifest。

<a id="build-proof-detail"></a>
#### 5.8.5 Node-Side Proof Assembly
CI/report 只能提供开发期或候选期证据；真正进入 `ModuleReleaseSubmitAttestation.proof_cid` 的 payload 需要由发布节点本地重新装配。

固定入口：
- `scripts/module-release-node-attestation-flow.sh`

节点侧固定流程：
1. 从本机收集 summary，或导入预收集 summary / 外部 bundle。
2. 运行 `scripts/wasm-release-evidence-report.sh` 做多 runner verify，输出人读/机读报告。
3. 将报告依赖的 per-runner summary 做 canonicalize，剥离 `generated_at_utc`、本地路径、run dir 等非语义字段。
4. 生成稳定的 `proof_inputs/release_evidence_summary.json`，只保留：
   - `required_runners / expected_runners / received_runners`
   - `stable_gate_passed / cross_host_evidence_pending / cross_host_closed / gate_result`
   - 每个 `module_set` 的 gate 结论与 canonical summary 文件 `sha256`
5. 再调用 `scripts/package-module-release-attestation-proof.sh` 生成正式 `proof_payload.json + submit_request.json`。
6. 如需直接入链，再调用 `scripts/submit-module-release-attestation.sh`。

设计约束：
- `proof_cid` 不得依赖 report 运行时目录、日志时间戳或宿主机临时路径。
- 人读报告与 verify log 可以保留在 run dir 内供排障，但默认不进入 proof payload 的 canonical evidence 集。
- 发布节点可选择 `--require-cross-host-closed`，在 full-tier 候选阶段把 `conditional-go` 直接升级为阻断。

<a id="build-policy-detail"></a>
### 5.9 Production Release Policy Binding
这是 `WDBP-3.3` 的设计边界；原阶段记录 production-facing 入口已补 hardened policy 绑定证据，本节保留为后续入口扩展时的约束。

#### 5.9.1 策略矩阵

| 运行形态 | `allow_builtin_manifest_fallback` | `allow_identity_hash_signature` | `allow_local_finality_signing` | `allow_runtime_source_compile` | 预期用途 |
| --- | --- | --- | --- | --- | --- |
| dev | `true` | `true` | `true` | `true` | 本地调试、实验工作流 |
| test | `true` 或按用例覆写 | `true` 或按用例覆写 | `true` 或按用例覆写 | `true` 或按用例覆写 | 定向回归、拒绝路径测试 |
| production | `false` | `false` | `false` | `false` | 节点执行、发布候选、线上验收 |

设计要求：
- production 不是“调用方约定”；必须在主运行入口自动绑定。
- dev/test 的放宽必须是显式 opt-in，不能继续复用默认值伪装成产品路径。

#### 5.9.2 主运行入口绑定点
本轮设计要求至少覆盖以下入口：
- `oasis7_chain_runtime`
- 任何由 launcher 拉起的 chain runtime 生产路径
- runtime 相关 release / acceptance 脚本入口

绑定策略：
1. 入口解析出运行模式或发布配置。
2. 若模式属于 release / prod / candidate，创建 `World` 后立即应用 hardened policy。
3. 在 status / evidence 输出中打印实际生效的四个布尔值。
4. 若入口没有进入 hardened policy，不允许给出 production-ready 结论。

#### 5.9.3 可验证证据面
除了行为拒绝测试，还需要有显式配置证据：
- `status.json` / `summary.md` / release gate 报告中写出 effective policy
- 节点验收脚本断言四个布尔值均为 `false`
- 若任一值为 `true`，报告必须输出 `production_release_policy_not_hardened`

推荐 evidence 字段：

```json
{
  "release_security_policy": {
    "allow_builtin_manifest_fallback": false,
    "allow_identity_hash_signature": false,
    "allow_local_finality_signing": false,
    "allow_runtime_source_compile": false
  }
}
```

#### 5.9.4 与 source compile gate 的关系
`WDBP-4` 解决的是“source compile 在 production 默认被拒绝”；原 `WDBP-3.3` 阶段记录把同类 hardened policy 绑定至主入口并写出证据；当前候选仍须按本节复核。

边界：
- `WDBP-4` 不再新增 source compile 业务设计。
- `WDBP-3` 只负责入口绑定与证据化，不改变 source compile reject 语义本身。

## 6. 角色分工

| 角色 | 负责内容 |
| --- | --- |
| `producer_system_designer` | 明确“容器解决漂移、runtime 不再直接编译源码”的目标边界 |
| `wasm_platform_engineer` | builder image、wrapper、receipt、manifest migration、cross-host evidence 协议与报告字段 |
| `runtime_engineer` | source compile 外移或 gating、release manifest 消费、production entry hardened policy 绑定 |
| `qa_engineer` | multi-runner Docker compare、失败签名、stable/full-tier 结论区分、policy 绑定复验 |

## 7. 失败模型与阻断点

| 失败点 | 触发位置 | 阻断行为 |
| --- | --- | --- |
| Docker 不可用 | host wrapper | 直接失败，不允许回退成发布级 native build |
| image digest 未 pin 或不匹配 | wrapper / receipt verify | 阻断发布 |
| container output 与 tracked canonical token 不一致 | sync/check | 阻断并报告 `module_id + expected + actual` |
| macOS/Linux 跑同一容器得出不同 hash | CI compare | 阻断并归类为 builder reproducibility defect |
| production runtime 仍启用 host source compile | runtime config gate | 启动即拒绝或 action rejected |
| GitHub-hosted gate 仅有 Linux summary | release evidence report | 允许 stable gate 通过，但 cross-host 结论必须保持 `pending` |
| production entry 未绑定 hardened release policy | runtime status / acceptance script | 直接 `no-go`，不得以测试辅助调用替代 |

## 8. 迁移计划
- M0：修正文档为 Docker-first 目标态。
- M1：新增 builder image 与 wrapper，保留读路径兼容。
- M2：manifest/identity 只写 canonical token。
- M3：runtime source compile 外移到 external builder 或 production 默认禁用。
- M4：CI / release gate 全量切换到 Docker reproducibility compare，并引入 stable gate 与 full-tier evidence 分层。
- M5：主运行入口默认绑定 hardened release policy，并把 effective policy 输出到 release evidence / acceptance report。

## 9. 设计边界
- Docker 只解决发布级构建确定性，不进入 wasm 执行期 sandbox。
- 本设计不要求立即删除现有 build suite；它是容器内核心执行器。
- 本设计不把 CI 变成生产发布者；CI 只是运行同一容器镜像做验证。

## 10. Source package guardrail 与生产边界

- 生产 runtime 不执行 host source compile，也不回退到仓库 build script；它只消费 external Docker canonical builder 产生的 binary、manifest 与 receipt。
- dev/test 显式 opt-in 的 source package 入口必须限制文件数量、单文件/总字节数和相对路径，拒绝绝对路径与 `..`，在隔离临时目录中以最小环境和明确 timeout 执行。
- dev/test 编译失败返回结构化 policy/validation 原因；这些 guardrail 只降低测试工具污染风险，不等价于 OS container、seccomp 或生产级 sandbox。
- 任何面向发布的产物仍必须重新进入 canonical builder、hash/identity/receipt 校验链；dev/test compile 结果不能直接提升为 publish authority。
