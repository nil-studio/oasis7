use std::collections::HashSet;

use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    protos::{
        CapacityUnit, DefinedColumnSchema, DefinedColumnType, IndexMeta, PrimaryKeySchema, PrimaryKeyType, ReservedThroughput, SseKeyType, SseSpecification,
        StreamSpecification, TableMeta, TableOptions,
    },
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

use crate::model::rules::{validate_column_name, validate_index_name, validate_table_name, MAX_PRIMARY_KEY_COUNT, MIN_PRIMARY_KEY_COUNT};

/// 根据给定的表结构信息创建相应的数据表的请求。
///
/// 根据官方文档 <https://help.aliyun.com/zh/tablestore/table-operations> 2025-03-06 10:05:03 更新的内容，在创建表的时候，支持设置以下内容：
///
/// - 主键
/// - 数据版本和生命周期
/// - 预留读写吞吐量
/// - 二级索引
/// - 数据加密
/// - 本地事务
///
/// 所以，虽然 `table_store.proto` 文件中的 `CreateTableRequest` 包含了分区相关的，但是这里没有放上来。对应的 Java SDK 5.17.5 版本中创建宽表的时候也是没有分区设定的。
#[derive(Debug, Default, Clone)]
pub struct CreateTableRequest {
    /// 表名
    pub table_name: String,

    /// 全部主键列
    pub primary_keys: Vec<PrimaryKeySchema>,

    /// 预定义列
    pub defined_columns: Vec<DefinedColumnSchema>,

    /// 预留读取吞吐量
    pub reserved_throughput_read: Option<i32>,

    /// 预留写入吞吐量
    pub reserved_throughput_write: Option<i32>,

    /// 数据生命周期，即数据的过期时间。当数据的保存时间超过设置的数据生命周期时，系统会自动清理超过数据生命周期的数据。
    /// 数据生命周期至少为86400秒（一天）或-1（数据永不过期）。
    pub ttl_seconds: Option<i32>,

    /// 最大版本数，即属性列能够保留数据的最大版本个数。当属性列数据的版本个数超过设置的最大版本数时，系统会自动删除较早版本的数据。
    pub max_versions: Option<i32>,

    /// 有效版本偏差，即写入数据的时间戳与系统当前时间的偏差允许最大值。只有当写入数据所有列的版本号与写入时时间的差值在数据有效版本偏差范围内，数据才能成功写入。
    pub deviation_cell_version_in_sec: Option<i64>,

    /// 是否允许通过 UpdateRow 更新写入数据。
    pub allow_update: Option<bool>,

    /// 该表是否打开stream。
    pub stream_enabled: bool,

    /// 该表的stream过期时间。
    pub stream_expiration_hour: Option<i32>,
    pub stream_columns: HashSet<String>,

    /// 是否启用加密
    pub sse_enabled: bool,

    /// 加密密钥类型
    pub sse_key_type: Option<SseKeyType>,

    /// 当密钥类型为 BYOK 时需要
    pub sse_key_id: Option<String>,

    /// 当密钥类型为 BYOK 时需要
    pub sse_arn: Option<String>,

    /// 是否启用本地事务
    pub enable_local_txn: Option<bool>,

    /// 二级索引
    pub indexes: Vec<IndexMeta>,
}

impl CreateTableRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 添加主键列。一个表格至少包含 1 个主键列，最多包含 4 个主键列
    fn add_primary_key(mut self, name: impl Into<String>, key_type: PrimaryKeyType, auto_inc: Option<bool>) -> Self {
        let pk = PrimaryKeySchema {
            name: name.into(),
            r#type: key_type as i32,
            option: auto_inc.map(|b| b as i32),
        };

        self.primary_keys.push(pk);
        self
    }

    /// 添加一个主键列
    pub fn primary_key(mut self, pk: PrimaryKeySchema) -> Self {
        self.primary_keys.push(pk);

        self
    }

    /// 设置主键列
    pub fn primary_keys(mut self, pks: impl IntoIterator<Item = PrimaryKeySchema>) -> Self {
        self.primary_keys = pks.into_iter().collect();

        self
    }

    /// 添加字符串类型的主键列
    pub fn primary_key_string(self, name: &str) -> Self {
        self.add_primary_key(name, PrimaryKeyType::String, None)
    }

    /// 添加整数类型的主键列。只有非分区键支持自增设置（主键集合中的第 1 个元素是分区键）
    pub fn primary_key_integer(self, name: &str, auto_inc: bool) -> Self {
        self.add_primary_key(name, PrimaryKeyType::Integer, Some(auto_inc))
    }

    /// 添加二进制类型的主键列
    pub fn primary_key_binary(self, name: &str) -> Self {
        self.add_primary_key(name, PrimaryKeyType::Binary, None)
    }

    /// 添加自增主键列
    pub fn primary_key_auto_increment(self, name: &str) -> Self {
        self.primary_key_integer(name, true)
    }

    /// 添加预定义列
    fn add_column(mut self, name: impl Into<String>, col_type: DefinedColumnType) -> Self {
        let col = DefinedColumnSchema {
            name: name.into(),
            r#type: col_type as i32,
        };

        self.defined_columns.push(col);

        self
    }

    /// 添加一个预定义列
    pub fn column(mut self, def_col: DefinedColumnSchema) -> Self {
        self.defined_columns.push(def_col);

        self
    }

    /// 设置预定义列
    pub fn columns(mut self, def_cols: impl IntoIterator<Item = DefinedColumnSchema>) -> Self {
        self.defined_columns = def_cols.into_iter().collect();

        self
    }

    /// 添加整数类型预定以列
    pub fn column_integer(self, name: &str) -> Self {
        self.add_column(name, DefinedColumnType::DctInteger)
    }

    /// 添加字符串类型预定义列
    pub fn column_string(self, name: &str) -> Self {
        self.add_column(name, DefinedColumnType::DctString)
    }

    /// 添加双精度类型预定义列
    pub fn column_double(self, name: &str) -> Self {
        self.add_column(name, DefinedColumnType::DctDouble)
    }

    /// 添加布尔值类型预定义列
    pub fn column_bool(self, name: &str) -> Self {
        self.add_column(name, DefinedColumnType::DctBoolean)
    }

    /// 添加二进制类型预定义列
    pub fn column_blob(self, name: &str) -> Self {
        self.add_column(name, DefinedColumnType::DctBlob)
    }

    /// 预设读取吞吐量。最大 100000 CU
    pub fn reserved_throughput_read(mut self, read_cu: i32) -> Self {
        self.reserved_throughput_read = Some(read_cu);

        self
    }

    /// 预设写入吞吐量。最大 100000 CU
    pub fn reserved_throughput_write(mut self, write_cu: i32) -> Self {
        self.reserved_throughput_write = Some(write_cu);

        self
    }

    /// 数据生命周期，即数据的过期时间。当数据的保存时间超过设置的数据生命周期时，系统会自动清理超过数据生命周期的数据。
    /// 数据生命周期至少为 `86400` 秒（一天）或 `-1`（数据永不过期）。
    pub fn ttl_seconds(mut self, ttl_seconds: i32) -> Self {
        self.ttl_seconds = Some(ttl_seconds);

        self
    }

    /// 最大版本数，即属性列能够保留数据的最大版本个数。当属性列数据的版本个数超过设置的最大版本数时，系统会自动删除较早版本的数据。
    pub fn max_versions(mut self, max_versions: i32) -> Self {
        self.max_versions = Some(max_versions);

        self
    }

    /// 有效版本偏差，即写入数据的时间戳与系统当前时间的偏差允许最大值。只有当写入数据所有列的版本号与写入时时间的差值在数据有效版本偏差范围内，数据才能成功写入。
    ///
    /// 属性列的有效版本范围为 `[max{数据写入时间-有效版本偏差,数据写入时间-数据生命周期}，数据写入时间+有效版本偏差)`。
    pub fn deviation_cell_version_seconds(mut self, dev: i64) -> Self {
        self.deviation_cell_version_in_sec = Some(dev);

        self
    }

    /// 是否允许通过 `UpdateRow` 更新写入数据。默认值为 `true`，表示允许通过 `UpdateRow` 更新写入数据。
    ///
    /// 当要使用多元索引生命周期功能时，您必须设置此参数为 `false`，即不允许通过 `UpdateRow` 更新写入数据。
    pub fn allow_update(mut self, allow_update: bool) -> Self {
        self.allow_update = Some(allow_update);

        self
    }

    /// 设置是否启用 stream
    pub fn stream(mut self, enabled: bool) -> Self {
        self.stream_enabled = enabled;

        self
    }

    /// 设置 stream 过期时间
    pub fn stream_expiration(mut self, exp: i32) -> Self {
        self.stream_expiration_hour = Some(exp);

        self
    }

    /// 添加一个 stream 列
    pub fn stream_column(mut self, col_name: impl Into<String>) -> Self {
        self.stream_columns.insert(col_name.into());

        self
    }

    /// 设置 stream 列
    pub fn stream_columns(mut self, col_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.stream_columns = col_names.into_iter().map(|s| s.into()).collect();

        self
    }

    /// 设置是否启用加密
    pub fn sse(mut self, enabled: bool) -> Self {
        self.sse_enabled = enabled;
        self
    }

    /// 设置加密类型
    pub fn sse_key_type(mut self, key_type: SseKeyType) -> Self {
        self.sse_key_type = Some(key_type);
        self
    }

    /// 设置加密密钥 ID
    pub fn sse_key_id(mut self, key_id: impl Into<String>) -> Self {
        self.sse_key_id = Some(key_id.into());
        self
    }

    /// 设置加密 ARN
    pub fn sse_arn(mut self, arn: impl Into<String>) -> Self {
        self.sse_arn = Some(arn.into());
        self
    }

    /// 是否启用本地事务
    pub fn local_txn(mut self, enabled: bool) -> Self {
        self.enable_local_txn = Some(enabled);
        self
    }

    /// 添加一个索引。可以使用 [`IndexMetaBuilder`](`crate::index::IndexMetaBuilder`) 建立索引信息
    pub fn index(mut self, idx_meta: IndexMeta) -> Self {
        self.indexes.push(idx_meta);
        self
    }

    /// 设置多个索引
    pub fn indexes(mut self, indexes: impl IntoIterator<Item = IndexMeta>) -> Self {
        self.indexes = indexes.into_iter().collect();

        self
    }

    /// Validate the create table settings
    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: \"{}\"", self.table_name)));
        }

        if !(MIN_PRIMARY_KEY_COUNT..=MAX_PRIMARY_KEY_COUNT).contains(&self.primary_keys.len()) {
            return Err(OtsError::ValidationFailed(format!(
                "invalid primary key count: {}. maximum primary key count must between {} to {}",
                self.primary_keys.len(),
                MIN_PRIMARY_KEY_COUNT,
                MAX_PRIMARY_KEY_COUNT
            )));
        }

        for pk in &self.primary_keys {
            if !validate_column_name(&pk.name) {
                return Err(OtsError::ValidationFailed(format!("invalid primary key name: {}", pk.name)));
            }
        }

        if let Some(n) = self.ttl_seconds {
            if n != -1 && n < 86400 {
                return Err(OtsError::ValidationFailed(format!(
                    "invalid time-to-live settings: {}. must be -1 or greater than 86400",
                    n
                )));
            }
        }

        for col in &self.defined_columns {
            if !validate_column_name(&col.name) {
                return Err(OtsError::ValidationFailed(format!("invalid column name: \"{}\"", col.name)));
            }
        }

        if self.sse_enabled {
            if let Some(SseKeyType::SseByok) = self.sse_key_type {
                if self.sse_key_id.is_none() || self.sse_arn.is_none() {
                    return Err(OtsError::ValidationFailed(
                        "You have SSE Enabled and key type is BYOK, but sse key id and ARN are not set".to_string(),
                    ));
                }
            }
        }

        let pk_names = self.primary_keys.iter().map(|k| k.name.as_str()).collect::<Vec<_>>();

        let col_names = self.defined_columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>();

        if !self.indexes.iter().all(|idx| {
            idx.primary_key.iter().all(|k| pk_names.contains(&k.as_str()))
                && idx.defined_column.iter().all(|c| col_names.contains(&c.as_str()))
                && validate_index_name(&idx.name)
        }) {
            return Err(OtsError::ValidationFailed(
                "invalid index meta. Can not find primary key or defined column with the name speicfied on index meta, or, the index name is invalid"
                    .to_string(),
            ));
        }

        Ok(())
    }
}

impl From<CreateTableRequest> for crate::protos::CreateTableRequest {
    fn from(value: CreateTableRequest) -> Self {
        let CreateTableRequest {
            table_name,
            primary_keys,
            defined_columns,
            reserved_throughput_read,
            reserved_throughput_write,
            ttl_seconds,
            max_versions,
            deviation_cell_version_in_sec,
            allow_update,
            stream_enabled,
            stream_expiration_hour,
            stream_columns,
            sse_enabled,
            sse_key_type,
            sse_key_id,
            sse_arn,
            enable_local_txn,
            indexes,
        } = value;

        crate::protos::CreateTableRequest {
            table_meta: TableMeta {
                table_name,
                primary_key: primary_keys,
                defined_column: defined_columns,
            },
            reserved_throughput: ReservedThroughput {
                capacity_unit: CapacityUnit {
                    read: Some(reserved_throughput_read.unwrap_or_default()),
                    write: Some(reserved_throughput_write.unwrap_or_default()),
                },
            },
            table_options: Some(TableOptions {
                time_to_live: Some(ttl_seconds.unwrap_or(-1)),
                max_versions: Some(max_versions.unwrap_or(1)),
                deviation_cell_version_in_sec: Some(deviation_cell_version_in_sec.unwrap_or(86400)),
                allow_update,
                update_full_row: None,
            }),
            partitions: vec![],
            stream_spec: if stream_enabled {
                Some(StreamSpecification {
                    enable_stream: stream_enabled,
                    expiration_time: stream_expiration_hour,
                    columns_to_get: stream_columns.into_iter().collect::<Vec<_>>(),
                })
            } else {
                None
            },
            sse_spec: if sse_enabled {
                Some(SseSpecification {
                    enable: sse_enabled,
                    key_type: sse_key_type.map(|v| v as i32),
                    key_id: sse_key_id.map(|v| v.into_bytes()),
                    role_arn: sse_arn.map(|v| v.into_bytes()),
                })
            } else {
                None
            },
            index_metas: indexes,
            enable_local_txn,
        }
    }
}

/// 创建库表请求
#[derive(Clone)]
pub struct CreateTableOperation {
    client: OtsClient,
    request: CreateTableRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(CreateTableOperation);

impl CreateTableOperation {
    pub(crate) fn new(client: OtsClient, request: CreateTableRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::CreateTableRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::CreateTable,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;

        response.bytes().await?;

        Ok(())
    }
}
