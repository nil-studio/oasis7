use aliyun_tablestore_rs::{data::GetRangeRequest, OtsClient, OtsResult};

#[tokio::main]
pub async fn main() -> OtsResult<()> {
    let client = OtsClient::new("your_ak_id", "your_ak_sec", "https://instance-name.region.ots.aliyuncs.com");
    let resp = client
        .get_range(
            GetRangeRequest::new("users")
                .start_primary_key_column_string("user_id_part", "0000")
                .start_primary_key_column_string("user_id", "0000006e-3d96-42b2-a624-d8ec9c52ad54")
                .end_primary_key_column_string("user_id_part", "0000")
                .end_primary_key_column_inf_max("user_id")
                .limit(100),
        )
        .send()
        .await?;

    for row in &resp.rows {
        println!("{:#?}", row);
    }

    Ok(())
}
