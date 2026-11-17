use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    protos::{DescribeTableRequest, DescribeTableResponse},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

use crate::model::rules::validate_table_name;

/// 查询指定表的结构信息以及预留读吞吐量和预留写吞吐量设置信息。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/describetable>
#[derive(Clone)]
pub struct DescribeTableOperation {
    client: OtsClient,
    table_name: String,
    options: OtsRequestOptions,
}

add_per_request_options!(DescribeTableOperation);

impl DescribeTableOperation {
    pub(crate) fn new(client: OtsClient, table_name: &str) -> Self {
        Self {
            client,
            table_name: table_name.to_string(),
            options: OtsRequestOptions::default(),
        }
    }

    fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid table name: {}", self.table_name)));
        }

        Ok(())
    }

    pub async fn send(self) -> OtsResult<DescribeTableResponse> {
        self.validate()?;

        let Self { client, table_name, options } = self;

        let body = DescribeTableRequest { table_name }.encode_to_vec();

        let req = OtsRequest {
            operation: OtsOp::DescribeTable,
            body,
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;

        Ok(DescribeTableResponse::decode(response.bytes().await?)?)
    }
}
