# DomainEvent legacy wire fixtures

来自 source head `163fffef9102121b04f0016652d14b05a15c79ec` 的原始 Serialize derive；该版本 Debug/Eq 已拆分，不是旧 Debug 的对照。104 个变体各一个基础值，再各增加一个值，使全部 37 个 skip 谓词经过 present/nonlegacy 分支，共 208 个纯数据值。

测试核对 JSON、普通/packed CBOR 与公开 Serde 调用轨迹（结构/变体名称、索引、字段顺序及值、skip/end）。嵌套字段值用 JSON 记录；编码字节另外核对。数据仅验证被构造值的 wire 兼容，不证明事件业务资格、应用结果或 CodeQL 性能。

来源及校验值见 provenance.json 和 legacy manifest。旧 manifest 的 generator.rs.txt 是模板逻辑标签，实际执行 workspace harness 为 legacy-oracle-workspace.rs.txt，详见 provenance。
