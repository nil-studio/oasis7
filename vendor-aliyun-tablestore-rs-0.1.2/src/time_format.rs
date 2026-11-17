//! Private UTC wire formatting; shared by the two original SDK clock wrappers.
//! This file is also compiled by the consuming launcher's ordinary CI tests.
use time::{OffsetDateTime, UtcOffset};

fn year(value: i32) -> String {
    if (0..=9999).contains(&value) {
        format!("{value:04}")
    } else {
        format!("{value:+05}")
    }
}

pub(crate) fn iso8601(value: OffsetDateTime) -> String {
    let value = value.to_offset(UtcOffset::UTC);
    format!(
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year(value.year()),
        value.month() as u8,
        value.day(),
        value.hour(),
        value.minute(),
        value.second(),
        value.nanosecond() / 1_000_000,
    )
}

pub(crate) fn http_date(value: OffsetDateTime) -> String {
    let value = value.to_offset(UtcOffset::UTC);
    let weekday = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        [value.weekday().number_days_from_monday() as usize];
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][value.month() as usize - 1];
    format!(
        "{weekday}, {:02} {month} {} {:02}:{:02}:{:02} GMT",
        value.day(),
        year(value.year()),
        value.hour(),
        value.minute(),
        value.second()
    )
}
