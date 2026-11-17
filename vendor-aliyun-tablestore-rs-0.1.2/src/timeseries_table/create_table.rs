use std::collections::HashSet;

use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    protos::{timeseries::TimeseriesAnalyticalStore, PrimaryKeyOption, PrimaryKeySchema, PrimaryKeyType},
    timeseries_model::rules::{
        validate_timeseries_table_name, DEFAULT_ANALYTICAL_NAME, MAX_FIELD_PRIMARY_KEY_COUNT, MAX_TIMESERIES_KEY_COUNT, MIN_ANALYTICAL_STORE_TTL_SECONDS,
        MIN_DATA_TTL_SECONDS, MIN_META_TTL_SECONDS,
    },
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 创建时序表
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/createtimeseriestable>
#[derive(Debug, Default, Clone)]
pub struct CreateTimeseriesTableRequest {
    /// 表名
    pub table_name: String,

    /// 数据生命周期，单位为秒。 默认为 `-1` 表示永不过期。最低 `86400` 秒（1 天）
    pub ttl_seconds: Option<i32>,

    /// 是否允许更新时间线属性列
    pub allow_update_attributes: Option<bool>,

    /// 时间线生命周期，单位为秒。取值必须大于等于 `604800` 秒（即 7 天）或者必须为 `-1`（数据永不过期）。
    pub meta_ttl_seconds: Option<i32>,

    /// 分析存储信息。默认为 `None` 表示不创建分析存储
    pub analytical_store: Option<TimeseriesAnalyticalStore>,

    /// lastpoint 索引
    pub lastpoint_indexes: HashSet<String>,

    /// 自定义时间线主键。留空则表示采用默认的 `_m_name`，`_data_source`，`_tags`
    pub timeseries_keys: Vec<String>,

    /// 作为主键的数据字段，支持配置多个。
    /// 当实际业务中存在时间线标识和时间点相同，但是时序数据不同的数据存储需求时，您可以通过为时序表添加作为主键的数据字段来实现。
    pub field_primary_keys: Vec<PrimaryKeySchema>,
}

impl CreateTimeseriesTableRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置数据生命周期，单位为秒。 默认为 `-1` 表示永不过期。最低 `86400` 秒（1 天）
    pub fn ttl_seconds(mut self, ttl_seconds: i32) -> Self {
        self.ttl_seconds = Some(ttl_seconds);

        self
    }

    /// 设置是否允许更新时间线属性列
    pub fn allow_update_attributes(mut self, allow: bool) -> Self {
        self.allow_update_attributes = Some(allow);

        self
    }

    /// 设置时间线生命周期，单位为秒。取值必须大于等于 `604800` 秒（即 7 天）或者必须为 `-1`（数据永不过期）。
    pub fn meta_ttl_seconds(mut self, meta_ttl_seconds: i32) -> Self {
        self.meta_ttl_seconds = Some(meta_ttl_seconds);

        self
    }

    /// 添加一个 lastpoint 索引
    pub fn lastpoint_index(mut self, index_name: impl Into<String>) -> Self {
        self.lastpoint_indexes.insert(index_name.into());

        self
    }

    /// 设置 lastpoint 索引
    pub fn lastpoint_indexes(mut self, index_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.lastpoint_indexes = index_names.into_iter().map(|s| s.into()).collect();

        self
    }

    /// 设置并启用分析存储
    pub fn analytical_store(mut self, a_store: TimeseriesAnalyticalStore) -> Self {
        self.analytical_store = Some(a_store);

        self
    }

    /// 添加一个时间线主键
    pub fn timeseries_key(mut self, key_name: impl Into<String>) -> Self {
        self.timeseries_keys.push(key_name.into());

        self
    }

    /// 设置时间线主键
    pub fn timeseries_keys(mut self, key_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.timeseries_keys = key_names.into_iter().map(|s| s.into()).collect();

        self
    }

    /// 添加一个扩展主键
    pub fn field_primary_key(mut self, pk_schema: PrimaryKeySchema) -> Self {
        self.field_primary_keys.push(pk_schema);

        self
    }

    /// 设置扩展主键
    pub fn field_primary_keys(mut self, pk_schemas: impl IntoIterator<Item = PrimaryKeySchema>) -> Self {
        self.field_primary_keys = pk_schemas.into_iter().collect();

        self
    }

    /// 添加字符串类型的主键列
    pub fn field_primary_key_string(mut self, name: &str) -> Self {
        self.field_primary_keys.push(PrimaryKeySchema {
            name: name.to_string(),
            r#type: PrimaryKeyType::String as i32,
            option: None,
        });

        self
    }

    /// 添加整数类型的主键列
    pub fn field_primary_key_integer(mut self, name: &str, auto_inc: bool) -> Self {
        self.field_primary_keys.push(PrimaryKeySchema {
            name: name.to_string(),
            r#type: PrimaryKeyType::Integer as i32,
            option: if auto_inc { Some(PrimaryKeyOption::AutoIncrement as i32) } else { None },
        });

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid timeseries table name: {}", self.table_name)));
        }

        if let Some(n) = self.ttl_seconds {
            if n != -1 && n < MIN_DATA_TTL_SECONDS {
                return Err(OtsError::ValidationFailed(format!(
                    "invalid ttl seconds: {}. must be -1 or greater than {}",
                    n, MIN_DATA_TTL_SECONDS
                )));
            }
        }

        if let Some(n) = self.meta_ttl_seconds {
            if n != -1 && n < MIN_META_TTL_SECONDS {
                return Err(OtsError::ValidationFailed(format!(
                    "invalid meta ttl seconds: {}. must be -1 or greater than {}",
                    n, MIN_META_TTL_SECONDS
                )));
            }
        }

        if self.timeseries_keys.len() > MAX_TIMESERIES_KEY_COUNT {
            return Err(OtsError::ValidationFailed(format!(
                "invalid timeseries keys. keys count must less than {}",
                MAX_TIMESERIES_KEY_COUNT
            )));
        }

        if self.field_primary_keys.len() > MAX_FIELD_PRIMARY_KEY_COUNT {
            return Err(OtsError::ValidationFailed(format!(
                "invalid field primary keys: keys count must less than {}",
                MAX_FIELD_PRIMARY_KEY_COUNT
            )));
        }

        if let Some(a_store) = &self.analytical_store {
            if let Some(n) = a_store.time_to_live {
                if n != -1 && n < MIN_ANALYTICAL_STORE_TTL_SECONDS {
                    return Err(OtsError::ValidationFailed(format!(
                        "invalid analytical store ttl seconds: {}. must be -1 or greater than {} (30 days)",
                        n, MIN_ANALYTICAL_STORE_TTL_SECONDS
                    )));
                }
            }
        }

        Ok(())
    }
}

impl From<CreateTimeseriesTableRequest> for crate::protos::timeseries::CreateTimeseriesTableRequest {
    fn from(value: CreateTimeseriesTableRequest) -> Self {
        let CreateTimeseriesTableRequest {
            table_name,
            ttl_seconds,
            allow_update_attributes,
            meta_ttl_seconds,
            analytical_store,
            lastpoint_indexes,
            timeseries_keys,
            field_primary_keys,
        } = value;

        let a_store = if let Some(store) = analytical_store {
            let crate::protos::timeseries::TimeseriesAnalyticalStore {
                store_name,
                time_to_live,
                sync_option,
            } = store;

            Some(crate::protos::timeseries::TimeseriesAnalyticalStore {
                store_name: if let Some(s) = store_name {
                    if s.is_empty() {
                        Some(DEFAULT_ANALYTICAL_NAME.to_string())
                    } else {
                        Some(s)
                    }
                } else {
                    Some(DEFAULT_ANALYTICAL_NAME.to_string())
                },

                time_to_live: Some(time_to_live.unwrap_or(-1)),
                sync_option: Some(sync_option.unwrap_or(crate::protos::timeseries::AnalyticalStoreSyncType::SyncTypeFull as i32)),
            })
        } else {
            None
        };

        Self {
            table_meta: crate::protos::timeseries::TimeseriesTableMeta {
                table_name,
                table_options: Some(crate::protos::timeseries::TimeseriesTableOptions {
                    time_to_live: Some(ttl_seconds.unwrap_or(-1)),
                }),
                status: None,
                meta_options: Some(crate::protos::timeseries::TimeseriesMetaOptions {
                    allow_update_attributes,
                    meta_time_to_live: Some(meta_ttl_seconds.unwrap_or(-1)),
                }),
                timeseries_key_schema: timeseries_keys,
                field_primary_key_schema: field_primary_keys,
                disable_hash_partition_key: None,
                disable_timeseries_meta_index: None,
            },
            enable_analytical_store: Some(a_store.is_some()),
            analytical_stores: if let Some(store) = a_store { vec![store] } else { vec![] },
            lastpoint_index_metas: lastpoint_indexes
                .into_iter()
                .map(|idx| crate::protos::timeseries::LastpointIndexMetaForCreate { index_table_name: idx })
                .collect(),
        }
    }
}

#[derive(Clone)]
pub struct CreateTimeseriesTableOperation {
    client: OtsClient,
    request: CreateTimeseriesTableRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(CreateTimeseriesTableOperation);

impl CreateTimeseriesTableOperation {
    pub(crate) fn new(client: OtsClient, request: CreateTimeseriesTableRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::CreateTimeseriesTableRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::CreateTimeseriesTable,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        resp.bytes().await?;

        Ok(())
    }
}
