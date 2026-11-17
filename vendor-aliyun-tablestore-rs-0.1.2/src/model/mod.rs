//! 自定义的类型，主要是将 Protobuf 的类型映射到 Rust 类型
mod column;
mod filter;
mod primary_key;
mod row;
pub(crate) mod rules;

pub use column::*;
pub use filter::*;
pub use primary_key::*;
pub use row::*;

#[cfg(test)]
mod test_model {
    use byteorder::{LittleEndian, ReadBytesExt};

    use crate::protos::plain_buffer;

    use super::Row;

    use std::{io::Cursor, sync::Once};

    static INIT: Once = Once::new();

    fn setup() {
        INIT.call_once(|| {
            simple_logger::init_with_level(log::Level::Debug).unwrap();
            dotenvy::dotenv().unwrap();
        });
    }

    #[test]
    fn test_decode_plain_buffer() {
        setup();
        let bytes = std::fs::read("/home/yuanyq/Downloads/aliyun-plainbuffer/get-data-response-versions.data").unwrap();
        let mut cursor = Cursor::new(bytes);
        let header = cursor.read_u32::<LittleEndian>().unwrap();

        assert_eq!(plain_buffer::HEADER, header);

        let row = Row::read_plain_buffer(&mut cursor).unwrap();
        log::debug!("{:#?}", row);
    }
}
