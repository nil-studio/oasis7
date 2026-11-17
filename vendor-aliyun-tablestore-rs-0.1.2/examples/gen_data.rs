use std::{
    sync::Once,
    time::{SystemTime, UNIX_EPOCH},
};

use aliyun_tablestore_rs::{data::BulkImportRequest, model::Row, OtsClient};
use base64::{prelude::BASE64_STANDARD, Engine};
use fake::{
    faker::{name::zh_cn::Name, phone_number::zh_cn::PhoneNumber},
    uuid::UUIDv4,
    Fake,
};
use rand::{random_range, Rng};

static INIT: Once = Once::new();

fn setup() {
    INIT.call_once(|| {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        dotenvy::dotenv().unwrap();
    });
}

fn current_time_ms() -> u128 {
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH).unwrap().as_millis()
}

#[tokio::main]
async fn main() {
    setup();
    let client = OtsClient::from_env();

    const MAX_USERS: usize = 1000;
    const BATCH_ROWS: usize = 200;

    let mut total = 0;

    loop {
        let mut req = BulkImportRequest::new("users");

        for _ in 0..BATCH_ROWS {
            let id: String = UUIDv4.fake();
            let id_part = &id[0..4];
            let full_name: String = Name().fake();
            let phone_number: String = PhoneNumber().fake();
            let mut pwd_bytes = [0u8; 16];
            rand::rng().fill(&mut pwd_bytes);
            let pwd_hash = BASE64_STANDARD.encode(pwd_bytes);

            let n: u32 = random_range(10000000..99999999);
            let badge_no = format!("{}", n);
            let n = random_range(0..100);
            let gender = if n % 2 == 0 { "M" } else { "F" };
            let registered_at_ms = current_time_ms() as i64;
            let score: f64 = random_range(0.0f64..100.0f64);

            req = req.put_row(
                Row::new()
                    .primary_key_column_string("user_id_part", id_part)
                    .primary_key_column_string("user_id", &id)
                    .column_string("full_name", &full_name)
                    .column_string("phone_number", &phone_number)
                    .column_string("pwd_hash", &pwd_hash)
                    .column_string("badge_no", &badge_no)
                    .column_string("gender", gender)
                    .column_integer("registered_at_ms", registered_at_ms)
                    .column_double("score", score)
                    .column_bool("deleted", false),
            );
        }

        let _ = client.bulk_import(req).timeout_ms(1000).send().await.unwrap();

        total += BATCH_ROWS;

        log::info!("{}/{}", total, MAX_USERS);

        if total >= MAX_USERS {
            break;
        }
    }
}
