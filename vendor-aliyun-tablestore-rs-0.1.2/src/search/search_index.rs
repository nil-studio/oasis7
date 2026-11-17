//! 这个 module 的名字有点儿问题，因为不想和上层 `search` 包名重名，所以搞了这么一个奇怪的名字

use std::collections::{HashMap, HashSet};

use prost::Message;

use super::{AggregationResult, GroupByResult, SearchQuery};
use crate::model::rules::{validate_index_name, validate_table_name};
use crate::{
    add_per_request_options,
    error::OtsError,
    model::{PrimaryKey, Row},
    protos::{
        plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
        search::{ColumnReturnType, SearchHit},
        ConsumedCapacity,
    },
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 通过多元索引查询数据。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/search>
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// 数据表名称。
    pub table_name: String,

    /// 多元索引名称。
    pub index_name: String,

    /// 查询配置
    pub search_query: SearchQuery,

    /// 路由键的值。默认为空，表示不使用路由键。大部分时候不需要使用此值
    pub routing_values: Vec<PrimaryKey>,

    /// 需要返回的全部列的列名
    pub columns_to_get: HashSet<String>,

    /// 列返回类型
    pub column_return_type: Option<ColumnReturnType>,

    /// 查询的超时时间。单位为毫秒。
    pub timeout_ms: Option<u32>,
}

impl SearchRequest {
    pub fn new(table_name: &str, index_name: &str, query: SearchQuery) -> Self {
        Self {
            table_name: table_name.to_string(),
            index_name: index_name.to_string(),
            search_query: query,
            routing_values: Vec::new(),
            columns_to_get: HashSet::new(),
            column_return_type: None,
            timeout_ms: None,
        }
    }

    /// 添加一个路由主键
    pub fn routing_value(mut self, pk: PrimaryKey) -> Self {
        self.routing_values.push(pk);

        self
    }

    /// 设置路由主键
    pub fn routing_values(mut self, pks: impl IntoIterator<Item = PrimaryKey>) -> Self {
        self.routing_values = pks.into_iter().collect();

        self
    }

    /// 设置列返回类型
    pub fn column_return_type(mut self, column_return_type: ColumnReturnType) -> Self {
        self.column_return_type = Some(column_return_type);

        self
    }

    /// 添加要返回的列名
    pub fn column_to_get(mut self, col: impl Into<String>) -> Self {
        self.columns_to_get.insert(col.into());

        self
    }

    /// 设置要返回的列名
    pub fn columns_to_get(mut self, cols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.columns_to_get = cols.into_iter().map(|col| col.into()).collect();

        self
    }

    /// 设置查询超时时间，单位为毫秒
    pub fn timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = Some(timeout_ms);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if !validate_index_name(&self.index_name) {
            return Err(OtsError::ValidationFailed(format!("invalid index name: {}", self.index_name)));
        }

        if let Some(n) = self.timeout_ms {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("invalid timeout(ms): {}", n)));
            }
        }

        self.search_query.validate()?;

        Ok(())
    }
}

impl From<SearchRequest> for crate::protos::search::SearchRequest {
    fn from(value: SearchRequest) -> Self {
        let SearchRequest {
            table_name,
            index_name,
            search_query,
            routing_values,
            columns_to_get,
            column_return_type,
            timeout_ms,
        } = value;

        Self {
            table_name: Some(table_name),
            index_name: Some(index_name),
            columns_to_get: if !columns_to_get.is_empty() || column_return_type.is_some() {
                Some(crate::protos::search::ColumnsToGet {
                    return_type: column_return_type.map(|v| v as i32),
                    column_names: columns_to_get.into_iter().collect(),
                })
            } else {
                None
            },
            search_query: Some(crate::protos::search::SearchQuery::from(search_query).encode_to_vec()),
            routing_values: routing_values
                .into_iter()
                .map(|pk| pk.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM))
                .collect(),
            timeout_ms: timeout_ms.map(|n| n as i32),
        }
    }
}

/// 通过多元索引查询数据响应结构
#[derive(Debug, Default, Clone)]
pub struct SearchResponse {
    /// 命中的总行数
    pub total_hits: u64,

    /// 数据行
    pub rows: Vec<Row>,

    /// 返回的命中结果。当使用查询摘要与高亮功能或向量检索进行查询时才有返回值。
    pub search_hits: Vec<SearchHit>,

    /// 是否全部成功。
    pub is_all_succeeded: bool,

    /// 下一次数据读取的起始位置。如果满足条件的数据行均已返回，则返回值为空。
    pub next_token: Option<Vec<u8>>,

    /// 对数据行进行统计聚合结果。key 是聚合名称
    pub aggregation_results: HashMap<String, AggregationResult>,

    /// 对数据行进行分组的结果。key 是分组名称
    pub group_by_results: HashMap<String, GroupByResult>,

    /// 一次操作消耗的按量服务能力单元
    pub consumed: ConsumedCapacity,

    /// 一次操作消耗的预留服务能力单元
    pub reserved_consumed: ConsumedCapacity,
}

impl SearchResponse {
    /// 获取一个聚合结果
    pub fn get_aggregation_result(&self, aggr_name: impl AsRef<str>) -> Option<&AggregationResult> {
        self.aggregation_results.get(aggr_name.as_ref())
    }

    /// 获取一个分组结果
    pub fn get_group_by_result(&self, group_by_name: impl AsRef<str>) -> Option<&GroupByResult> {
        self.group_by_results.get(group_by_name.as_ref())
    }
}

impl TryFrom<crate::protos::search::SearchResponse> for SearchResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::SearchResponse) -> Result<Self, Self::Error> {
        let crate::protos::search::SearchResponse {
            total_hits,
            rows: rows_bytes,
            is_all_succeeded,
            search_hits,
            next_token,
            aggs: aggs_bytes,
            group_bys: group_bys_bytes,
            consumed,
            reserved_consumed,
        } = value;

        let mut rows = vec![];
        for row_bytes in rows_bytes {
            if !row_bytes.is_empty() {
                rows.push(Row::decode_plain_buffer(row_bytes, MASK_HEADER)?);
            }
        }

        let aggregation_results = if let Some(bytes) = aggs_bytes {
            let msg = crate::protos::search::AggregationsResult::decode(bytes.as_slice())?;
            HashMap::<String, AggregationResult>::try_from(msg)?
        } else {
            HashMap::new()
        };

        let group_by_results = if let Some(bytes) = group_bys_bytes {
            let msg = crate::protos::search::GroupBysResult::decode(bytes.as_slice())?;
            HashMap::<String, GroupByResult>::try_from(msg)?
        } else {
            HashMap::new()
        };

        Ok(Self {
            total_hits: total_hits.map_or(0, |n| n as u64),
            rows,
            is_all_succeeded: is_all_succeeded.unwrap_or(true),
            search_hits,
            next_token,
            aggregation_results,
            group_by_results,
            consumed: consumed.unwrap_or_default(),
            reserved_consumed: reserved_consumed.unwrap_or_default(),
        })
    }
}

/// 多元索引搜索
#[derive(Clone)]
pub struct SearchOperation {
    client: OtsClient,
    request: SearchRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(SearchOperation);

impl SearchOperation {
    pub(crate) fn new(client: OtsClient, request: SearchRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<SearchResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::search::SearchRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::Search,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        let resp_msg = crate::protos::search::SearchResponse::decode(resp.bytes().await?)?;

        SearchResponse::try_from(resp_msg)
    }
}
