//! SQL 查询
use std::collections::HashMap;

use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    model::{decode_plainbuf_rows, Row},
    protos::{
        plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
        ConsumedCapacity, SqlPayloadVersion, SqlStatementType,
    },
    timeseries_model::TimeseriesRow,
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 从字节解析数据的 trait
pub trait TryFromBytes
where
    Self: Sized,
{
    fn try_from_bytes(bytes: Vec<u8>) -> OtsResult<Vec<Self>>;
}

impl TryFromBytes for Row {
    fn try_from_bytes(bytes: Vec<u8>) -> OtsResult<Vec<Self>> {
        if bytes.is_empty() {
            return Ok(vec![]);
        }

        decode_plainbuf_rows(bytes, MASK_HEADER | MASK_ROW_CHECKSUM)
    }
}

impl TryFromBytes for TimeseriesRow {
    fn try_from_bytes(bytes: Vec<u8>) -> OtsResult<Vec<Self>> {
        let rows: Vec<Row> = Row::try_from_bytes(bytes)?;

        Ok(rows.into_iter().map(TimeseriesRow::from).collect())
    }
}

/// SQL协议版本，取值范围如下：
///
/// - `0` ：以字符串编码返回时间日期类型。
/// - `1` ：以整型编码返回时间日期类型
#[derive(Debug, Default, Copy, Clone)]
pub enum SqlVersion {
    #[default]
    DateTimeAsString = 0,
    DateTimeAsLong = 1,
}

/// 使用 SQL 查询数据。对于返回的数据目前采用的是 Plain buffer 编码。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/sqlquery>
#[derive(Debug, Default, Clone)]
pub struct SqlQueryRequest {
    /// SQL 语句
    pub query: String,

    /// SQL 版本协议。见：[`SqlVersion`]
    pub sql_version: SqlVersion,

    /// 翻页查询的标识
    pub search_token: Option<String>,
}

impl SqlQueryRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            sql_version: SqlVersion::DateTimeAsString,
            search_token: None,
        }
    }

    /// 设置 sql 版本
    pub fn sql_version(mut self, ver: SqlVersion) -> Self {
        self.sql_version = ver;

        self
    }

    /// 设置翻页 Token
    pub fn search_token(mut self, token: impl Into<String>) -> Self {
        self.search_token = Some(token.into());

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if self.query.is_empty() {
            return Err(OtsError::ValidationFailed("query statement can not be empty".to_string()));
        }
        Ok(())
    }
}

impl From<SqlQueryRequest> for crate::protos::SqlQueryRequest {
    fn from(value: SqlQueryRequest) -> Self {
        let SqlQueryRequest {
            query,
            sql_version,
            search_token,
        } = value;

        Self {
            query,
            version: Some(SqlPayloadVersion::SqlPlainBuffer as i32),
            sql_version: Some(sql_version as i64),
            search_token,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SqlQueryResponse<T>
where
    T: TryFromBytes,
{
    pub consumes: HashMap<String, ConsumedCapacity>,
    pub rows: Vec<T>,
    pub sql_statement_type: SqlStatementType,
    pub next_search_token: Option<String>,
}

impl<T> TryFrom<crate::protos::SqlQueryResponse> for SqlQueryResponse<T>
where
    T: TryFromBytes,
{
    type Error = OtsError;

    fn try_from(value: crate::protos::SqlQueryResponse) -> Result<Self, Self::Error> {
        let crate::protos::SqlQueryResponse {
            consumes,
            rows,
            version: _,
            r#type,
            next_search_token,
        } = value;

        Ok(Self {
            consumes: consumes
                .into_iter()
                .filter(|tcc| tcc.table_name.is_some())
                .map(|tcc| (tcc.table_name.unwrap(), tcc.consumed.unwrap_or_default()))
                .collect::<HashMap<_, _>>(),

            rows: if let Some(rows_bytes) = rows {
                T::try_from_bytes(rows_bytes)?
            } else {
                vec![]
            },

            sql_statement_type: match r#type {
                Some(n) if (1..=6).contains(&n) => SqlStatementType::try_from(n).unwrap(),
                _ => return Err(OtsError::ValidationFailed(format!("invalid sql statement type: {:?}", r#type))),
            },
            next_search_token,
        })
    }
}

#[derive(Clone)]
pub struct SqlQueryOperation {
    client: OtsClient,
    request: SqlQueryRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(SqlQueryOperation);

impl SqlQueryOperation {
    pub(crate) fn new(client: OtsClient, request: SqlQueryRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    /// 注意：这里使用 SQL 查询之后的数据就没有再区分主键和普通列了，
    /// 需要调用者根据实际需求将列中的主键数据提取出来
    pub async fn send<T>(self) -> OtsResult<SqlQueryResponse<T>>
    where
        T: TryFromBytes,
    {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::SqlQueryRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::SQLQuery,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        let resp_msg = crate::protos::SqlQueryResponse::decode(resp.bytes().await?)?;

        resp_msg.try_into()
    }
}

#[cfg(test)]
mod test_sql_query {
    use crate::{model::Row, test_util::setup, timeseries_model::TimeseriesRow, OtsClient};

    use super::SqlQueryRequest;

    async fn test_sql_query_impl() {
        setup();
        let client = OtsClient::from_env();

        let req = SqlQueryRequest::new("select * from timeseries_demo_with_data where _m_name = 'measure_11'");
        let resp = client.sql_query(req).send::<TimeseriesRow>().await;
        log::debug!("timeseries table: {:?}", resp);

        let req = SqlQueryRequest::new("select * from users limit 10");
        let resp = client.sql_query(req).send::<Row>().await;
        log::debug!("wide column table: {:?}", resp);
    }

    #[tokio::test]
    async fn test_sql_query() {
        test_sql_query_impl().await;
    }
}
