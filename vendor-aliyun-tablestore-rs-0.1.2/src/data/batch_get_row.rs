use std::collections::HashSet;

use prost::Message;

use crate::model::rules::{validate_table_name, MAX_COLUMNS_TO_GET};
use crate::OtsRequestOptions;
use crate::{
    add_per_request_options,
    error::OtsError,
    model::{PrimaryKey, Row},
    protos::{
        plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
        ConsumedCapacity, TimeRange,
    },
    OtsClient, OtsOp, OtsRequest, OtsResult,
};

/// 单个表读取数据的配置
#[derive(Debug, Clone, Default)]
pub struct TableInBatchGetRowRequest {
    pub table_name: String,
    pub primary_keys: Vec<PrimaryKey>,
    pub columns_to_get: HashSet<String>,
    // Time range fields
    pub time_range_start_ms: Option<i64>,
    pub time_range_end_ms: Option<i64>,
    pub time_range_specific_ms: Option<i64>,

    pub max_versions: Option<i32>,
    pub start_column: Option<String>,
    pub end_column: Option<String>,
}

impl TableInBatchGetRowRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 添加一个主键
    pub fn primary_key(mut self, primary_key: PrimaryKey) -> Self {
        self.primary_keys.push(primary_key);

        self
    }

    /// 设置多个主键
    pub fn primary_keys(mut self, pks: impl IntoIterator<Item = PrimaryKey>) -> Self {
        self.primary_keys = pks.into_iter().collect();

        self
    }

    /// 需要返回的全部列的列名。如果为空，则返回指定行的所有列。`columns_to_get` 个数不应超过128个。
    /// 如果指定的列不存在，则不会返回指定列的数据；如果给出了重复的列名，返回结果只会包含一次指定列。
    pub fn column_to_get(mut self, name: &str) -> Self {
        self.columns_to_get.insert(name.to_string());

        self
    }

    /// 设置需要返回的列
    pub fn columns_to_get(mut self, names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.columns_to_get = names.into_iter().map(|s| s.into()).collect();

        self
    }

    /// 读取数据时，返回的最多版本个数。和 `time_range` 至少存在一个
    /// 如果指定 `max_versions` 为 `2` ，则每一列最多返回 `2` 个版本的数据。
    pub fn max_versions(mut self, max_versions: i32) -> Self {
        self.max_versions = Some(max_versions);

        self
    }

    /// 查询数据时指定的时间戳范围 `[start_time, end_time)` 或特定时间戳值 `time_specific`。
    /// 时间范围和特定时间戳值二者指定其一即可。
    ///
    /// - `start_ms`: 起始时间戳。单位是毫秒。时间戳的取值最小值为 `0`，最大值为 [`i64::MAX`](`std::i64::MAX`)。
    /// - `end_ms`: 结束时间戳。单位是毫秒。时间戳的取值最小值为 `0`，最大值为 [`i64::MAX`](`std::i64::MAX`)。
    pub fn time_range(mut self, start_ms: i64, end_ms: i64) -> Self {
        self.time_range_start_ms = Some(start_ms);
        self.time_range_end_ms = Some(end_ms);

        self
    }

    /// 指定精确的时间戳
    pub fn specific_time_ms(mut self, time_ms: i64) -> Self {
        self.time_range_specific_ms = Some(time_ms);

        self
    }

    /// 指定读取时的起始列，主要用于宽行读。列的顺序按照列名的字典序排序。返回的结果中**包含**当前起始列。
    /// 如果一张表有 `a` 、 `b` 、 `c` 三列，读取时指定 `start_column` 为 `b` ，则会从 `b` 列开始读，返回 `b`、`c` 两列。
    pub fn start_column(mut self, name: &str) -> Self {
        self.start_column = Some(name.into());

        self
    }

    /// 返回的结果中**不包含**当前结束列。列的顺序按照列名的字典序排序。
    /// 如果一张表有 `a` 、 `b` 、 `c` 三列，读取时指定 `end_column` 为 `b`，则读到 `b` 列时会结束，返回 `a` 列。
    pub fn end_column(mut self, name: &str) -> Self {
        self.end_column = Some(name.into());

        self
    }

    /// 设置读取的列范围。包含开始列名，不包含结束列名
    pub fn column_range(mut self, start_column_inclusive: Option<impl Into<String>>, end_column_exclusive: Option<impl Into<String>>) -> Self {
        self.start_column = start_column_inclusive.map(|s| s.into());
        self.end_column = end_column_exclusive.map(|s| s.into());

        self
    }

    /// Validate request parameter
    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid table name: {}", self.table_name)));
        }

        if self.primary_keys.is_empty() {
            return Err(OtsError::ValidationFailed("The primary keys can not be empty".to_string()));
        }

        for pk in &self.primary_keys {
            if pk.columns.is_empty() {
                return Err(OtsError::ValidationFailed("The row's primary key can not be empty".to_string()));
            }
        }

        if self.columns_to_get.len() > MAX_COLUMNS_TO_GET {
            return Err(OtsError::ValidationFailed(format!(
                "invalid columns to get: {}, must be less than or equal to {}",
                self.columns_to_get.len(),
                MAX_COLUMNS_TO_GET,
            )));
        }

        Ok(())
    }
}

impl From<TableInBatchGetRowRequest> for crate::protos::TableInBatchGetRowRequest {
    fn from(value: TableInBatchGetRowRequest) -> Self {
        let TableInBatchGetRowRequest {
            table_name,
            primary_keys,
            columns_to_get,
            time_range_start_ms,
            time_range_end_ms,
            time_range_specific_ms,
            max_versions,
            start_column,
            end_column,
        } = value;

        // 时间范围和最大版本都未设置的时候，默认设置 max_versions 为 1
        let max_versions = if max_versions.is_none() && time_range_start_ms.is_none() && time_range_end_ms.is_none() && time_range_specific_ms.is_none() {
            Some(1)
        } else {
            max_versions
        };

        Self {
            table_name,
            primary_key: primary_keys
                .into_iter()
                .map(|pk| pk.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM))
                .collect(),
            token: vec![],
            columns_to_get: columns_to_get.into_iter().collect(),
            time_range: if time_range_start_ms.is_some() || time_range_end_ms.is_some() || time_range_specific_ms.is_some() {
                Some(TimeRange {
                    start_time: time_range_start_ms,
                    end_time: time_range_end_ms,
                    specific_time: time_range_specific_ms,
                })
            } else {
                None
            },
            max_versions,
            filter: None,
            start_column,
            end_column,
        }
    }
}

/// 批量读取一个表或多个表中的若干行数据。
/// BatchGetRow 操作可视为多个 GetRow 操作的集合，各个操作独立执行，独立返回结果，独立计算服务能力单元。
/// 与执行大量的 GetRow 操作相比，使用BatchGetRow操作可以有效减少请求的响应时间，提高数据的读取速率。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/batchgetrow>
#[derive(Debug, Clone, Default)]
pub struct BatchGetRowRequest {
    pub tables: Vec<TableInBatchGetRowRequest>,
}

impl BatchGetRowRequest {
    pub fn new() -> Self {
        Self { tables: Vec::new() }
    }

    /// 添加一个表的查询
    pub fn table(mut self, item: TableInBatchGetRowRequest) -> Self {
        self.tables.push(item);

        self
    }

    /// 设置多个表的查询
    pub fn tables(mut self, items: impl IntoIterator<Item = TableInBatchGetRowRequest>) -> Self {
        self.tables = items.into_iter().collect();

        self
    }

    /// 指定需要读取的行信息。
    ///
    /// 如果 tables 中出现了下述情况，则操作整体失败，返回错误。
    ///
    /// - tables 中任一表不存在。
    /// - tables 中任一表名不符合命名规则和数据类型。更多信息，请参见命名规则和数据类型。
    /// - tables 中任一行未指定主键、主键名称不符合规范或者主键类型不正确。
    /// - tables 中任一表的 columns_to_get 内的列名不符合命令规则和数据类型。更多信息，请参见命名规则和数据类型。
    /// - tables 中包含同名的表。
    /// - 所有 tables 中 RowInBatchGetRowRequest 的总个数超过 100 个。
    /// - tables 中任一表中不包含任何 RowInBatchGetRowRequest。
    /// - tables 中任一表的 columns_to_get 超过 128 列。
    fn validate(&self) -> OtsResult<()> {
        if self.tables.is_empty() {
            return Err(OtsError::ValidationFailed("tables can not be empty".to_string()));
        }

        let table_name_set: HashSet<&String> = self.tables.iter().map(|t| &t.table_name).collect();

        if table_name_set.len() != self.tables.len() {
            return Err(OtsError::ValidationFailed(
                "There are multiple tables have same name in the request".to_string(),
            ));
        }

        let n = self.tables.iter().map(|t| t.primary_keys.len()).sum::<usize>();

        if n > 100 {
            return Err(OtsError::ValidationFailed(format!(
                "invalid tables. maximum rows to get is 100, you passed {}",
                n
            )));
        }

        for table in &self.tables {
            table.validate()?;
        }

        Ok(())
    }
}

impl From<BatchGetRowRequest> for crate::protos::BatchGetRowRequest {
    fn from(request: BatchGetRowRequest) -> Self {
        let mut tables = Vec::new();
        for table in request.tables {
            tables.push(table.into());
        }

        Self { tables }
    }
}

/// 批量读取一个表或多个表的响应中的一个条目
#[derive(Debug, Default, Clone)]
pub struct TableInBatchGetRowResponse {
    pub table_name: String,
    pub rows: Vec<RowInBatchGetRowResponse>,
}

impl TryFrom<crate::protos::TableInBatchGetRowResponse> for TableInBatchGetRowResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::TableInBatchGetRowResponse) -> Result<Self, Self::Error> {
        let crate::protos::TableInBatchGetRowResponse { table_name, rows } = value;

        let mut ret_rows = vec![];

        for r in rows {
            ret_rows.push(r.try_into()?);
        }

        Ok(Self { table_name, rows: ret_rows })
    }
}

#[derive(Debug, Default, Clone)]
pub struct RowInBatchGetRowResponse {
    pub is_ok: bool,
    pub error: Option<crate::protos::Error>,
    pub consumed: Option<ConsumedCapacity>,
    pub row: Option<Row>,
    pub next_token: Option<Vec<u8>>,
}

impl TryFrom<crate::protos::RowInBatchGetRowResponse> for RowInBatchGetRowResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::RowInBatchGetRowResponse) -> Result<Self, Self::Error> {
        let crate::protos::RowInBatchGetRowResponse {
            is_ok,
            error,
            consumed,
            row,
            next_token,
        } = value;

        Ok(Self {
            is_ok,
            error,
            consumed,
            row: if let Some(row_bytes) = row {
                if !row_bytes.is_empty() {
                    Some(Row::decode_plain_buffer(row_bytes, MASK_HEADER)?)
                } else {
                    None
                }
            } else {
                None
            },
            next_token,
        })
    }
}

/// 批量读取一个表或多个表的响应
#[derive(Debug, Default, Clone)]
pub struct BatchGetRowResponse {
    pub tables: Vec<TableInBatchGetRowResponse>,
}

impl TryFrom<crate::protos::BatchGetRowResponse> for BatchGetRowResponse {
    type Error = OtsError;
    fn try_from(value: crate::protos::BatchGetRowResponse) -> OtsResult<Self> {
        let crate::protos::BatchGetRowResponse { tables } = value;

        let mut ret_tables = vec![];
        for t in tables {
            ret_tables.push(t.try_into()?);
        }

        Ok(Self { tables: ret_tables })
    }
}

/// 批量读取表数据的操作
#[derive(Clone)]
pub struct BatchGetRowOperation {
    client: OtsClient,
    request: BatchGetRowRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(BatchGetRowOperation);

impl BatchGetRowOperation {
    pub(crate) fn new(client: OtsClient, request: BatchGetRowRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<BatchGetRowResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::BatchGetRowRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::BatchGetRow,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;

        let response_msg = crate::protos::BatchGetRowResponse::decode(response.bytes().await?)?;

        response_msg.try_into()
    }
}
