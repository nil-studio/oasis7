# WASM Builder Image

这个目录定义 oasis7 的 canonical WASM builder image。

## 目标
- 发布级 WASM 只通过 Docker 构建。
- canonical 容器平台固定为 `linux-x86_64`（Docker platform `linux/amd64`）。
- 宿主机不再保留 native fallback。

## 本地构建
```bash
docker build \
  --platform linux/amd64 \
  -t oasis7/wasm-builder:nightly-2025-12-11 \
  -f docker/wasm-builder/Dockerfile \
  .
```

## 本地使用
默认入口仍是仓库根目录的 `scripts/build-wasm-module.sh`。该脚本会：
- 检查 Docker daemon。
- 自动构建默认 builder image（若本地不存在）。
- 把源码 workspace 绑定到容器内 `/workspace`。
- 在容器内执行 `wasm_build_suite build ...`。

示例：
```bash
./scripts/build-wasm-module.sh \
  --module-id kwt.template \
  --manifest-path tools/wasm_build_suite/templates/minimal_module/Cargo.toml \
  --out-dir .tmp/wasm-build-suite \
  --profile dev
```

## 约束
- 调用方必须保证 `manifest-path` 和 `out-dir` 位于同一个 workspace root 下。
- repo 内模块默认以仓库根目录作为 workspace root。
- `ModuleSourcePackage` 这类临时源码包应在其临时 workspace root 下调用该脚本。

`--candidate-build` 为每次构建在输出目录下创建空的私有临时 Cargo home，
挂载到容器内固定的 `/tmp/oasis7-cargo-home`，退出时随本次临时目录清理。
这样 registry 与 git 依赖提取占用任务工作区的磁盘，不占 Docker 临时可写层；Cargo 状态、配置与凭据均不跨构建复用。
同次构建另建空的私有编译临时目录，挂载到 `/tmp/oasis7-compiler-tmp` 并设为
`TMPDIR`，使 rustc 临时文件也使用任务工作区。该目录与 Cargo home 独立，退出时一并清理，不挂载整个容器 `/tmp`。
输出目录不能通过符号链接指向其他工作区。锁文件、实际 image ID、producer recipe 与 build receipt 校验保持生效。


## 批量候选构建

使用 `scripts/build-builtin-wasm-modules.sh --candidate-build` 构建一组模块。每批核对当前 producer recipe 并固定一个真实不可变镜像 ID；本地没有匹配镜像时只构建一次。已有镜像仍须通过平台、recipe labels、入口与环境检查，每个模块还会验证镜像中的实际 producer 源码，并独立使用空 Cargo/TMP 目录。

批次运行前后若 producer 输入发生变化则失败。镜像 tag 改变不会改变已经选定的 image ID；批次结束只清理自身临时目录和用于保持镜像引用的未运行容器，不删除共享镜像或缓存。

构建成功的 receipt 保留真实 recipe 和实际执行镜像身份。优化镜像或更新 producer 后必须重新构建对应工件，不能给旧字节重新标注新 receipt。批量优化本身不授予发布或运行时启用资格。
