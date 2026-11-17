# Aliyun Tablestore SDK 有界维护补丁

## 来源与许可

完整导入 crates.io `aliyun-tablestore-rs` 0.1.2 发布归档，未删除协议、模型、protobuf、示例或原测试。保留原 MIT LICENSE 与版权。归档地址：
https://static.crates.io/crates/aliyun-tablestore-rs/aliyun-tablestore-rs-0.1.2.crate

归档 SHA-256：`2b4ed7ec37de593bb94baf7f0fa7589af4cc525542d5b3ab49b653ca9601e85c`，与原 workspace Cargo.lock 的 registry checksum 一致；`.cargo_vcs_info.json` 记录上游 commit `c6ac74635658258ad4aad15a92078e0b515a0b34`。`UPSTREAM.sha256.json` 保存归档中每一原文件的原始哈希。Cargo.toml.orig 与 vendor 内 Cargo.lock 保留发布原件用于对照；实际消费以根 workspace Cargo.lock 和本目录修改后的 Cargo.toml 为准。

## 补丁与兼容边界

RUSTSEC-2026-0543 表示 chrono 所有版本未维护，未设置 advisory ignore 或降低门禁。仅将 SDK 私有 `util.rs` 两个时钟格式化函数迁到已锁定 time 0.3.55；请求签名、HTTP、重试、公开 API、序列化及服务语义不变。SDK 的公开类型/签名没有 chrono。UTC、负 epoch、毫秒截断、英文星期/月及年份补零规则保留；私有 helper 的 time 可表示范围不同于 chrono，不声称扩展年份/闰秒 API 全域等价，正式入口仍仅生成当前 UTC 时间。所用 time 最低 Rust 1.88，诚实更新本维护副本 rust-version；Oasis7 当前 canonical Rust 1.96 满足它。

补丁文件：Cargo.toml（依赖和 MSRV）、src/util.rs（调用私有格式化模块）、新增 src/time_format.rs；其余原文件逐字保留。不改只读 third_party；使用已有顶层 vendor + patch.crates-io 模式并从 workspace members 排除此第三方包，避免误运行其依赖真实云凭据的原测试。

## 实际验证入口

`oasis7_launcher_backend` 现有 `oasis7_game_launcher` 普通测试 target 直接通过 path 编译同一个 src/time_format.rs，覆盖 OTS/HTTP 日期及固定请求 HMAC。真实 SDK OtsClient 另向有界本地 TCP fixture 发 ListTable，验证实际日期、签名和 protobuf 响应。不是仅放在依赖包里而不会执行的测试；不需要真实云账号，不认证云服务能力。

Canonical（资源允许时）：
`env -u RUSTC_WRAPPER cargo test --locked -p oasis7_launcher_backend --bin oasis7_game_launcher tablestore_sdk_time_tests`

## 维护与退出

后续每次更新记录发布归档/commit/checksum，重核原文件与补丁差异、公开 API、两种日期和签名字节、真实 SDK loopback、依赖扫描和适用 Wasm recipe。优先接受上游官方不再依赖 chrono 且兼容的发布；验证通过后删除该 patch/vendor。上游协作应单独发送最小补丁，本 PR 不冒称已经联系或被上游接受。

此片只移除 aliyun-tablestore-rs 的 chrono 入边；context 公开返回类型与 worker Date/ScheduledTime/timezone 仍需独立迁移，不能据此宣称全仓安全门禁解除。不要通过保留 chrono 的 all-features 兼容开关或改名包冒充已移除。
