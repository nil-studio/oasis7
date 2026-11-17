use prost::Message;

use crate::{
    add_per_request_options,
    protos::{ListTableRequest, ListTableResponse},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 获取当前实例下已创建的所有表的表名。
#[derive(Clone)]
pub struct ListTableOperation {
    client: OtsClient,
    options: OtsRequestOptions,
}

add_per_request_options!(ListTableOperation);

impl ListTableOperation {
    pub(crate) fn new(client: OtsClient) -> Self {
        Self {
            client,
            options: OtsRequestOptions::default(),
        }
    }

    /// Consume the builder and send request
    pub async fn send(self) -> OtsResult<Vec<String>> {
        let msg = ListTableRequest {};

        let Self { client, options } = self;

        let req = OtsRequest {
            operation: OtsOp::ListTable,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let response = client.send(req).await?;
        Ok(ListTableResponse::decode(response.bytes().await?)?.table_names)
    }
}
