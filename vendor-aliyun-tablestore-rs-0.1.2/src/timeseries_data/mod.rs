//! 时序数据

mod delete_meta;
mod get_data;
mod put_data;
mod query_meta;
mod scan_data;
mod split_scan;
mod update_meta;

pub use delete_meta::*;
pub use get_data::*;
pub use put_data::*;
pub use query_meta::*;
pub use scan_data::*;
pub use split_scan::*;
pub use update_meta::*;

#[cfg(test)]
mod test_timeseries_data {
    use crate::{
        protos::timeseries::MetaQueryCompositeOperator,
        test_util::setup,
        timeseries_data::SplitTimeseriesScanTaskRequest,
        timeseries_model::{CompositeMetaQuery, DatasourceMetaQuery, MeasurementMetaQuery, MetaQuery, TimeseriesKey, TimeseriesMeta, TimeseriesRow},
        util::current_time_ms,
        OtsClient,
    };

    use super::{
        DeleteTimeseriesMetaRequest, GetTimeseriesDataRequest, PutTimeseriesDataRequest, QueryTimeseriesMetaRequest, ScanTimeseriesDataRequest,
        UpdateTimeseriesMetaRequest,
    };

    /// Test query timeseries data
    async fn test_get_timeseries_data_impl() {
        setup();
        let client = OtsClient::from_env();

        let request = GetTimeseriesDataRequest::new(
            "timeseries_demo_with_data",
            TimeseriesKey::new()
                .measurement_name("measure_7")
                .datasource("data_3")
                .tag("cluster", "cluster_3")
                .tag("region", "region_7"),
        )
        .end_time_us(1744119422199000)
        .limit(10);

        let resp = client.get_timeseries_data(request).send().await;
        log::debug!("{:?}", resp);
        assert!(resp.is_ok());

        let resp = resp.unwrap();
        for row in &resp.rows {
            assert_eq!(&Some("measure_7".to_string()), &row.key.measurement_name);

            for col in &row.fields {
                log::debug!("{}: {} => {:?}", row.timestamp_us, col.name, col.value);
            }
        }

        log::debug!("total rows returned: {}", resp.rows.len());
    }

    #[tokio::test]
    async fn test_get_timeseries_data() {
        test_get_timeseries_data_impl().await;
    }

    async fn test_put_timeseries_data_impl() {
        setup();

        let client = OtsClient::from_env();

        let ts_us = (current_time_ms() * 1000) as u64;

        let request = PutTimeseriesDataRequest::new("timeseries_demo_with_data")
            .row(
                TimeseriesRow::new()
                    .measurement_name("measure_11")
                    .datasource("data_11")
                    .tag("cluster", "cluster_11")
                    .tag("region", "region_11")
                    .timestamp_us(ts_us)
                    .field_integer("temp", 123),
            )
            .row(
                TimeseriesRow::new()
                    .measurement_name("measure_11")
                    .datasource("data_11")
                    .tag("cluster", "cluster_11")
                    .tag("region", "region_11")
                    .timestamp_us(ts_us + 1000)
                    .field_double("temp", 543.21),
            );

        let resp = client.put_timeseries_data(request).send().await;

        log::debug!("{:?}", resp);
    }

    #[tokio::test]
    async fn test_put_timeseries_data() {
        test_put_timeseries_data_impl().await
    }

    async fn test_query_timeseries_meta_impl() {
        setup();
        let client = OtsClient::from_env();

        let req = QueryTimeseriesMetaRequest::new(
            "timeseries_demo_with_data",
            MetaQuery::Measurement(MeasurementMetaQuery::Equal("measure_11".to_string())),
        )
        .get_total_hit(true);

        let resp = client.query_timeseries_meta(req).send().await;
        log::debug!("{:?}", resp);

        let resp = resp.unwrap();
        for m in &resp.metas {
            assert_eq!(&Some("measure_11".to_string()), &m.key.measurement_name);
        }

        let req = QueryTimeseriesMetaRequest::new(
            "timeseries_demo_with_data",
            MetaQuery::Composite(Box::new(
                CompositeMetaQuery::new(MetaQueryCompositeOperator::OpAnd)
                    .sub_query(MetaQuery::Measurement(MeasurementMetaQuery::Equal("measure_7".to_string())))
                    .sub_query(MetaQuery::Datasource(DatasourceMetaQuery::Equal("data_3".to_string()))),
            )),
        )
        .get_total_hit(true);

        let resp = client.query_timeseries_meta(req).send().await;

        assert!(resp.is_ok());

        let resp = resp.unwrap();

        assert!(resp.total_hit.is_some());
        if let Some(n) = resp.total_hit {
            assert!(n > 0)
        } else {
            panic!("shoule more than 1 row");
        };

        for m in &resp.metas {
            assert_eq!(&Some("measure_7".to_string()), &m.key.measurement_name);
            assert_eq!(&Some("data_3".to_string()), &m.key.datasource);
        }
    }

    #[tokio::test]
    async fn test_query_timeseries_meta() {
        test_query_timeseries_meta_impl().await
    }

    async fn test_query_timeseries_meta_with_attributes_impl() {
        setup();
        let client = OtsClient::from_env();

        let req = QueryTimeseriesMetaRequest::new(
            "timeseries_demo_with_data",
            MetaQuery::Measurement(MeasurementMetaQuery::Equal("measure_12".to_string())),
        )
        .get_total_hit(true);

        let resp = client.query_timeseries_meta(req).send().await;
        log::debug!("{:?}", resp);
    }

    #[tokio::test]
    async fn test_query_timeseries_meta_with_attributes() {
        test_query_timeseries_meta_with_attributes_impl().await
    }

    async fn test_update_timeseries_meta_impl() {
        setup();

        let client = OtsClient::from_env();

        let req = UpdateTimeseriesMetaRequest::new("timeseries_demo_with_data").meta(
            TimeseriesMeta::new()
                .measurement_name("measure_13")
                .datasource("data_13")
                .attribute("attr1", "value"),
        );

        let resp = client.update_timeseries_meta(req).send().await;

        assert!(resp.is_ok());

        log::debug!("{:#?}", resp);
    }

    #[tokio::test]
    async fn test_update_timeseries_meta() {
        test_update_timeseries_meta_impl().await
    }

    async fn test_delete_timeseries_meta_impl() {
        setup();
        let client = OtsClient::from_env();
        let req = DeleteTimeseriesMetaRequest::new("timeseries_demo_with_data").key(TimeseriesKey::new().measurement_name("measure_13").datasource("data_13"));

        let resp = client.delete_timeseries_meta(req).send().await;

        log::debug!("{:#?}", resp);

        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn test_delete_timeseries_meta() {
        test_delete_timeseries_meta_impl().await;
    }

    #[tokio::test]
    async fn test_split_timeseries_scan_task() {
        setup();

        let client = OtsClient::from_env();

        let resp = client
            .split_timeseries_scan_task(SplitTimeseriesScanTaskRequest::new("timeseries_demo_with_data", 1))
            .send()
            .await;

        log::debug!("{:#?}", resp);
    }

    async fn test_scan_timeseries_data_impl() {
        setup();
        let client = OtsClient::from_env();

        let req = ScanTimeseriesDataRequest::new("timeseries_demo_with_data").limit(10);
        let resp = client.scan_timeseries_data(req).send().await;
        log::debug!("{:?}", resp);
    }

    #[tokio::test]
    async fn test_scan_timeseries_data() {
        test_scan_timeseries_data_impl().await;
    }
}
