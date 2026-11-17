use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    model::{PrimaryKey, PrimaryKeyColumn, PrimaryKeyValue, Row},
    protos::{
        plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
        {compute_split_points_by_size_response::SplitLocation, ConsumedCapacity, PrimaryKeySchema},
    },
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

use crate::model::rules::validate_table_name;

/// 将全表的数据在逻辑上划分成接近指定大小的若干分片，返回这些分片之间的分割点以及分片所在机器的提示。一般用于计算引擎规划并发度等执行计划。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/computesplitpointsbysize>
#[derive(Default, Clone, Debug)]
pub struct ComputeSplitPointsBySizeRequest {
    pub table_name: String,

    /// 每个分片的近似大小，以百兆为单位。
    pub split_size: u64,

    /// 指定分割大小的单位，以便在分割点计算时使用正确的单位，并确保计算结果的准确性。
    pub split_size_unit_in_byte: Option<u64>,

    /// 指定对分割点数量的限制，以便在进行分割点计算时控制返回的分割点数量。
    pub split_point_limit: Option<u32>,
}

impl ComputeSplitPointsBySizeRequest {
    /// `split_size` 百兆为单位
    pub fn new(table_name: &str, split_size: u64) -> Self {
        Self {
            table_name: table_name.to_string(),
            split_size,
            ..Default::default()
        }
    }

    /// 指定分割大小的单位，以便在分割点计算时使用正确的单位，并确保计算结果的准确性。
    pub fn split_size_unit_in_byte(mut self, split_size_unit_in_byte: u64) -> Self {
        self.split_size_unit_in_byte = Some(split_size_unit_in_byte);
        self
    }

    /// 指定对分割点数量的限制，以便在进行分割点计算时控制返回的分割点数量。
    pub fn split_point_limit(mut self, split_point_limit: u32) -> Self {
        self.split_point_limit = Some(split_point_limit);
        self
    }

    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid table name: {}", self.table_name)));
        }

        if self.split_size > i64::MAX as u64 {
            return Err(OtsError::ValidationFailed(format!("split size: {} is too large for i64", self.split_size)));
        }

        if let Some(n) = self.split_size_unit_in_byte {
            if n > i64::MAX as u64 {
                return Err(OtsError::ValidationFailed(format!("split size unit in byte: {} is too large for i64", n)));
            }
        }

        if let Some(n) = self.split_point_limit {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("split point limit: {} is too large for i32", n)));
            }
        }

        Ok(())
    }
}

impl From<ComputeSplitPointsBySizeRequest> for crate::protos::ComputeSplitPointsBySizeRequest {
    fn from(value: ComputeSplitPointsBySizeRequest) -> Self {
        let ComputeSplitPointsBySizeRequest {
            table_name,
            split_size,
            split_size_unit_in_byte,
            split_point_limit,
        } = value;

        crate::protos::ComputeSplitPointsBySizeRequest {
            table_name,
            split_size: split_size as i64,
            split_size_unit_in_byte: split_size_unit_in_byte.map(|n| n as i64),
            split_point_limit: split_point_limit.map(|n| n as i32),
        }
    }
}

/// 例如有一张表有三列主键，其中首列主键类型为 `string`。
/// 调用该 API 后得到 `5` 个分片，分别为
///
/// 1. `(-inf,-inf,-inf)` 到 `("a",-inf,-inf)`
/// 2. `("a",-inf,-inf)` 到 `("b",-inf,-inf)`
/// 3. `("b",-inf,-inf)` 到 `("c",-inf,-inf)`
/// 4. `("c",-inf,-inf)` 到 `("d",-inf,-inf)`
/// 5. `("d",-inf,-inf)` 到 `(+inf,+inf,+inf)`
///
/// 前三个落在 "machine-A"，后两个落在 "machine-B"。
/// 那么，`split_points` 为（示意）`[("a"),("b"),("c"),("d")]`，
/// 而 `locations` 为（示意）"machine-A" * 3, "machine-B" * 2。
#[derive(Debug, Default, Clone)]
pub struct ComputeSplitPointsBySizeResponse {
    pub consumed: ConsumedCapacity,

    /// 该表的Schema，与建表时的Schema相同。
    pub schema: Vec<PrimaryKeySchema>,

    /// 分片之间的分割点。每个主键列对应的值
    pub split_points: Vec<PrimaryKey>,

    /// 分割点所在机器的提示。可以为空
    pub locations: Vec<SplitLocation>,
}

impl TryFrom<crate::protos::ComputeSplitPointsBySizeResponse> for ComputeSplitPointsBySizeResponse {
    type Error = OtsError;

    fn try_from(value: crate::protos::ComputeSplitPointsBySizeResponse) -> Result<Self, Self::Error> {
        let crate::protos::ComputeSplitPointsBySizeResponse {
            consumed,
            schema,
            split_points,
            locations,
        } = value;

        let mut split_pks = vec![];
        for row_bytes in split_points {
            if !row_bytes.is_empty() {
                let row = Row::decode_plain_buffer(row_bytes, MASK_HEADER | MASK_ROW_CHECKSUM)?;
                let mut pk = row.primary_key;
                // 把每个主键尾部省略的 `-INF` 补充回来
                for i in pk.columns.len()..schema.len() {
                    pk.columns.push(PrimaryKeyColumn::new(&schema.get(i).unwrap().name, PrimaryKeyValue::InfMin));
                }

                split_pks.push(pk);
            }
        }

        Ok(Self {
            consumed,
            schema,
            split_points: split_pks,
            locations,
        })
    }
}

#[derive(Clone)]
pub struct ComputeSplitPointsBySizeOperation {
    client: OtsClient,
    request: ComputeSplitPointsBySizeRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(ComputeSplitPointsBySizeOperation);

impl ComputeSplitPointsBySizeOperation {
    pub(crate) fn new(client: OtsClient, request: ComputeSplitPointsBySizeRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<ComputeSplitPointsBySizeResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg: crate::protos::ComputeSplitPointsBySizeRequest = request.into();

        let req = OtsRequest {
            operation: OtsOp::ComputeSplitPointsBySize,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;
        let msg = crate::protos::ComputeSplitPointsBySizeResponse::decode(response.bytes().await?)?;

        msg.try_into()
    }
}
