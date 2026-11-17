//! 时序表分析存储模块
//!

mod create_analytical_store;
mod delete_analytical_store;
mod describe_analytical_store;
mod update_analytical_store;

pub use create_analytical_store::*;
pub use delete_analytical_store::*;
pub use describe_analytical_store::*;
pub use update_analytical_store::*;

#[cfg(test)]
mod test_analytical_store {
    use crate::{test_util::setup, OtsClient};

    use super::CreateTimeseriesAnalyticalStoreRequest;

    #[tokio::test]
    async fn test_describe_analytical_store() {
        setup();

        let client = OtsClient::from_env();
        let resp = client
            .describe_timeseries_analytical_store("timeseries_demo_with_data", "default_analytical_store")
            .send()
            .await;
        log::debug!("{:?}", resp);
    }

    #[tokio::test]
    async fn test_create_analytical_store() {
        setup();

        let client = OtsClient::from_env();
        let resp = client
            .create_timeseries_analytical_store(CreateTimeseriesAnalyticalStoreRequest::new(
                "timeseries_demo_with_data",
                "default_analytical_store",
            ))
            .send()
            .await;
        log::debug!("{:?}", resp);
    }
}
