use prost::Message;

use crate::{add_per_request_options, OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult};

#[derive(Clone)]
pub struct ListTimeseriesTableOperation {
    client: OtsClient,
    options: OtsRequestOptions,
}

add_per_request_options!(ListTimeseriesTableOperation);

impl ListTimeseriesTableOperation {
    pub(crate) fn new(client: OtsClient) -> Self {
        Self {
            client,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<crate::protos::timeseries::ListTimeseriesTableResponse> {
        let Self { client, options } = self;

        let req = OtsRequest {
            operation: OtsOp::ListTimeseriesTable,
            body: vec![],
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        let resp_msg = crate::protos::timeseries::ListTimeseriesTableResponse::decode(resp.bytes().await?)?;

        Ok(resp_msg)
    }
}
