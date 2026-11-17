use prost::Message;

use crate::{add_per_request_options, protos::search::CreateSearchIndexRequest, OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult};

/// 接口创建一个多元索引。这个请求数据太复杂了，还是建议去控制台创建吧。Sorry
///
/// 创建多元索引前，请确保数据表的最大版本数为 `1`，数据生命周期满足如下条件中的任意一个。
///
/// - 数据表的数据生命周期为 `-1`（数据永不过期）。
/// - 数据表的数据生命周期不为 `-1` 时，数据表为禁止更新状态（即是否允许更新为否）。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/createsearchindex>
#[derive(Clone)]
pub struct CreateSearchIndexOperation {
    client: OtsClient,
    request: CreateSearchIndexRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(CreateSearchIndexOperation);

impl CreateSearchIndexOperation {
    pub(crate) fn new(client: OtsClient, request: CreateSearchIndexRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        let Self { client, request, options } = self;

        let req = OtsRequest {
            operation: OtsOp::CreateSearchIndex,
            body: request.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        resp.bytes().await?;
        Ok(())
    }
}
