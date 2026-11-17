use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    protos::search::{DescribeSearchIndexRequest, DescribeSearchIndexResponse},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 查询多元索引描述信息，包括多元索引的字段信息和索引配置等。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/describesearchindex>
#[derive(Clone)]
pub struct DescribeSearchIndexOperation {
    client: OtsClient,
    request: DescribeSearchIndexRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(DescribeSearchIndexOperation);

impl DescribeSearchIndexRequest {
    fn validate(&self) -> OtsResult<()> {
        if self.table_name.is_none() || self.table_name.as_ref().unwrap().is_empty() {
            return Err(OtsError::ValidationFailed("table name must not be empty".to_string()));
        }

        if self.index_name.is_none() || self.index_name.as_ref().unwrap().is_empty() {
            return Err(OtsError::ValidationFailed("index name must not be empty".to_string()));
        }

        Ok(())
    }
}

impl DescribeSearchIndexOperation {
    pub(crate) fn new(client: OtsClient, table_name: &str, index_name: &str) -> Self {
        Self {
            client,
            request: DescribeSearchIndexRequest {
                table_name: Some(table_name.to_string()),
                index_name: Some(index_name.to_string()),
                include_sync_stat: Some(true),
            },
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<DescribeSearchIndexResponse> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let req = OtsRequest {
            operation: OtsOp::DescribeSearchIndex,
            body: request.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        Ok(DescribeSearchIndexResponse::decode(resp.bytes().await?)?)
    }
}
