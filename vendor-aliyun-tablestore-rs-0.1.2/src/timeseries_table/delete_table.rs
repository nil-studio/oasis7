use prost::Message;

use crate::{
    add_per_request_options, error::OtsError, timeseries_model::rules::validate_timeseries_table_name, OtsClient, OtsOp, OtsRequest, OtsRequestOptions,
    OtsResult,
};

/// 删除指定时序表
pub struct DeleteTimeseriesTableOperation {
    client: OtsClient,
    table_name: String,
    options: OtsRequestOptions,
}

add_per_request_options!(DeleteTimeseriesTableOperation);

impl DeleteTimeseriesTableOperation {
    pub(crate) fn new(client: OtsClient, table_name: &str) -> Self {
        Self {
            client,
            table_name: table_name.to_string(),
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        if !validate_timeseries_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        let Self { client, table_name, options } = self;
        let msg = crate::protos::timeseries::DeleteTimeseriesTableRequest { table_name };

        let req = OtsRequest {
            operation: OtsOp::DeleteTimeseriesTable,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        resp.bytes().await?;

        Ok(())
    }
}
