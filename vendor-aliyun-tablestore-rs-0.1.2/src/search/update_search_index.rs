use prost::Message;

use crate::{add_per_request_options, error::OtsError, protos::search::UpdateSearchIndexRequest, OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult};

/// 接口更新多元索引的配置，包括数据生命周期（TTL）和多元索引 schema。
///
/// **注意事项**
///
/// - 当修改多元索引生命周期时，请确保数据表为禁止更新状态（即 `allow_update` 为 `false` ）。具体操作，请参见 `UpdateTable`。
/// - 由于通过 SDK 调用 API 修改多元索引 schema 的操作较复杂，因此如需修改多元索引 schema，请通过控制台进行操作。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/updatesearchindex>
#[derive(Clone)]
pub struct UpdateSearchIndexOperation {
    client: OtsClient,
    request: UpdateSearchIndexRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(UpdateSearchIndexOperation);

impl UpdateSearchIndexRequest {
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

impl UpdateSearchIndexOperation {
    pub(crate) fn new(client: OtsClient, request: UpdateSearchIndexRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn execute(self) -> OtsResult<()> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let req = OtsRequest {
            operation: OtsOp::UpdateSearchIndex,
            body: request.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        resp.bytes().await?;

        Ok(())
    }
}
