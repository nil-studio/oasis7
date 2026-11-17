//! 宽表模型数据操作
mod batch_get_row;
mod batch_write_row;
mod bulk_export;
mod bulk_import;
mod delete_row;
mod get_range;
mod get_row;
mod put_row;
mod update_row;

pub use batch_get_row::*;
pub use batch_write_row::*;
pub use bulk_export::*;
pub use bulk_import::*;
pub use delete_row::*;
pub use get_range::*;
pub use get_row::*;
pub use put_row::*;
pub use update_row::*;

#[cfg(test)]
mod test_row_operations {

    use fake::{faker::name::raw::Name, locales::ZH_CN, uuid::UUIDv4, Fake};

    use crate::{
        data::{DeleteRowRequest, GetRowRequest, PutRowRequest, UpdateRowRequest},
        error::OtsError,
        model::{Column, ColumnValue, CompositeColumnValueFilter, Filter, PrimaryKey, PrimaryKeyValue, Row, SingleColumnValueFilter},
        protos::{filter::LogicalOperator, Direction, ReturnType},
        test_util::setup,
        OtsClient,
    };

    use super::{
        BatchGetRowRequest, BatchWriteRowRequest, BulkExportRequest, BulkImportRequest, GetRangeRequest, RowInBatchWriteRowRequest, TableInBatchGetRowRequest,
        TableInBatchWriteRowRequest,
    };

    async fn test_get_row_impl() {
        setup();
        let client = OtsClient::from_env();
        let response = client
            .get_row(
                GetRowRequest::new("schools")
                    .primary_key_column_string("school_id", "00020FFB-BB14-CCAD-0181-A929E71C7312")
                    .primary_key_column_integer("id", 1742203524276000)
                    .max_versions(1),
            )
            .send()
            .await;

        log::debug!("get data response: \n{:?}", response);
        assert!(response.is_ok());
        let response = response.unwrap();
        assert!(response.row.is_some());
        assert!(response
            .row
            .as_ref()
            .unwrap()
            .primary_key
            .columns
            .iter()
            .any(|k| { &k.name == "school_id" && k.value == PrimaryKeyValue::String("00020FFB-BB14-CCAD-0181-A929E71C7312".to_string()) }));
    }

    #[tokio::test]
    async fn test_get_row() {
        test_get_row_impl().await;
    }

    async fn test_get_range_with_single_filter_impl() {
        setup();
        let client = OtsClient::from_env();

        let mut get_range_req = GetRangeRequest::new("ccNgMemberRecord")
            .start_primary_key_column_string("cc_id", "0080669C-3A83-4B94-8D3A-C4A1FC54EBB1")
            .start_primary_key_column_string("stat_date", "2023-12-04")
            .start_primary_key_column_inf_min("user_id")
            .start_primary_key_column_inf_min("id")
            .end_primary_key_column_string("cc_id", "0082455B-D5A7-11E8-AF2C-7CD30AC4E9EA")
            .end_primary_key_column_string("stat_date", "2023-12-04")
            .end_primary_key_column_inf_max("user_id")
            .end_primary_key_column_inf_max("id")
            .filter(crate::model::Filter::Single(
                SingleColumnValueFilter::new()
                    .equal_column(Column::from_string("cc_school_id", "A006D67B-4330-1DEF-1354-0DB43F2F5F21"))
                    .filter_if_missing(false)
                    .latest_version_only(true),
            ))
            .max_versions(1)
            .limit(1000)
            .direction(Direction::Forward);

        let mut total_row = 0;

        loop {
            let response = client.get_range(get_range_req.clone()).send().await;

            assert!(response.is_ok());
            let response = response.unwrap();

            for row in &response.rows {
                log::debug!(
                    "cc_id: {:?}, user_id: {:?}, school_id: {:?}",
                    row.get_primary_key_value("cc_id"),
                    row.get_primary_key_value("user_id"),
                    row.get_column_value("cc_school_id")
                );

                assert_eq!(
                    Some(&ColumnValue::String("A006D67B-4330-1DEF-1354-0DB43F2F5F21".to_string())),
                    row.get_column_value("cc_school_id")
                );
            }

            total_row += response.rows.len();
            log::debug!("total read: {} rows", total_row);

            if let Some(keys) = response.next_start_primary_key {
                log::debug!("Going to send next query");
                get_range_req = get_range_req.start_primary_key_columns(keys);
            } else {
                break;
            }
        }
        // log::debug!("{:#?}", response);
        // assert_eq!(2, response.rows.len());
    }

    #[tokio::test]
    async fn test_get_range_with_single_filter() {
        test_get_range_with_single_filter_impl().await;
    }

    async fn test_put_row_impl() {
        setup();

        let client = OtsClient::from_env();

        let school_id = UUIDv4.fake();

        let row = Row::default()
            .primary_key_column_string("school_id", &school_id)
            .primary_key_column_auto_increment("id")
            .column_string("name", Name(ZH_CN).fake::<String>())
            .column_string("province", Name(ZH_CN).fake::<String>());

        log::debug!("insert row into schools with school_id: {:?}", row.get_primary_key_value("school_id"));

        let response = client
            .put_row(PutRowRequest::new("schools").row(row).return_type(ReturnType::RtPk))
            .send()
            .await
            .unwrap();
        log::debug!("{:#?}", response);

        assert!(response.row.is_some());

        let row = response.row;
        assert!(row.is_some());

        let row = row.unwrap();
        assert_eq!(Some(&PrimaryKeyValue::String(school_id)), row.get_primary_key_value("school_id"));
    }

    #[tokio::test]
    async fn test_put_row() {
        test_put_row_impl().await;
    }

    async fn test_update_row_impl() {
        setup();
        let client = OtsClient::from_env();

        let table_name = "data_types";
        let id: String = UUIDv4.fake();

        log::debug!("insert new data to test update with id: {}", id);

        let response = client
            .put_row(
                PutRowRequest::new(table_name).row(
                    Row::new()
                        .primary_key_column_string("str_id", &id)
                        .column_string("str_col", "a")
                        .column_integer("int_col", 1)
                        .column_double("double_col", 1.234)
                        .column_bool("bool_col", false)
                        .column_blob("blob_col", b"a"),
                ),
            )
            .send()
            .await;

        assert!(response.is_ok());

        log::debug!("update row with id: {}", id);
        let response = client
            .update_row(
                UpdateRowRequest::new(table_name)
                    .row(
                        Row::new()
                            .primary_key_column_string("str_id", &id)
                            .column_string("str_col", "b")
                            .column_to_increse("int_col", 1)
                            .column_bool("bool_col", true)
                            .column_to_delete_all_versions("blob_col"),
                    )
                    .return_type(ReturnType::RtPk),
            )
            .send()
            .await;

        assert!(response.is_ok());

        log::debug!("update row response: {:#?}", response);

        let response = client
            .get_row(GetRowRequest::new(table_name).primary_key_column_string("str_id", &id))
            .send()
            .await;

        assert!(response.is_ok());

        let response = response.unwrap();
        let row = response.row;
        assert!(row.is_some());

        let row = row.unwrap();
        assert_eq!(Some(&ColumnValue::String("b".to_string())), row.get_column_value("str_col"));
        assert_eq!(Some(&ColumnValue::Integer(2)), row.get_column_value("int_col"));
        assert_eq!(Some(&ColumnValue::Double(1.234)), row.get_column_value("double_col"));
        assert_eq!(Some(&ColumnValue::Boolean(true)), row.get_column_value("bool_col"));
        assert_eq!(None, row.get_column_value("blob_col"));
    }

    #[tokio::test]
    async fn test_update_row() {
        test_update_row_impl().await;
    }

    async fn test_delete_row_impl() {
        setup();
        let client = OtsClient::from_env();

        let table_name = "data_types";

        let id: String = UUIDv4.fake();
        let row = Row::new()
            .primary_key_column_string("str_id", &id)
            .column_string("str_col", "hello, you are inserted to be deleted")
            .column_bool("bool_col", true);

        let req = PutRowRequest::new(table_name).row(row);

        let resp = client.put_row(req).send().await;

        assert!(resp.is_ok());

        let resp = client
            .delete_row(DeleteRowRequest::new(table_name).primary_key_column_string("str_id", &id))
            .send()
            .await;
        log::debug!("{:#?}", resp);
        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn test_delete_row() {
        test_delete_row_impl().await;
    }

    async fn test_batch_get_row_impl() {
        setup();

        let client = OtsClient::from_env();

        let t1 = TableInBatchGetRowRequest::new("data_types")
            .primary_key(PrimaryKey::new().column_string("str_id", "1"))
            .primary_key(PrimaryKey::new().column_string("str_id", "02421870-56d8-4429-a548-27e0e1f42894"));

        let t2 = TableInBatchGetRowRequest::new("schools").primary_key(
            PrimaryKey::new()
                .column_string("school_id", "00020FFB-BB14-CCAD-0181-A929E71C7312")
                .column_integer("id", 1742203524276000),
        );

        let request = BatchGetRowRequest::new().tables(vec![t1, t2]);

        let resp = client.batch_get_row(request).send().await;

        log::debug!("batch get row response: {:#?}", resp);

        assert!(resp.is_ok());

        let resp = resp.unwrap();
        assert_eq!(2, resp.tables.len());

        let tables = &resp.tables;

        assert_eq!(2, tables.first().unwrap().rows.len());

        assert_eq!(
            &Some(&PrimaryKeyValue::String("02421870-56d8-4429-a548-27e0e1f42894".to_string())),
            &tables
                .first()
                .unwrap()
                .rows
                .get(1)
                .unwrap()
                .row
                .as_ref()
                .unwrap()
                .get_primary_key_value("str_id")
        );
    }

    #[tokio::test]
    async fn test_batch_get_row() {
        test_batch_get_row_impl().await;
    }

    async fn test_batch_write_row_impl() {
        setup();
        let client = OtsClient::from_env();

        let uuid: String = UUIDv4.fake();

        let t1 = TableInBatchWriteRowRequest::new("data_types").rows(vec![
            RowInBatchWriteRowRequest::put_row(
                Row::new()
                    .primary_key_column_string("str_id", &uuid)
                    .column_string("str_col", "column is generated from batch writing"),
            ),
            RowInBatchWriteRowRequest::delete_row(Row::new().primary_key_column_string("str_id", "266e79aa-eb74-47d8-9658-e17d52fc012d")),
            RowInBatchWriteRowRequest::update_row(
                Row::new()
                    .primary_key_column_string("str_id", "975e9e17-f969-4387-9cef-a6ae9849a10d")
                    .column_double("double_col", 11.234),
            ),
        ]);

        let t2 = TableInBatchWriteRowRequest::new("schools").rows(vec![RowInBatchWriteRowRequest::update_row(
            Row::new()
                .primary_key_column_string("school_id", "2")
                .primary_key_column_integer("id", 1742378007415000)
                .column_string("name", "School-AAAA"),
        )]);

        let req = BatchWriteRowRequest::new().table(t1).table(t2);

        let resp = client.batch_write_row(req).send().await;

        log::debug!("{:#?}", resp);

        assert!(resp.is_ok());

        let tmp_res = client
            .get_row(GetRowRequest::new("data_types").primary_key_column_string("str_id", &uuid))
            .send()
            .await;

        assert!(tmp_res.is_ok());

        assert!(tmp_res.unwrap().row.is_some());
    }

    #[tokio::test]
    async fn test_batch_write_row() {
        test_batch_write_row_impl().await
    }

    /// 测试更新的时候使用过滤器
    async fn test_update_row_with_filter_impl() {
        setup();

        let client = OtsClient::from_env();

        let uuid: String = UUIDv4.fake();

        let new_row = Row::new()
            .primary_key_column_string("str_id", &uuid)
            .column_string("str_col", "1")
            .column_bool("bool_col", false);

        let resp = client.put_row(PutRowRequest::new("data_types").row(new_row)).send().await;

        assert!(resp.is_ok());

        let update_row = Row::new()
            .primary_key_column_string("str_id", &uuid)
            .column_string("str_col", "2")
            .column_bool("bool_col", false);

        let resp = client
            .update_row(
                UpdateRowRequest::new("data_types").row(update_row).column_condition(Filter::Composite(
                    CompositeColumnValueFilter::new(LogicalOperator::LoAnd)
                        .sub_filter(Filter::Single(SingleColumnValueFilter::new().equal_column(Column::from_string("str_col", "2"))))
                        .sub_filter(Filter::Single(
                            SingleColumnValueFilter::new().equal_column(Column::from_string("bool_col", "false")),
                        )),
                )),
            )
            .send()
            .await;

        assert!(resp.is_err());

        if let Err(OtsError::ApiError(apie)) = resp {
            let crate::protos::Error {
                code,
                message: _,
                access_denied_detail: _,
            } = *apie;

            assert_eq!("OTSConditionCheckFail", code);
        } else {
            panic!("the update operation should be failed with api error code: OTSConditionCheckFail")
        }
    }

    #[tokio::test]
    async fn test_update_row_with_filter() {
        test_update_row_with_filter_impl().await;
    }

    async fn test_bulk_import_impl() {
        setup();
        let client = OtsClient::from_env();
        let mut req = BulkImportRequest::new("data_types");
        for i in 0..200 {
            let id: String = UUIDv4.fake();
            let mut blob_val = [0u8; 16];
            rand::fill(&mut blob_val);

            let bool_val = i % 2 == 0;
            let double_val = rand::random_range::<f64, _>(0.0f64..99.99f64);
            let int_val = rand::random_range::<i64, _>(0..10000);
            let str_val: String = Name(ZH_CN).fake();

            let row = Row::new()
                .primary_key_column_string("str_id", &id)
                .column_blob("blob_col", blob_val)
                .column_bool("bool_col", bool_val)
                .column_double("double_col", double_val)
                .column_integer("int_col", int_val)
                .column_string("str_col", &str_val);

            req = req.put_row(row);
        }

        let resp = client.bulk_import(req).send().await;

        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn test_bulk_import() {
        test_bulk_import_impl().await
    }

    async fn test_bulk_export_impl() {
        setup();
        let client = OtsClient::from_env();

        let mut start_pk = PrimaryKey::new().column_inf_min("str_id");

        let mut total_rows = 0;

        loop {
            let request = BulkExportRequest::new("data_types")
                .start_primary_key(start_pk.clone())
                .end_primary_key_column_inf_max("str_id")
                .columns_to_get(["str_id", "str_col", "int_col", "double_col", "blob_col", "bool_col"]);

            let resp = client.bulk_export(request).send().await;
            let resp = resp.unwrap();
            total_rows += resp.rows.len();

            resp.rows.iter().for_each(|r| {
                log::debug!("row: {:?}", r.get_primary_key_value("str_id").unwrap());
            });

            match resp.next_start_primary_key {
                Some(next_pk) => start_pk = next_pk,
                None => break,
            }
        }

        log::debug!("total read rows: {}", total_rows);
    }

    #[tokio::test]
    async fn test_bulk_export() {
        test_bulk_export_impl().await
    }
}
