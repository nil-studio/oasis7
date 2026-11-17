//! 时序表模块
//!

mod create_table;
mod delete_table;
mod describe_table;
mod list_table;
mod update_table;

pub use create_table::*;
pub use delete_table::*;
pub use describe_table::*;
pub use list_table::*;
pub use update_table::*;

#[cfg(test)]
mod test_timeseries_table {
    use crate::{test_util::setup, OtsClient};

    use super::{CreateTimeseriesTableRequest, UpdateTimeseriesTableRequest};

    async fn test_create_timeseries_table_impl() {
        setup();
        let client = OtsClient::from_env();

        let request = CreateTimeseriesTableRequest::new("my_ts_test");

        let resp = client.create_timeseries_table(request).send().await;

        assert!(resp.is_ok());

        log::debug!("{:?}", resp);
    }

    #[tokio::test]
    async fn test_create_timeseries_table() {
        test_create_timeseries_table_impl().await;
    }

    #[tokio::test]
    async fn test_desc_timeseries_table() {
        setup();
        let client = OtsClient::from_env();

        let resp = client.describe_timeseries_table("timeseries_demo_with_data").send().await;
        log::debug!("{:?}", resp);
    }

    #[tokio::test]
    async fn test_list_timeseries_table() {
        setup();
        let client = OtsClient::from_env();

        let resp = client.list_timeseries_table().send().await;
        log::debug!("{:?}", resp);
    }

    #[tokio::test]
    async fn test_update_timeseries_table() {
        setup();

        let client = OtsClient::from_env();
        let request = UpdateTimeseriesTableRequest::new("my_ts_test").ttl_seconds(-1);
        let resp = client.update_timeseries_table(request).send().await;

        log::debug!("{:?}", resp);
    }

    #[tokio::test]
    async fn test_delete_timeseries_table() {
        setup();
        let client = OtsClient::from_env();

        let resp = client.delete_timeseries_table("my_ts_test").send().await;
        log::debug!("{:?}", resp);
    }
}
