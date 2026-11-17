use prost::Message;

use crate::{add_per_request_options, protos::DropIndexRequest, OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult};

/// 删除二级索引
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/createindex>
#[derive(Clone)]
pub struct DropIndexOperation {
    client: OtsClient,
    request: DropIndexRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(DropIndexOperation);

impl DropIndexOperation {
    pub(crate) fn new(client: OtsClient, table_name: &str, idx_name: &str) -> Self {
        Self {
            client,
            request: DropIndexRequest {
                main_table_name: table_name.to_string(),
                index_name: idx_name.to_string(),
            },
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        let Self { client, request, options } = self;

        let req = OtsRequest {
            operation: OtsOp::DropIndex,
            body: request.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        resp.bytes().await?;

        Ok(())
    }
}
