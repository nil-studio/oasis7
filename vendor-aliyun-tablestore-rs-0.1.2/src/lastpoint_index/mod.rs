//! 时序表 lastpoint 索引模块

mod create_lastpoint_index;
mod delete_lastpoint_index;

pub use create_lastpoint_index::*;
pub use delete_lastpoint_index::*;

#[cfg(test)]
mod test {
    use crate::{lastpoint_index::CreateTimeseriesLastpointIndexRequest, test_util::setup, OtsClient};

    async fn test_create_lastpoint_index_impl() {
        setup();

        let client = OtsClient::from_env();

        let req = CreateTimeseriesLastpointIndexRequest::new("timeseries_demo_with_data", "my_lpi");
        let resp = client.create_timeseries_lastpoint_index(req).send().await;
        log::debug!("{:?}", resp);
    }

    #[tokio::test]
    async fn test_create_lastpoint_index() {
        test_create_lastpoint_index_impl().await;
    }

    async fn test_delete_lastpoint_index_impl() {
        setup();

        let client = OtsClient::from_env();

        let resp = client.delete_timeseries_lastpoint_index("timeseries_demo_with_data", "my_lpi").send().await;
        log::debug!("{:?}", resp);
    }

    #[tokio::test]
    async fn test_delete_lastpoint_index() {
        test_delete_lastpoint_index_impl().await;
    }
}
