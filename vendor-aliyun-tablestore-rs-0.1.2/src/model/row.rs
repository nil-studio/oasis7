use std::io::Cursor;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    crc8::crc_u8,
    error::OtsError,
    protos::plain_buffer::{
        self, HEADER, LITTLE_ENDIAN_32_SIZE, MASK_HEADER, MASK_ROW_CHECKSUM, TAG_DELETE_ROW_MARKER, TAG_ROW_CHECKSUM, TAG_ROW_DATA, TAG_ROW_PK,
    },
    OtsResult,
};

use super::{Column, ColumnOp, ColumnValue, PrimaryKey, PrimaryKeyColumn, PrimaryKeyValue};

/// 宽表模型的行
#[derive(Debug, Clone, Default)]
pub struct Row {
    /// 主键列
    pub primary_key: PrimaryKey,

    /// 数据列
    pub columns: Vec<Column>,

    /// 是否要删除行
    pub deleted: bool,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum RowType {
    PrimaryKey,
    Column,
}

impl Row {
    pub fn new() -> Self {
        Self {
            primary_key: PrimaryKey::default(),
            columns: vec![],
            deleted: false,
        }
    }

    /// 获取给定名称的主键的值
    pub fn get_primary_key_value(&self, name: &str) -> Option<&PrimaryKeyValue> {
        self.primary_key.columns.iter().find(|pk| pk.name.as_str() == name).map(|col| &col.value)
    }

    /// 获取给定名称的列的值, 适用于列在行中只出现一次的情况
    pub fn get_column_value(&self, name: &str) -> Option<&ColumnValue> {
        self.columns.iter().find(|c| c.name.as_str() == name).map(|c| &c.value)
    }

    /// 计算一个行的 plain buffer
    pub(crate) fn compute_size(&self, masks: u32) -> u32 {
        let mut size = if masks & MASK_HEADER == MASK_HEADER { LITTLE_ENDIAN_32_SIZE } else { 0u32 };

        if self.deleted {
            size += 1;
        }
        size + self.primary_key.columns.iter().map(|k| k.compute_size()).sum::<u32>() + self.columns.iter().map(|c| c.compute_size()).sum::<u32>()
    }

    /// 输出 plain buffer 的编码
    pub(crate) fn encode_plain_buffer(&self, masks: u32) -> Vec<u8> {
        let size = self.compute_size(masks);

        let mut cursor = Cursor::new(vec![0u8; size as usize]);

        if masks & MASK_HEADER == MASK_HEADER {
            cursor.write_u32::<LittleEndian>(HEADER).unwrap();
        }

        self.write_plain_buffer(&mut cursor, 0u32);

        cursor.into_inner()
    }

    /// 解码 plain buffer
    pub(crate) fn decode_plain_buffer(bytes: Vec<u8>, masks: u32) -> OtsResult<Self> {
        let mut cursor = Cursor::new(bytes);

        if masks & MASK_HEADER == MASK_HEADER {
            let header = cursor.read_u32::<LittleEndian>()?;

            if header != HEADER {
                return Err(OtsError::PlainBufferError(format!("invalid message header: {}", header)));
            }
        }

        Row::read_plain_buffer(&mut cursor)
    }

    pub(crate) fn write_plain_buffer(&self, cursor: &mut Cursor<Vec<u8>>, _masks: u32) {
        let Self { primary_key, columns, deleted } = self;

        cursor.write_u8(TAG_ROW_PK).unwrap();
        for key_col in &primary_key.columns {
            key_col.write_plain_buffer(cursor);
        }

        if !columns.is_empty() {
            cursor.write_u8(TAG_ROW_DATA).unwrap();

            for col in columns {
                col.write_plain_buffer(cursor);
            }
        }

        if *deleted {
            cursor.write_u8(TAG_DELETE_ROW_MARKER).unwrap();
        }

        cursor.write_u8(TAG_ROW_CHECKSUM).unwrap();
        cursor.write_u8(self.crc8_checksum()).unwrap();
    }

    /// 从 cursor 构建行
    pub(crate) fn read_plain_buffer(cursor: &mut Cursor<Vec<u8>>) -> OtsResult<Self> {
        let mut row_type: RowType = RowType::PrimaryKey;
        let mut pk_columns = vec![];
        let mut columns = vec![];

        loop {
            let tag = cursor.read_u8()?;
            // log::debug!("tag = 0x{:02X}, pos = 0x{:02X}, len = 0x{:02X}", tag, cursor.position(), cursor.get_ref().len());
            if cursor.position() as usize >= cursor.get_ref().len() - 1 {
                // log::debug!("read to stream end");
                break;
            }

            match tag {
                plain_buffer::TAG_ROW_PK => {
                    // log::debug!("TAG_ROW_PK read");
                    row_type = RowType::PrimaryKey;
                }

                plain_buffer::TAG_ROW_DATA => {
                    // log::debug!("TAG_ROW_DATA read");
                    row_type = RowType::Column;
                }

                plain_buffer::TAG_CELL => match row_type {
                    RowType::PrimaryKey => {
                        let pkc = PrimaryKeyColumn::read_plain_buffer(cursor)?;
                        // log::debug!("primary key column read: {:#?}", pkc);
                        pk_columns.push(pkc);
                    }

                    RowType::Column => {
                        let cell = Column::read_plain_buffer(cursor)?;
                        // log::debug!("data column read: {:#?}", cell);
                        columns.push(cell);
                    }
                },

                plain_buffer::TAG_ROW_CHECKSUM => {
                    // log::debug!("TAG_ROW_CHECKSUM read");
                    let checksum = cursor.read_u8()?;

                    let mut row_checksum = 0u8;
                    for key_col in &pk_columns {
                        // log::debug!("primary key: {:#?}", key_col);
                        row_checksum = crc_u8(row_checksum, key_col.crc8_checksum());
                    }

                    for col in &columns {
                        // log::debug!("column: {:#?}", col);
                        row_checksum = crc_u8(row_checksum, col.crc8_checksum());
                    }

                    row_checksum = crc_u8(row_checksum, 0u8);

                    if row_checksum != checksum {
                        return Err(OtsError::PlainBufferError(format!(
                            "data data checksum validation failed. calculated: {}, received: {}",
                            row_checksum, checksum
                        )));
                    }
                    break;
                }

                _ => return Err(OtsError::PlainBufferError(format!("invalid tag: {}", tag))),
            };
        }

        Ok(Self {
            primary_key: PrimaryKey { columns: pk_columns },
            columns,
            deleted: false,
        })
    }

    /// 计算整行的校验码
    pub(crate) fn crc8_checksum(&self) -> u8 {
        let mut checksum = 0u8;
        for key_col in &self.primary_key.columns {
            let cell_checksum = key_col.crc8_checksum();
            // log::debug!("primary key: {} crc8 checksum: {:02x}", key_col.name, cell_checksum);
            checksum = crc_u8(checksum, cell_checksum);
        }

        for col in &self.columns {
            let cell_checksum = col.crc8_checksum();
            // log::debug!("column key: {} crc8 checksum: {:02x}", col.name, cell_checksum);
            checksum = crc_u8(checksum, cell_checksum);
        }

        checksum = crc_u8(checksum, if self.deleted { 1u8 } else { 0u8 });
        checksum
    }

    /// 设置主键
    pub fn primary_key(mut self, pk: PrimaryKey) -> Self {
        self.primary_key = pk;

        self
    }

    /// 添加一个主键列
    pub fn primary_key_column(mut self, pk_col: PrimaryKeyColumn) -> Self {
        self.primary_key.columns.push(pk_col);

        self
    }

    /// 设置全部主键列
    pub fn primary_key_columns(mut self, pk_cols: impl IntoIterator<Item = PrimaryKeyColumn>) -> Self {
        self.primary_key.columns = pk_cols.into_iter().collect();

        self
    }

    /// 添加字符串类型的主键值
    pub fn primary_key_column_string(mut self, name: &str, value: impl Into<String>) -> Self {
        self.primary_key.columns.push(PrimaryKeyColumn::from_string(name, value));

        self
    }

    /// 添加整数类型的主键值
    pub fn primary_key_column_integer(mut self, name: &str, value: i64) -> Self {
        self.primary_key.columns.push(PrimaryKeyColumn {
            name: name.to_string(),
            value: PrimaryKeyValue::Integer(value),
        });

        self
    }

    /// 添加二进制类型的主键值
    pub fn primary_key_column_binary(mut self, name: &str, value: impl Into<Vec<u8>>) -> Self {
        self.primary_key.columns.push(PrimaryKeyColumn {
            name: name.to_string(),
            value: PrimaryKeyValue::Binary(value.into()),
        });

        self
    }

    /// 添加自增主键列
    pub fn primary_key_column_auto_increment(mut self, name: &str) -> Self {
        self.primary_key.columns.push(PrimaryKeyColumn::auto_increment(name));

        self
    }

    /// 添加一个自定义的列
    pub fn column(mut self, col: Column) -> Self {
        self.columns.push(col);

        self
    }

    /// 设置列
    pub fn columns(mut self, cols: impl IntoIterator<Item = Column>) -> Self {
        self.columns = cols.into_iter().collect();

        self
    }

    /// 添加/更新字符串类型的列
    pub fn column_string(mut self, name: &str, value: impl Into<String>) -> Self {
        self.columns.push(Column::from_string(name, value));

        self
    }

    /// 添加/更新整数列
    pub fn column_integer(mut self, name: &str, value: i64) -> Self {
        self.columns.push(Column::from_integer(name, value));

        self
    }

    /// 添加/更新双精度列
    pub fn column_double(mut self, name: &str, value: f64) -> Self {
        self.columns.push(Column::from_double(name, value));

        self
    }

    /// 添加/更新布尔值列
    pub fn column_bool(mut self, name: &str, value: bool) -> Self {
        self.columns.push(Column::from_bool(name, value));

        self
    }

    /// 添加/更新二进制列
    pub fn column_blob(mut self, name: &str, value: impl Into<Vec<u8>>) -> Self {
        self.columns.push(Column::from_blob(name, value));

        self
    }

    /// 添加要递增值的列。这个是用在 UpdateRow 的时候使用的
    pub fn column_to_increse(mut self, name: &str, inc: i64) -> Self {
        self.columns.push(Column {
            op: Some(ColumnOp::Increment),
            ..Column::from_integer(name, inc)
        });

        self
    }

    /// 添加要删除指定版本值的列
    pub fn column_to_delete(mut self, name: &str, timestamp: u64) -> Self {
        self.columns.push(Column {
            name: name.to_string(),
            value: ColumnValue::Null,
            op: Some(ColumnOp::Delete),
            timestamp: Some(timestamp),
        });

        self
    }

    /// 添加要删除全部版本的列
    pub fn column_to_delete_all_versions(mut self, name: &str) -> Self {
        self.columns.push(Column {
            name: name.to_string(),
            value: ColumnValue::Null,
            op: Some(ColumnOp::DeleteAll),
            timestamp: None,
        });

        self
    }

    /// 添加行删除标记。这个仅在删除行的时候用得到
    pub fn delete_marker(mut self) -> Self {
        self.deleted = true;

        self
    }
}

/// 行操作及行数据
#[derive(Debug, Clone)]
pub enum RowOperation {
    Put(Row),
    Update(Row),
    Delete(Row),
}

impl RowOperation {
    pub fn as_i32(&self) -> i32 {
        match self {
            Self::Put(_) => crate::protos::OperationType::Put as i32,
            Self::Update(_) => crate::protos::OperationType::Update as i32,
            Self::Delete(_) => crate::protos::OperationType::Delete as i32,
        }
    }
}

/// 将多行数据编码成一个 plain buffer
#[allow(dead_code)]
pub(crate) fn encode_plainbuf_rows(rows: Vec<Row>, masks: u32) -> Vec<u8> {
    let size = rows.iter().map(|r| r.compute_size(MASK_ROW_CHECKSUM)).sum::<u32>() as usize;
    let buf = if masks & MASK_HEADER == MASK_HEADER {
        vec![0u8; size + 4]
    } else {
        vec![0u8; size]
    };

    let mut cursor = Cursor::new(buf);

    if masks & MASK_HEADER == MASK_HEADER {
        cursor.write_u32::<LittleEndian>(HEADER).unwrap();
    }

    for row in rows {
        row.write_plain_buffer(&mut cursor, MASK_ROW_CHECKSUM);
    }

    cursor.into_inner()
}

/// 从一个响应数据中读取多行
#[allow(dead_code)]
pub(crate) fn decode_plainbuf_rows(bytes: Vec<u8>, masks: u32) -> OtsResult<Vec<Row>> {
    if bytes.is_empty() {
        return Ok(vec![]);
    }

    let mut cursor = Cursor::new(bytes);

    if masks & MASK_HEADER == MASK_HEADER {
        let header = cursor.read_u32::<LittleEndian>()?;

        if header != HEADER {
            return Err(OtsError::PlainBufferError(format!("invalid message header: {}", header)));
        }
    }

    let mut rows = Vec::new();
    while cursor.position() < (cursor.get_ref().len() - 1) as u64 {
        rows.push(Row::read_plain_buffer(&mut cursor)?);
    }

    Ok(rows)
}

#[cfg(test)]
mod test_row {
    use base64::{prelude::BASE64_STANDARD, Engine};

    use crate::{
        model::{Column, ColumnValue, PrimaryKey, PrimaryKeyColumn},
        protos::plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
        test_util::setup,
    };

    use super::Row;

    // #[tokio::test]
    // async fn get_row_no_col() {
    //     setup();
    //     let client = OtsClient::from_env();

    //     let pk = PrimaryKey {
    //         keys: vec![
    //             PrimaryKeyColumn::with_string_value("school_id", "1"),
    //             PrimaryKeyColumn::with_integer_value("id", 1742373697699000)
    //         ]
    //     };

    //     let pk_bytes = pk.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM);

    //     let msg = GetRowRequest {
    //         table_name: "schools".to_string(),
    //         primary_key: pk_bytes,
    //         columns_to_get: vec![],
    //         time_range: None,
    //         max_versions: Some(1),
    //         filter: None,
    //         start_column: None,
    //         end_column: None,
    //         token: None,
    //         transaction_id: None,
    //     };

    //     let body_bytes = msg.encode_to_vec();

    //     let req = OtsRequest {
    //         method: reqwest::Method::POST,
    //         operation: OtsOp::GetRow,
    //         body: body_bytes,
    //         ..Default::default()
    //     };

    //     let response = client.send(req).await.unwrap();
    //     let response_bytes = response.bytes().await.unwrap();
    //     let msg = GetRowResponse::decode(response_bytes).unwrap();
    //     let row_bytes = &msg.row;

    //     std::fs::write("/home/yuanyq/Downloads/aliyun-plainbuffer/row-no-col.data", &row_bytes).unwrap();
    // }

    // #[tokio::test]
    // async fn get_row_1_col() {
    //     setup();
    //     let client = OtsClient::from_env();

    //     let pk = PrimaryKey {
    //         keys: vec![
    //             PrimaryKeyColumn::with_string_value("school_id", "2"),
    //             PrimaryKeyColumn::with_integer_value("id", 1742378007415000)
    //         ]
    //     };

    //     let pk_bytes = pk.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM);

    //     let msg = GetRowRequest {
    //         table_name: "schools".to_string(),
    //         primary_key: pk_bytes,
    //         columns_to_get: vec![],
    //         time_range: None,
    //         max_versions: Some(1),
    //         filter: None,
    //         start_column: None,
    //         end_column: None,
    //         token: None,
    //         transaction_id: None,
    //     };

    //     let body_bytes = msg.encode_to_vec();

    //     let req = OtsRequest {
    //         method: reqwest::Method::POST,
    //         operation: OtsOp::GetRow,
    //         body: body_bytes,
    //         ..Default::default()
    //     };

    //     let response = client.send(req).await.unwrap();
    //     let response_bytes = response.bytes().await.unwrap();
    //     let msg = GetRowResponse::decode(response_bytes).unwrap();
    //     let row_bytes = &msg.row;

    //     std::fs::write("/home/yuanyq/Downloads/aliyun-plainbuffer/row-1-col.data", &row_bytes).unwrap();
    // }

    /// 测试主键不包含数据列的 plain buffer 编码
    #[test]
    fn test_row_no_col() {
        let md5_expected = "gpADtIzJpJRgXgSMKOUHTQ==";

        let row = Row {
            primary_key: PrimaryKey {
                columns: vec![
                    PrimaryKeyColumn::from_string("school_id", "1"),
                    PrimaryKeyColumn::from_integer("id", 1742373697699000),
                ],
            },
            columns: vec![],
            deleted: false,
        };

        let pb_bytes = row.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM);
        let md5_calc = BASE64_STANDARD.encode(md5::compute(&pb_bytes).to_vec());

        assert_eq!(md5_expected, md5_calc);
    }

    #[test]
    fn test_row_1_col() {
        setup();

        let md5_expected = "LkUq5OPGrWhSyrC7qenr2A==";

        let row = Row {
            primary_key: PrimaryKey {
                columns: vec![
                    PrimaryKeyColumn::from_string("school_id", "1"),
                    PrimaryKeyColumn::from_integer("id", 1742373697699000),
                ],
            },
            columns: vec![Column {
                name: "name".to_string(),
                value: ColumnValue::String("School-A".to_string()),
                timestamp: Some(1742378007415),
                ..Default::default()
            }],
            deleted: false,
        };

        log::debug!("row CRC8 checksum = {:02x}", row.crc8_checksum());

        let pb_bytes = row.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM);
        std::fs::write("/home/yuanyq/Downloads/aliyun-plainbuffer/row-1-col-mine.data", &pb_bytes).unwrap();
        let md5_calc = BASE64_STANDARD.encode(md5::compute(&pb_bytes).to_vec());

        assert_eq!(md5_expected, md5_calc);
    }
}
