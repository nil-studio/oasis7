//! Actual SDK loopback plus the exact private formatter compiled by that SDK.
//! No cloud credentials or live service calls. This is an ordinary launcher test.
#[path = "../../../../../vendor-aliyun-tablestore-rs-0.1.2/src/time_format.rs"]
mod sdk_time;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac_legacy::{Hmac, Mac};
use sha1::Sha1;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};
use time::{Date, Month, OffsetDateTime, UtcOffset};

fn at(
    year: i32,
    month: Month,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    nano: u32,
) -> OffsetDateTime {
    Date::from_calendar_date(year, month, day)
        .unwrap()
        .with_hms_nano(hour, minute, second, nano)
        .unwrap()
        .assume_utc()
}
fn signature(message: &str) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(b"fixture-secret").unwrap();
    mac.update(message.as_bytes());
    STANDARD.encode(mac.finalize().into_bytes())
}
#[test]
fn sdk_private_dates_preserve_utc_wire_bytes() {
    let cases = [
        (
            OffsetDateTime::UNIX_EPOCH,
            "1970-01-01T00:00:00.000Z",
            "Thu, 01 Jan 1970 00:00:00 GMT",
        ),
        (
            OffsetDateTime::from_unix_timestamp_nanos(-1).unwrap(),
            "1969-12-31T23:59:59.999Z",
            "Wed, 31 Dec 1969 23:59:59 GMT",
        ),
        (
            at(1994, Month::November, 6, 8, 49, 37, 999_999_999),
            "1994-11-06T08:49:37.999Z",
            "Sun, 06 Nov 1994 08:49:37 GMT",
        ),
        (
            at(2024, Month::February, 29, 12, 34, 56, 123_999_999),
            "2024-02-29T12:34:56.123Z",
            "Thu, 29 Feb 2024 12:34:56 GMT",
        ),
        (
            at(0, Month::January, 1, 0, 0, 0, 0),
            "0000-01-01T00:00:00.000Z",
            "Sat, 01 Jan 0000 00:00:00 GMT",
        ),
        (
            at(-1, Month::January, 1, 0, 0, 0, 0),
            "-0001-01-01T00:00:00.000Z",
            "Fri, 01 Jan -0001 00:00:00 GMT",
        ),
        (
            at(9999, Month::December, 31, 23, 59, 59, 999_999_999),
            "9999-12-31T23:59:59.999Z",
            "Fri, 31 Dec 9999 23:59:59 GMT",
        ),
    ];
    for (value, iso, http) in cases {
        assert_eq!(sdk_time::iso8601(value), iso);
        assert_eq!(sdk_time::http_date(value), http);
        if value.year() < 9999 {
            let shifted = value.to_offset(UtcOffset::from_hms(8, 0, 0).unwrap());
            assert_eq!(sdk_time::iso8601(shifted), iso);
            assert_eq!(sdk_time::http_date(shifted), http);
        }
    }
}
#[test]
fn sdk_date_fixed_request_signature_preserves_trailing_newline() {
    let date = sdk_time::iso8601(at(2024, Month::February, 29, 12, 34, 56, 123_999_999));
    let message = format!("/GetRow\nPOST\n\nx-ots-date:{date}\nx-ots-instancename:fixture\n");
    assert_eq!(signature(&message), "azPrBBPxDQ7SY0bUW/f+iaa4azs=");
    assert_ne!(
        signature(message.trim_end_matches('\n')),
        signature(&message)
    );
}
#[test]
fn actual_sdk_loopback_uses_compatible_date_and_request_signature() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("bounded fixture accept failed: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("fixture header absolute deadline exceeded");
            stream
                .set_read_timeout(Some(remaining.min(Duration::from_secs(5))))
                .unwrap();
            assert!(request.len() < 16 * 1024);
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        // ListTable's protobuf request/response are both empty for this fixture.
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
        String::from_utf8(request).unwrap()
    });
    let before = OffsetDateTime::now_utc();
    let client = aliyun_tablestore_rs::OtsClient::builder("fixture-ak", "fixture-secret")
        .endpoint(endpoint)
        .instance_name("fixture")
        .region("fixture-region")
        .rety_policy(Box::new(aliyun_tablestore_rs::DefaultRetryPolicy {
            max_retry_times: 0,
        }))
        .build();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(client.list_table().timeout_ms(5_000).send());
    let after = OffsetDateTime::now_utc();
    let request = server.join().unwrap();
    assert!(result.unwrap().is_empty());
    let mut lines = request.lines();
    assert_eq!(lines.next().unwrap(), "POST /ListTable HTTP/1.1");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
        .collect::<BTreeMap<_, _>>();
    let date = &headers["x-ots-date"];
    assert_eq!(date.len(), 24);
    assert!(date.is_ascii());
    assert_eq!(&date[19..20], ".");
    assert_eq!(&date[23..], "Z");
    let parsed = at(
        date[0..4].parse().unwrap(),
        Month::try_from(date[5..7].parse::<u8>().unwrap()).unwrap(),
        date[8..10].parse().unwrap(),
        date[11..13].parse().unwrap(),
        date[14..16].parse().unwrap(),
        date[17..19].parse().unwrap(),
        date[20..23].parse::<u32>().unwrap() * 1_000_000,
    );
    assert_eq!(sdk_time::iso8601(parsed), *date);
    assert!(
        parsed.unix_timestamp() >= before.unix_timestamp()
            && parsed.unix_timestamp() <= after.unix_timestamp()
    );
    assert_eq!(headers["x-ots-signdate"], date[..10].replace('-', ""));
    let canonical = headers
        .iter()
        .filter(|(name, _)| name.starts_with("x-ots-") && name.as_str() != "x-ots-signature")
        .map(|(name, value)| format!("{name}:{value}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        headers["x-ots-signature"],
        signature(&format!("/ListTable\nPOST\n\n{canonical}\n"))
    );
}
