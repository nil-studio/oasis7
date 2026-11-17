use prost::Message;

use crate::{
    error::OtsError,
    timeseries_model::rules::{validate_timeseries_table_name, MIN_DATA_TTL_SECONDS, MIN_META_TTL_SECONDS},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 更新时序表的配置信息或时序时间线表的配置信息
///
/// **注意：** 表格选项（`ttl_seconds`）和元数据选项（ `allow_update_attributes` 和 `meta_ttl_seconds` ），
/// 可以视为两部分设置，不可以在同一个请求中同时修改这两部分。如果需要修改，那么就创建 2 个请求，分别修改。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/updatetimeseriestable>
#[derive(Debug, Default, Clone)]
pub struct UpdateTimeseriesTableRequest {
    /// 表名
    pub table_name: String,

    /// 数据生命周期，单位为秒。 默认为 `-1` 表示永不过期。最低 `86400` 秒（1 天）
    pub ttl_seconds: Option<i32>,

    /// 是否允许更新时间线属性列
    pub allow_update_attributes: Option<bool>,

    /// 时间线生命周期，单位为秒。取值必须大于等于 `604800` 秒（即 7 天）或者必须为 `-1`（数据永不过期）。
    pub meta_ttl_seconds: Option<i32>,
}

impl UpdateTimeseriesTableRequest {
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

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid timeseries table name: {}", self.table_name)));
        }

        if let (None, None, None) = (self.ttl_seconds, self.allow_update_attributes, self.meta_ttl_seconds) {
            return Err(OtsError::ValidationFailed("nothing to update".to_string()));
        }

        if self.ttl_seconds.is_some() && (self.allow_update_attributes.is_some() || self.meta_ttl_seconds.is_some()) {
            return Err(OtsError::ValidationFailed(
                "cannot update table options and meta options in one request".to_string(),
            ));
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

        Ok(())
    }
}

impl From<UpdateTimeseriesTableRequest> for crate::protos::timeseries::UpdateTimeseriesTableRequest {
    fn from(value: UpdateTimeseriesTableRequest) -> Self {
        let UpdateTimeseriesTableRequest {
            table_name,
            ttl_seconds,
            allow_update_attributes,
            meta_ttl_seconds,
        } = value;

        Self {
            table_name,
            table_options: ttl_seconds.map(|n| crate::protos::timeseries::TimeseriesTableOptions { time_to_live: Some(n) }),

            meta_options: if allow_update_attributes.is_some() || meta_ttl_seconds.is_some() {
                Some(crate::protos::timeseries::TimeseriesMetaOptions {
                    allow_update_attributes,
                    meta_time_to_live: meta_ttl_seconds,
                })
            } else {
                None
            },
        }
    }
}

#[derive(Clone)]
pub struct UpdateTimeseriesTableOperation {
    client: OtsClient,
    request: UpdateTimeseriesTableRequest,
    options: OtsRequestOptions,
}

impl UpdateTimeseriesTableOperation {
    pub(crate) fn new(client: OtsClient, request: UpdateTimeseriesTableRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::timeseries::UpdateTimeseriesTableRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::UpdateTimeseriesTable,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        resp.bytes().await?;

        Ok(())
    }
}
