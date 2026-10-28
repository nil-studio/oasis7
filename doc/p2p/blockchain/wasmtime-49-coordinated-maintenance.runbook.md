<a id="wasmtime-49-maintenance"></a>
# Wasmtime 49 引擎升级协调维护流程

本分册承接 [governed bootstrap Phase C0](public-testnet-governed-bootstrap.runbook.md#85-phase-c0---routine-ci-package-update) 的引擎特定维护限制；该 runbook 仍是 public-testnet topology/deployment/rollout/recovery authority。本分册不授予部署批准。

本节只适用于 [执行器兼容说明](../../world-runtime/wasm/wasm-executor.design.md#wasmtime-49-coordinated-release) 所述升级：结构化失败进入 tick hash，禁止同一 world 的不同引擎目标版本进程重叠 author 或独立 execute/validate tick。当前无自动 fleet fence；该升级不适用 Phase C0 的运行中 validator-first/observer-later 顺序或 Linux/Windows/macOS 自动重启、单节点回滚路径。

现有 identity/checkpoint/closure/admission/recovery gates 全部保留；若 wrapper 需要运行中的旧 provider、无法在全停条件完成校验或 hold 自动启动/回滚，保持 `hold` 并另取独立授权且审读的执行计划，不得绕过 wrapper 或降低 gates。PR 合入不授权部署或证明现场安全；缺证阻断部署，合入仍由独立 review/完整 CI 裁定。

1. 授权 coordinator 固定完整 host/service/process 执行 inventory，涵盖 validators、sequencers 及实际执行验证的 observers、embedded/full nodes；未知执行者或不可控 restart/admission 来源使维护保持 `hold`。记录目标 source commit、各 platform package/buildinfo/hash、lockfile 精确 Wasmtime 版本 binding、共同 committed parent height/hash 与 backup/checkpoint；跨平台须同 source/dependency，不要求 binary SHA 相同。

2. 任何目标启动前停止整个执行集合，hold 自动 restart/admission，逐项记录 service stopped 与无残留执行进程。流量暂停或 head 不增长不足以证明 quiescence，因为无新 intent 时仍可执行 tick。

3. 全量保持停止时安装/切换已验证目标工件；`scripts/p2p-public-testnet-package-node-upgrade.sh` 的 `--restart-service` 禁用，不带该选项仍须独立证明 quiescence，且 primitive 不能替代 observer governed wrapper/identity/closure gates。完整回读每个 host 的 resolved current path、executable hash/buildinfo 与 source/Wasmtime binding；任一缺失或失败保持全停。

4. 全量回读通过后只恢复已回读的精确目标版本，阻止旧版/未验证进程加入；在共同新高度记录 execution block/hash、可观测 events_hash、state root、health/readiness/peer 收敛及 runtime/QA same-candidate deterministic execution 验证。service active、height 前进、`p2p-upgrade-preflight.sh` rolling envelope 或 package-rollout provider checkpoint gate 均不能豁免边界。inventory、stop/process/restart hold、回读、恢复顺序与收敛证据进入 task evidence；历史同 SHA 最终状态不证明本次无重叠。

5. 恢复前失败保持全停，或全部回切旧工件并完整核验后统一启动；目标执行后失败先停整个集合，由 runtime/QA 判定 fleet rollback/recovery，禁止单节点自动回滚旧引擎加入目标 world。保留 confirmed history，遵循 journal/checkpoint 与 Phase E/F/H signed recovery/clean-redeploy 合同；backup/checkpoint 不许可复制节点数据、恢复旧状态或跨节点 copy，unsupported rollback 保持 hold，本节不授权删除/重建/历史改写。

