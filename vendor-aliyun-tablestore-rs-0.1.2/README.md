# Aliyun Tablestore (OTS) Rust SDK

[![Crates.io Version](https://img.shields.io/crates/v/aliyun-tablestore-rs?_ts_=20250411)](https://crates.io/crates/aliyun-tablestore-rs)
![Crates.io MSRV](https://img.shields.io/crates/msrv/aliyun-tablestore-rs?_ts_=20250411)
[![docs.rs](https://img.shields.io/docsrs/aliyun-tablestore-rs)](https://docs.rs/aliyun-tablestore-rs)
[![Crates.io License](https://img.shields.io/crates/l/aliyun-tablestore-rs?_ts_=20250411)](https://github.com/yuqiang-yuan/aliyun-tablestore-rs?tab=License-1-ov-file)

[English](https://github.com/yuqiang-yuan/aliyun-tablestore-rs) | [中文](https://github.com/yuqiang-yuan/aliyun-tablestore-rs/blob/dev/README.zh-CN.md)

This is a Rust SDK for the Aliyun Tablestore (OTS) service. Aliyun tablestore is a fully managed NoSQL database service that provides high-performance, scalable, and cost-effective storage for large amounts of structured data. It supports various data models such as key-value, wide-column, and document, and offers features like strong consistency, global distribution, and automatic sharding.

This project has designed chained calling methods for types with many attributes, making it more convenient to use. Additionally, most parameters are designed to take ownership - while this may use more memory, it's more convenient in asynchronous scenarios and designs with retry mechanisms.

So far, local transactions, tunnel and streaming **NOT** supported.

There operations are implemented:

- Wide-column table
  - Table operations
    - Create table
    - Describe table
    - List tables
    - Update table
    - Delete table
    - Compute table split points by size
  - Defined column operations
    - Add column
    - Delete column
  - Data operations
    - Get row
    - Get range
    - Put row
    - Update row
    - Delete row
    - Batch get row
    - Batch write row
    - Bulk import
    - Bulk export
  - Index
    - Create index
    - Drop index
  - Search Index
    - Create search index
    - Delete search index
    - Describe search Index
    - List search Index
    - Search using index
    - Parallel scan
    - Compute splits
- Time series table
  - Table operations
    - Create table
    - Delete table
    - Update table
    - List table
    - Describe table
  - Data operations
    - Get data
    - Put data
    - Query meta
    - Update meta
    - Delete meta
    - Split scan
    - Scan data
- SQL Query


## Examples

### Get range example

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
