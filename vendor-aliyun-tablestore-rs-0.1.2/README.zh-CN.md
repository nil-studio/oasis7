# Aliyun Tablestore (OTS) Rust SDK

[![Crates.io Version](https://img.shields.io/crates/v/aliyun-tablestore-rs?_ts_=20250411)](https://crates.io/crates/aliyun-tablestore-rs)
![Crates.io MSRV](https://img.shields.io/crates/msrv/aliyun-tablestore-rs?_ts_=20250411)
[![docs.rs](https://img.shields.io/docsrs/aliyun-tablestore-rs)](https://docs.rs/aliyun-tablestore-rs)
[![Crates.io License](https://img.shields.io/crates/l/aliyun-tablestore-rs?_ts_=20250411)](https://github.com/yuqiang-yuan/aliyun-tablestore-rs?tab=License-1-ov-file)

[English](https://github.com/yuqiang-yuan/aliyun-tablestore-rs) | [中文](https://github.com/yuqiang-yuan/aliyun-tablestore-rs/blob/dev/README.zh-CN.md)

阿里云表格存储面向海量结构化数据提供Serverless表存储服务，适用于海量账单、IM消息、物联网、车联网、风控、推荐等场景中的结构化数据存储，提供海量数据低成本存储、毫秒级的在线数据查询和检索以及灵活的数据分析能力。

本项目在属性较多的类型上，设计了链式调用方法，方便大家使用。另外，大部分的参数都是获得所有权的设计，虽然会多花掉一些内存，但是在异步场景下以及带有重试机制的设计下，会更加方便一些。

目前尚未实现的有：本地事务、通道服务和数据流操作。

已实现的功能：

- 宽表模型
  - 表操作
    - 创建表
    - 获取表信息
    - 列出表
    - 更新表
    - 删除表
    - 计算分割点
  - 预定义列操作
    - 添加预定义列
    - 删除预定义列
  - 数据操作
    - 读取行数据
    - 按范围读取行数据
    - 写入行数据
    - 更新行数据
    - 删除行数据
    - 批量读取行数据
    - 批量写入行数据
    - 批量导入
    - 批量导出
  - 索引操作
    - 创建索引
    - 删除索引
  - 多元索引
    - 创建多元索引
    - 删除多元索引
    - 获取多元索引信息
    - 列出多元索引
    - 使用多元索引查询
    - 并行扫描
    - 计算分割点
- 时序表
  - 时序表操作
    - 创建时序表
    - 获取时序表信息
    - 删除时序表
    - 列出时序表
    - 更新时序表配置
  - 时序表数据操作
    - 读取数据
    - 写入数据
    - 查询时间线元数据
    - 更新时间线元数据
    - 删除时间线元数据
    - 扫描时序数据
    - 切分全量导出任务
- SQL 查询



## 示例代码

### 根据主键范围查询数据

```rust
use aliyun_tablestore_rs::{data::GetRangeRequest, OtsClient, OtsResult};

#[tokio::main]
pub async fn main() -> OtsResult<()> {
    let client = OtsClient::new(
        "your_ak_id",
        "your_ak_sec",
        "https://instance-name.region.ots.aliyuncs.com",
    );
    let resp = client
        .get_range(
            GetRangeRequest::new("users")
                .start_primary_key_column_string("user_id_part", "0000")
                .start_primary_key_column_string("user_id", "0000006e-3d96-42b2-a624-d8ec9c52ad54")
                .end_primary_key_column_string("user_id_part", "0000")
                .end_primary_key_column_inf_max("user_id")
                .limit(100),
        )
        .send()
        .await?;

    for row in &resp.rows {
        println!("{:#?}", row);
    }

    Ok(())
}
```
