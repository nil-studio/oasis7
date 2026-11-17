use std::io::{Cursor, Read, Seek, SeekFrom};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::{crc8::crc_bytes, error::OtsError, model::Row, OtsResult};

const API_VERSION: u32 = 0x304d5253;
const TAG_CHECKSUM: u8 = 0x01;
const TAG_ROW: u8 = 0x02;
const TAG_ROW_COUNT: u8 = 0x03;
const TAG_ENTIRE_PRIMARY_KEYS: u8 = 0x0A;

#[derive(Debug, Default)]
pub(crate) struct SimpleRowMatrix {
    total_bytes: usize,
    data_offset: u32,
    option_offset: u32,
    pk_col_count: u32,
    col_count: u32,
    // pk_count + col_count
    field_count: u32,
    field_names: Vec<String>,
    cursor: Cursor<Vec<u8>>,

    row_count: u32,

    initialized: bool,
}

impl SimpleRowMatrix {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        let data: Vec<u8> = bytes.into();
        Self {
            total_bytes: data.len(),
            cursor: Cursor::new(data),
            initialized: false,
            ..Default::default()
        }
    }

    fn initialize(&mut self) -> OtsResult<()> {
        let cursor = &mut self.cursor;

        let api_ver = cursor.read_u32::<LittleEndian>()?;

        if api_ver != API_VERSION {
            return Err(OtsError::SrmDecodeError(format!(
                "API VERSION header validation failed. Expected: {}, read: {}",
                API_VERSION, api_ver
            )));
        }

        self.data_offset = cursor.read_u32::<LittleEndian>()?;
        self.option_offset = cursor.read_u32::<LittleEndian>()?;
        self.pk_col_count = cursor.read_u32::<LittleEndian>()?;
        self.col_count = cursor.read_u32::<LittleEndian>()?;
        self.field_count = self.pk_col_count + self.col_count;

        let field_name_array_offset = cursor.position();

        cursor.set_position(self.option_offset as u64);
        let tag_entire_pk = cursor.read_u8()?;

        if tag_entire_pk != TAG_ENTIRE_PRIMARY_KEYS {
            return Err(OtsError::SrmDecodeError(format!(
                "TAG ENTIRE PRIMARY KEYS validation failed. Expected: {}, read: {}",
                TAG_ENTIRE_PRIMARY_KEYS, tag_entire_pk
            )));
        }

        let has_entire_pk = cursor.read_u8()?;

        if has_entire_pk != 0u8 && has_entire_pk != 1u8 {
            return Err(OtsError::SrmDecodeError(format!(
                "has entire primary key tag should be either 0 or 1. read: {}",
                has_entire_pk
            )));
        }

        // 这个标记位不知道是什么意思
        let _has_entire_pk = has_entire_pk == 1u8;

        let tag_row_count = cursor.read_u8()?;
        if tag_row_count != TAG_ROW_COUNT {
            return Err(OtsError::SrmDecodeError(format!(
                "TAG ROW COUNT validation failed. Expected: {}, read: {}",
                TAG_ROW_COUNT, tag_row_count
            )));
        }

        self.row_count = cursor.read_u32::<LittleEndian>()?;

        // footer, checksum
        cursor.seek(SeekFrom::End(-2))?;
        let tag_checksum = cursor.read_u8()?;
        if tag_checksum != TAG_CHECKSUM {
            return Err(OtsError::SrmDecodeError(format!(
                "TAG CHECKSUM validation failed. Expected: {}, read: {}",
                TAG_CHECKSUM, tag_checksum
            )));
        }

        let checksum = cursor.read_u8()?;
        let calculated_checksum = crc_bytes(0u8, &cursor.get_ref()[0..self.total_bytes - 1]);
        if checksum != calculated_checksum {
            return Err(OtsError::SrmDecodeError(format!(
                "checksum validation failed. Expected: {}, read: {}",
                calculated_checksum, checksum
            )));
        }

        // Parse all field names
        cursor.set_position(field_name_array_offset);
        let mut field_names = vec![];
        for _ in 0..self.field_count {
            let name_len = cursor.read_u16::<LittleEndian>()?;
            let mut name_bytes = vec![0u8; name_len as usize];
            cursor.read_exact(&mut name_bytes)?;
            field_names.push(String::from_utf8(name_bytes)?)
        }

        self.field_names = field_names;

        /*
        log::debug!(
            "primary key count: {}, column count: {}, row count: {}. fields: {:?}",
            self.pk_col_count,
            self.col_count,
            self.row_count,
            self.field_names
        );
         */

        self.initialized = true;

        Ok(())
    }

    pub fn get_rows(&mut self) -> OtsResult<Vec<Row>> {
        if !self.initialized {
            self.initialize()?;
        }

        let cursor = &mut self.cursor;
        cursor.set_position(self.data_offset as u64);

        let mut rows = vec![];
        let field_names = &self.field_names;

        loop {
            if cursor.position() >= (self.total_bytes - 3) as u64 {
                break;
            }

            let tag = cursor.read_u8()?;
            if tag != TAG_ROW {
                return Err(OtsError::SrmDecodeError(format!(
                    "TAG ROW validation failed. Expected: {}, read: {}",
                    TAG_ROW, tag
                )));
            }

            let mut row = Row::new();

            // primary key columns
            for i in 0..self.pk_col_count {
                let col_name = match field_names.get(i as usize) {
                    Some(s) => s,
                    None => return Err(OtsError::SrmDecodeError(format!("can not find field name at index: {}", i))),
                };

                let col_type = cursor.read_u8()?;
                match col_type {
                    // integer
                    0u8 => {
                        let value = cursor.read_i64::<LittleEndian>()?;
                        row = row.primary_key_column_integer(col_name, value);
                    }

                    // string
                    3u8 => {
                        let len = cursor.read_u32::<LittleEndian>()?;
                        let mut buf = vec![0u8; len as usize];
                        cursor.read_exact(&mut buf)?;
                        let s = String::from_utf8(buf)?;
                        row = row.primary_key_column_string(col_name, s);
                    }

                    // blob/binary
                    7u8 => {
                        let len = cursor.read_u32::<LittleEndian>()?;
                        let mut buf = vec![0u8; len as usize];
                        cursor.read_exact(&mut buf)?;
                        row = row.primary_key_column_binary(col_name, buf);
                    }

                    _ => return Err(OtsError::SrmDecodeError(format!("unknown primary key column data type: {}", col_type))),
                }
            }

            // attribute columns
            for i in 0..self.col_count {
                let col_name = match field_names.get((i + self.pk_col_count) as usize) {
                    Some(s) => s,
                    None => return Err(OtsError::SrmDecodeError(format!("can not find field name at index: {}", i + self.pk_col_count))),
                };

                let col_type = cursor.read_u8()?;
                match col_type {
                    // integer
                    0u8 => {
                        let value = cursor.read_i64::<LittleEndian>()?;
                        row = row.column_integer(col_name, value);
                    }

                    // double
                    1u8 => {
                        let value = cursor.read_f64::<LittleEndian>()?;
                        row = row.column_double(col_name, value);
                    }

                    // boolean
                    2u8 => {
                        let b = cursor.read_u8()?;
                        row = row.column_bool(col_name, b == 1u8);
                    }

                    // string
                    3u8 => {
                        let len = cursor.read_u32::<LittleEndian>()?;
                        let mut buf = vec![0u8; len as usize];
                        cursor.read_exact(&mut buf)?;
                        let s = String::from_utf8(buf)?;
                        row = row.column_string(col_name, s);
                    }

                    // null
                    6u8 => {}

                    // blob/binary
                    7u8 => {
                        let len = cursor.read_u32::<LittleEndian>()?;
                        let mut buf = vec![0u8; len as usize];
                        cursor.read_exact(&mut buf)?;
                        row = row.column_blob(col_name, buf);
                    }

                    _ => return Err(OtsError::SrmDecodeError(format!("unknown column data type: {}", col_type))),
                }
            }

            rows.push(row);
        }

        Ok(rows)
    }
}

#[cfg(test)]
mod test_simple_row_matrix {
    use std::sync::Once;

    use super::SimpleRowMatrix;

    static INIT: Once = Once::new();

    fn setup() {
        INIT.call_once(|| {
            simple_logger::init_with_level(log::Level::Debug).unwrap();
            dotenvy::dotenv().unwrap();
        });
    }

    #[test]
    fn test_srm() {
        setup();

        let file = "/home/yuanyq/Downloads/aliyun-plainbuffer/bulk-export-simple-matrix-no-id-col.data";
        let bytes = std::fs::read(file).unwrap();
        let rows = SimpleRowMatrix::new(bytes).get_rows();
        log::debug!("{:?}", rows);
    }
}
