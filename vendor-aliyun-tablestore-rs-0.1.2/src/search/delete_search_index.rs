use prost::Message;

use crate::{add_per_request_options, error::OtsError, protos::search::DeleteSearchIndexRequest, OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult};

/// 删除一个多元索引。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/deletesearchindex>
#[derive(Clone)]
pub struct DeleteSearchIndexOperation {
    client: OtsClient,
    request: DeleteSearchIndexRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(DeleteSearchIndexOperation);

impl DeleteSearchIndexOperation {
    pub(crate) fn new(client: OtsClient, table_name: &str, index_name: &str) -> Self {
        Self {
            client,
            request: DeleteSearchIndexRequest {
                table_name: Some(table_name.to_string()),
                index_name: Some(index_name.to_string()),
            },
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let req = OtsRequest {
            operation: OtsOp::DeleteSearchIndex,
            body: request.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;

        resp.bytes().await?;

        Ok(())
    }
}

impl DeleteSearchIndexRequest {
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
