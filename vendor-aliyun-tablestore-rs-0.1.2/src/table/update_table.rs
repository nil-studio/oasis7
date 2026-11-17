use std::collections::HashSet;

use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    protos::{CapacityUnit, ReservedThroughput, StreamSpecification, TableOptions, UpdateTableResponse},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

use crate::model::rules::validate_table_name;

/// 修改表的配置信息 table_options 和 Stream 配置 StreamSpecification。
/// 如果表处于 CU 模式（原按量模式）的高性能型实例中，
/// 您还可以为数据表配置预留读/写吞吐量 reserved_throughput，新设定将于更新成功后的一分钟内生效。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/updatetable-of-tablestore>
#[derive(Debug, Default, Clone)]
pub struct UpdateTableRequest {
    // table meta
    pub table_name: String,

    // reserved throughput
    pub reserved_throughput_read: Option<i32>,
    pub reserved_throughput_write: Option<i32>,

    // table options
    pub ttl_seconds: Option<i32>,
    pub max_versions: Option<i32>,
    pub deviation_cell_version_in_sec: Option<i64>,
    pub allow_update: Option<bool>,

    // stream spec.
    pub stream_enabled: bool,
    pub stream_expiration_hour: Option<i32>,
    pub stream_columns: HashSet<String>,
}

impl UpdateTableRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
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

    /// 添加 stream 列
    pub fn add_stream_column(mut self, col_name: impl Into<String>) -> Self {
        self.stream_columns.insert(col_name.into());
        self
    }

    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid table name: {}", self.table_name)));
        }

        Ok(())
    }
}

impl From<UpdateTableRequest> for crate::protos::UpdateTableRequest {
    fn from(value: UpdateTableRequest) -> Self {
        let UpdateTableRequest {
            table_name,
            reserved_throughput_read,
            reserved_throughput_write,
            ttl_seconds,
            max_versions,
            deviation_cell_version_in_sec,
            allow_update,
            stream_enabled,
            stream_expiration_hour,
            stream_columns,
        } = value;

        crate::protos::UpdateTableRequest {
            table_name,
            reserved_throughput: if reserved_throughput_read.is_some() || reserved_throughput_write.is_some() {
                Some(ReservedThroughput {
                    capacity_unit: CapacityUnit {
                        read: reserved_throughput_read,
                        write: reserved_throughput_write,
                    },
                })
            } else {
                None
            },
            table_options: if ttl_seconds.is_some() || max_versions.is_some() || deviation_cell_version_in_sec.is_some() || allow_update.is_some() {
                Some(TableOptions {
                    time_to_live: ttl_seconds,
                    max_versions,
                    deviation_cell_version_in_sec,
                    allow_update,
                    update_full_row: None,
                })
            } else {
                None
            },
            stream_spec: if stream_enabled {
                Some(StreamSpecification {
                    enable_stream: stream_enabled,
                    expiration_time: stream_expiration_hour,
                    columns_to_get: stream_columns.into_iter().collect::<Vec<_>>(),
                })
            } else {
                None
            },
        }
    }
}

/// 修改表配置信息
#[derive(Clone)]
pub struct UpdateTableOperation {
    client: OtsClient,
    request: UpdateTableRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(UpdateTableOperation);

impl UpdateTableOperation {
    /// Create a new update table operation
    pub(crate) fn new(client: OtsClient, request: UpdateTableRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<UpdateTableResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::UpdateTableRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::UpdateTable,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;

        Ok(UpdateTableResponse::decode(response.bytes().await?)?)
    }
}
