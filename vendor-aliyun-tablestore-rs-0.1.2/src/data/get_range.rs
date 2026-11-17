use crate::model::rules::validate_table_name;
use crate::model::Row;
use crate::protos::plain_buffer::{HEADER, MASK_HEADER, MASK_ROW_CHECKSUM};
use crate::protos::ConsumedCapacity;
use crate::{
    add_per_request_options,
    error::OtsError,
    model::{Filter, PrimaryKey, PrimaryKeyColumn},
    protos::{Direction, TimeRange},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};
use byteorder::{LittleEndian, ReadBytesExt};
use prost::Message;
use std::collections::HashSet;
use std::io::Cursor;

/// 读取指定主键范围内的数据请求
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/getrange>
#[derive(Default, Debug, Clone)]
pub struct GetRangeRequest {
    pub table_name: String,

    /// 本次查询的顺序。见 [`direction`](`Self::direction`)
    pub direction: Direction,

    /// 需要返回的全部列的列名。见 [`columns_to_get`](`Self::columns_to_get`)
    pub columns_to_get: HashSet<String>,

    /// TimeRange 设置。和 `max_versions` 只能存在一个。
    ///
    /// 读取数据的版本时间戳范围。时间戳的单位为毫秒，取值最小值为0，最大值为INT64.MAX。
    pub time_range_start_ms: Option<i64>,
    pub time_range_end_ms: Option<i64>,
    pub time_range_specific_ms: Option<i64>,

    /// 本次范围读取的起始主键，包含
    pub inclusive_start_primary_key: PrimaryKey,

    /// 本次范围读取的终止主键，不包含
    pub exclusive_end_primary_key: PrimaryKey,

    /// 最多返回的版本个数。
    pub max_versions: Option<i32>,

    /// 本次读取最多返回的行数。取值必须大于 0。如果查询到的行数超过此值，则通过响应中会包含断点来记录本次读取到的位置，以便下一次读取。
    /// 无论是否设置此项，表格存储最多返回的行数为 5000 且总数据大小不超过 4 MB。
    pub limit: Option<i32>,

    /// 过滤条件表达式
    pub filter: Option<Filter>,

    /// 指定读取时的起始列，主要用于宽行读。返回结果中会包含当前起始列。列的顺序按照列名的字典序排序。
    pub start_column: Option<String>,

    /// 指定读取时的结束列，主要用于宽行读。返回结果中不会包含当前结束列。列的顺序按照列名的字典序排序。
    pub end_column: Option<String>,

    /// 启用本地事务时使用
    pub transaction_id: Option<String>,
}

impl GetRangeRequest {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            ..Default::default()
        }
    }

    /// 本次查询的顺序。
    ///
    /// - 如果设置此项为 `FORWARD`（正序），则 `inclusive_start_primary` 必须小于 `exclusive_end_primary`，响应中各行按照主键由小到大的顺序进行排列。
    /// - 如果设置此项为 `BACKWARD`（逆序），则 `inclusive_start_primary` 必须大于 `exclusive_end_primary`，响应中各行按照主键由大到小的顺序进行排列。
    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;

        self
    }

    /// 添加主键查询范围
    pub fn primary_key_range(mut self, start_pk: PrimaryKey, end_pk: PrimaryKey) -> Self {
        self.inclusive_start_primary_key = start_pk;
        self.exclusive_end_primary_key = end_pk;

        self
    }

    pub fn start_primary_key(mut self, pk: PrimaryKey) -> Self {
        self.inclusive_start_primary_key = pk;

        self
    }

    /// 添加一个开始主键列
    pub fn start_primary_key_column(mut self, pk_col: PrimaryKeyColumn) -> Self {
        self.inclusive_start_primary_key.columns.push(pk_col);

        self
    }

    /// 设置开始主键列
    pub fn start_primary_key_columns(mut self, pk_cols: impl IntoIterator<Item = PrimaryKeyColumn>) -> Self {
        self.inclusive_start_primary_key = PrimaryKey {
            columns: pk_cols.into_iter().collect(),
        };

        self
    }

    /// 添加字符串类型的开始主键查询值。本次范围读取的起始主键，如果该行存在，则响应中一定会包含此行。
    pub fn start_primary_key_column_string(mut self, name: &str, value: impl Into<String>) -> Self {
        self.inclusive_start_primary_key.columns.push(PrimaryKeyColumn::from_string(name, value));
        self
    }

    /// 添加整数类型的开始主键查询值。本次范围读取的起始主键，如果该行存在，则响应中一定会包含此行。
    pub fn start_primary_key_column_integer(mut self, name: &str, value: i64) -> Self {
        self.inclusive_start_primary_key.columns.push(PrimaryKeyColumn::from_integer(name, value));

        self
    }

    /// 添加二进制类型的开始主键查询值。本次范围读取的起始主键，如果该行存在，则响应中一定会包含此行。
    pub fn start_primary_key_column_binary(mut self, name: &str, value: impl Into<Vec<u8>>) -> Self {
        self.inclusive_start_primary_key.columns.push(PrimaryKeyColumn::from_binary(name, value));

        self
    }

    /// 添加无穷小值开始主键
    pub fn start_primary_key_column_inf_min(mut self, name: &str) -> Self {
        self.inclusive_start_primary_key.columns.push(PrimaryKeyColumn::inf_min(name));

        self
    }

    /// 添加无穷大值开始主键
    pub fn start_primary_key_column_inf_max(mut self, name: &str) -> Self {
        self.inclusive_start_primary_key.columns.push(PrimaryKeyColumn::inf_max(name));

        self
    }

    /// 设置结束主键列
    pub fn end_primary_key(mut self, pk: PrimaryKey) -> Self {
        self.exclusive_end_primary_key = pk;

        self
    }

    /// 添加一个结束主键列
    pub fn end_primary_key_column(mut self, pk_col: PrimaryKeyColumn) -> Self {
        self.exclusive_end_primary_key.columns.push(pk_col);

        self
    }

    /// 设置结束主键列
    pub fn end_primary_key_columns(mut self, pk_cols: impl IntoIterator<Item = PrimaryKeyColumn>) -> Self {
        self.exclusive_end_primary_key = PrimaryKey {
            columns: pk_cols.into_iter().collect(),
        };

        self
    }

    /// 添加字符串类型的结束主键查询值。无论该行是否存在，则响应中一定不会包含此行。
    pub fn end_primary_key_column_string(mut self, name: &str, value: impl Into<String>) -> Self {
        self.exclusive_end_primary_key.columns.push(PrimaryKeyColumn::from_string(name, value));
        self
    }

    /// 添加整数类型的结束主键查询值。无论该行是否存在，则响应中一定不会包含此行。
    pub fn end_primary_key_column_integer(mut self, name: &str, value: i64) -> Self {
        self.exclusive_end_primary_key.columns.push(PrimaryKeyColumn::from_integer(name, value));

        self
    }

    /// 添加二进制类型的结束主键查询值。无论该行是否存在，则响应中一定不会包含此行。
    pub fn end_primary_key_column_binary(mut self, name: &str, value: impl Into<Vec<u8>>) -> Self {
        self.exclusive_end_primary_key.columns.push(PrimaryKeyColumn::from_binary(name, value));

        self
    }

    /// 添加无穷小值结束主键
    pub fn end_primary_key_column_inf_min(mut self, name: &str) -> Self {
        self.exclusive_end_primary_key.columns.push(PrimaryKeyColumn::inf_min(name));

        self
    }

    /// 添加无穷大值结束主键
    pub fn end_primary_key_column_inf_max(mut self, name: &str) -> Self {
        self.exclusive_end_primary_key.columns.push(PrimaryKeyColumn::inf_max(name));

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

    /// 本次读取最多返回的行数。取值必须大于0。
    /// 如果查询到的行数超过此值，则通过响应中会包含断点来记录本次读取到的位置，以便下一次读取。
    /// 无论是否设置此项，表格存储最多返回的行数为 5000 且总数据大小不超过 4 MB。
    pub fn limit(mut self, limit: i32) -> Self {
        self.limit = Some(limit);

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

    /// 设置过滤条件
    pub fn filter(mut self, f: Filter) -> Self {
        self.filter = Some(f);

        self
    }

    /// 局部事务ID。当使用局部事务功能读取数据时必须设置此参数。
    pub fn transaction_id(mut self, tx_id: impl Into<String>) -> Self {
        self.transaction_id = Some(tx_id.into());

        self
    }

    /// 验证请求参数
    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if self.inclusive_start_primary_key.columns.is_empty() {
            return Err(OtsError::ValidationFailed("inclusive start primary key can not be empty".to_string()));
        }

        if self.exclusive_end_primary_key.columns.is_empty() {
            return Err(OtsError::ValidationFailed("exclusive end primary key can not be empty".to_string()));
        }

        if self.max_versions.is_some() && (self.time_range_start_ms.is_some() || self.time_range_end_ms.is_some() || self.time_range_specific_ms.is_some()) {
            return Err(OtsError::ValidationFailed(
                "can not set `max_versions` and `time_range` both at the same time".to_string(),
            ));
        }

        Ok(())
    }
}

impl From<GetRangeRequest> for crate::protos::GetRangeRequest {
    fn from(value: GetRangeRequest) -> crate::protos::GetRangeRequest {
        let GetRangeRequest {
            inclusive_start_primary_key,
            exclusive_end_primary_key,
            max_versions,
            direction,
            columns_to_get,
            time_range_start_ms,
            time_range_end_ms,
            time_range_specific_ms,
            limit,
            start_column,
            end_column,
            table_name,
            transaction_id,
            filter,
        } = value;

        // 时间范围和最大版本都未设置的时候，默认设置 max_versions 为 1
        let max_versions = if max_versions.is_none() && time_range_start_ms.is_none() && time_range_end_ms.is_none() && time_range_specific_ms.is_none() {
            Some(1)
        } else {
            max_versions
        };

        let start_pk_bytes = inclusive_start_primary_key.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM);
        let end_pk_bytes = exclusive_end_primary_key.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM);

        crate::protos::GetRangeRequest {
            table_name,
            direction: direction as i32,
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
            limit,
            inclusive_start_primary_key: start_pk_bytes,
            exclusive_end_primary_key: end_pk_bytes,
            filter: filter.map(|f| f.into_protobuf_bytes()),
            start_column,
            end_column,
            token: None,
            transaction_id,
        }
    }
}

/// 读取指定主键范围内的数据的响应
#[derive(Clone, Default, Debug)]
pub struct GetRangeResponse {
    pub consumed: ConsumedCapacity,
    pub rows: Vec<Row>,
    pub next_token: Option<Vec<u8>>,

    /// 本次操作的断点信息
    ///
    /// - 当返回值为空时，表示本次 `GetRange` 的响应消息中已包含请求范围内的所有数据。
    /// - 当返回值不为空时，表示本次 `GetRange` 的响应消息中只包含了 `[inclusive_start_primary_key, next_start_primary_key)` 间的数据。
    ///   如果需要继续读取剩下的数据，则需要将 `next_start_primary_key` 作为 `inclusive_start_primary_key`，原始请求中的 `exclusive_end_primary_key` 作为 `exclusive_end_primary_key` 继续执行 `GetRange` 操作。
    pub next_start_primary_key: Option<Vec<PrimaryKeyColumn>>,
}

impl TryFrom<crate::protos::GetRangeResponse> for GetRangeResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::GetRangeResponse) -> Result<Self, Self::Error> {
        let crate::protos::GetRangeResponse {
            consumed,
            rows: rows_bytes,
            next_start_primary_key,
            next_token,
        } = value;

        let next_pk = if let Some(bytes) = next_start_primary_key {
            if !bytes.is_empty() {
                let Row {
                    primary_key,
                    columns: _,
                    deleted: _,
                } = Row::decode_plain_buffer(bytes, MASK_HEADER)?;
                Some(primary_key)
            } else {
                None
            }
        } else {
            None
        };

        let mut rows = vec![];

        if !rows_bytes.is_empty() {
            let mut cursor = Cursor::new(rows_bytes);
            let header = cursor.read_u32::<LittleEndian>()?;

            if header != HEADER {
                return Err(OtsError::PlainBufferError(format!("invalid message header: {}", header)));
            }

            loop {
                if cursor.position() as usize >= cursor.get_ref().len() - 1 {
                    break;
                }

                let row = Row::read_plain_buffer(&mut cursor)?;
                rows.push(row);
            }
        }

        Ok(Self {
            consumed,
            rows,
            next_token,
            next_start_primary_key: next_pk.map(|pk| pk.columns),
        })
    }
}

/// 读取指定主键范围内的数据。
#[derive(Clone)]
pub struct GetRangeOperation {
    client: OtsClient,
    request: GetRangeRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(GetRangeOperation);

impl GetRangeOperation {
    pub(crate) fn new(client: OtsClient, request: GetRangeRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    /// 发送请求。*注意：* 如果 `time_range` 和 `max_versions` 都没有设置，则默认设置 `max_versions` 为 `1`
    pub async fn send(self) -> OtsResult<GetRangeResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::GetRangeRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::GetRange,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;
        let response_msg = crate::protos::GetRangeResponse::decode(response.bytes().await?)?;

        response_msg.try_into()
    }
}
