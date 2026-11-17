use std::io::{Cursor, Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    crc8::{crc_bytes, crc_i64, crc_u32, crc_u8},
    error::OtsError,
    protos::plain_buffer::{
        self, HEADER, LITTLE_ENDIAN_32_SIZE, LITTLE_ENDIAN_64_SIZE, MASK_HEADER, MASK_ROW_CHECKSUM, TAG_CELL, TAG_CELL_CHECKSUM, TAG_CELL_NAME, TAG_CELL_VALUE,
        TAG_ROW_CHECKSUM, VT_AUTO_INCREMENT, VT_BLOB, VT_INF_MAX, VT_INF_MIN, VT_INTEGER, VT_STRING,
    },
    OtsResult,
};

/// 主键容器
#[derive(Debug, Clone, Default)]
pub struct PrimaryKey {
    pub columns: Vec<PrimaryKeyColumn>,
}

impl PrimaryKey {
    /// 返回的长度：1 字节的 TAG_ROW_PK + 各 Key 的长度 + 行校验码（看掩码）。
    ///
    /// 只有当一行数据只有 PK 的时候，才把它当整行处理，需要补充行校验码，例如：在 GetRow 操作中。
    ///
    /// 0x01 - TAG_ROW_PK
    /// 0x00 ... Keys size
    pub(crate) fn compute_size(&self, masks: u32) -> u32 {
        let mut size = 1u32 + self.columns.iter().map(|k| k.compute_size()).sum::<u32>();

        if masks & MASK_ROW_CHECKSUM == MASK_ROW_CHECKSUM {
            size += 2;
        }

        if masks & MASK_HEADER == MASK_HEADER {
            size += 4;
        }

        size
    }

    /// Consume self and output plain buffer data
    pub(crate) fn encode_plain_buffer(&self, masks: u32) -> Vec<u8> {
        let size = self.compute_size(masks);

        let bytes = vec![0u8; size as usize];
        let mut cursor = Cursor::new(bytes);

        if masks & MASK_HEADER == MASK_HEADER {
            cursor.write_u32::<LittleEndian>(HEADER).unwrap();
        }

        self.write_plain_buffer(&mut cursor, masks);

        cursor.into_inner()
    }

    /// Write data to cursor
    pub(crate) fn write_plain_buffer(&self, cursor: &mut Cursor<Vec<u8>>, masks: u32) {
        let Self { columns: keys } = self;

        cursor.write_u8(plain_buffer::TAG_ROW_PK).unwrap();

        for key_col in keys {
            key_col.write_plain_buffer(cursor);
        }

        if masks & MASK_ROW_CHECKSUM == MASK_ROW_CHECKSUM {
            cursor.write_u8(TAG_ROW_CHECKSUM).unwrap();
            cursor.write_u8(self.crc8_checksum()).unwrap();
        }
    }

    /// 计算主键的一行的校验码
    pub(crate) fn crc8_checksum(&self) -> u8 {
        let mut c = 0u8;
        for key_col in &self.columns {
            c = crc_u8(c, key_col.crc8_checksum());
        }

        c = crc_u8(c, 0u8);
        c
    }

    pub fn new() -> Self {
        Self { columns: vec![] }
    }

    /// 添加一个主键列
    pub fn column(mut self, pk_col: PrimaryKeyColumn) -> Self {
        self.columns.push(pk_col);

        self
    }

    /// 设置全部主键列
    pub fn columns(mut self, pk_cols: impl IntoIterator<Item = PrimaryKeyColumn>) -> Self {
        self.columns = pk_cols.into_iter().collect();

        self
    }

    /// 添加字符串类型的主键列
    pub fn column_string(mut self, name: &str, value: impl Into<String>) -> Self {
        self.columns.push(PrimaryKeyColumn::from_string(name, value));
        self
    }

    /// 添加整数类型的主键列
    pub fn column_integer(mut self, name: &str, value: i64) -> Self {
        self.columns.push(PrimaryKeyColumn::from_integer(name, value));

        self
    }

    /// 添加二进制类型的主键列
    pub fn column_binary(mut self, name: &str, value: impl Into<Vec<u8>>) -> Self {
        self.columns.push(PrimaryKeyColumn::from_binary(name, value));

        self
    }

    /// 添加一个极小值列。范围查询时可以使用
    pub fn column_inf_min(mut self, name: &str) -> Self {
        self.columns.push(PrimaryKeyColumn::inf_min(name));

        self
    }

    /// 添加一个极大值列。范围查询时可以使用
    pub fn column_info_max(mut self, name: &str) -> Self {
        self.columns.push(PrimaryKeyColumn::inf_max(name));

        self
    }

    /// 添加一个自增主键列。这个主要是在写入数据的使用用得到，查询的时候用不上
    pub fn column_auto_increment(mut self, name: &str) -> Self {
        self.columns.push(PrimaryKeyColumn::auto_increment(name));

        self
    }
}

/// 主键值
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimaryKeyValue {
    Integer(i64),
    String(String),
    Binary(Vec<u8>),

    /// 无穷大。主要是用来查询
    InfMax,

    /// 无穷小。主要是用来查询
    InfMin,

    /// 自增
    AutoIncrement,
}

impl Default for PrimaryKeyValue {
    fn default() -> Self {
        Self::Integer(0)
    }
}

impl PrimaryKeyValue {
    /// 返回的长度包含：4 字节前缀 + 1 字节类型 + 4 字节值的长度（仅针对 String 和 Binary）+ 值的实际数据长度
    ///
    /// 0x00 0x00 0x00 0x00 - Marker?, 4 bytes le
    ///                     - Integer: 0x08 + 0x01 = 0x09
    ///                     - String:  0x04 + 0x01 + string bytes len
    ///                     - Binary:  0x04 + 0x01 + bytes len
    ///                     - Inf min: 0x01
    ///                     - Inf max: 0x01
    /// 0x00                - Cell value type, 1 byte
    ///                     - Integer: 0x00
    ///                     - String:  0x03
    ///                     - Binary:  0x07
    ///                     - Inf min: 0x09, for query
    ///                     - Inf max: 0x0A, for query
    /// 0x00 ...            - Cell value
    ///                     - Integer: 8 bytes
    ///                     - String:  string utf8 bytes
    ///                     - Binary:  bytes
    ///                     - Inf min: 1 byte
    ///                     - Inf max: 1 byte
    pub(crate) fn compute_size(&self) -> u32 {
        let size = plain_buffer::LITTLE_ENDIAN_32_SIZE + 1;

        match self {
            Self::Integer(_) => size + LITTLE_ENDIAN_64_SIZE,
            Self::String(s) => size + plain_buffer::LITTLE_ENDIAN_32_SIZE + s.len() as u32,
            Self::Binary(buf) => size + plain_buffer::LITTLE_ENDIAN_32_SIZE + buf.len() as u32,
            Self::InfMax | Self::InfMin | Self::AutoIncrement => size,
        }
    }

    pub(crate) fn crc8_checksum(&self, input_checksum: u8) -> u8 {
        let mut checksum = input_checksum;

        match self {
            Self::InfMin => crc_u8(checksum, VT_INF_MIN),
            Self::InfMax => crc_u8(checksum, VT_INF_MAX),
            Self::AutoIncrement => crc_u8(checksum, VT_AUTO_INCREMENT),
            // Self::AutoIncrement => crc_u8(crc, VT_AUTO_INCREMENT),
            Self::Integer(n) => {
                checksum = crc_u8(checksum, VT_INTEGER);
                crc_i64(checksum, *n)
            }

            Self::String(s) => {
                checksum = crc_u8(checksum, VT_STRING);
                checksum = crc_u32(checksum, s.len() as u32);
                crc_bytes(checksum, s.as_bytes())
            }

            Self::Binary(buf) => {
                checksum = crc_u8(checksum, VT_BLOB);
                checksum = crc_u32(checksum, buf.len() as u32);
                crc_bytes(checksum, buf)
            }
        }
    }

    /// Consume self and write plain buffer.
    pub(crate) fn write_plain_buffer(&self, cursor: &mut Cursor<Vec<u8>>) {
        match self {
            Self::Integer(n) => {
                cursor.write_u32::<LittleEndian>(LITTLE_ENDIAN_64_SIZE + 1).unwrap();
                cursor.write_u8(VT_INTEGER).unwrap();
                cursor.write_i64::<LittleEndian>(*n).unwrap();
            }

            Self::String(s) => {
                cursor.write_u32::<LittleEndian>(1 + LITTLE_ENDIAN_32_SIZE + s.len() as u32).unwrap();
                cursor.write_u8(VT_STRING).unwrap();
                cursor.write_u32::<LittleEndian>(s.len() as u32).unwrap();
                cursor.write_all(s.as_bytes()).unwrap();
            }

            Self::Binary(buf) => {
                cursor.write_u32::<LittleEndian>(1 + LITTLE_ENDIAN_32_SIZE + buf.len() as u32).unwrap();
                cursor.write_u8(VT_BLOB).unwrap();
                cursor.write_u32::<LittleEndian>(buf.len() as u32).unwrap();
                cursor.write_all(buf).unwrap();
            }

            Self::InfMin => {
                cursor.write_u32::<LittleEndian>(1).unwrap();
                cursor.write_u8(VT_INF_MIN).unwrap();
            }

            Self::InfMax => {
                cursor.write_u32::<LittleEndian>(1).unwrap();
                cursor.write_u8(VT_INF_MAX).unwrap();
            }

            Self::AutoIncrement => {
                cursor.write_u32::<LittleEndian>(1).unwrap();
                cursor.write_u8(VT_AUTO_INCREMENT).unwrap();
            }
        }
    }
}

/// 主键列
#[derive(Debug, Clone, Default)]
pub struct PrimaryKeyColumn {
    /// 列名
    pub name: String,

    /// 列的值
    pub value: PrimaryKeyValue,
}

impl PrimaryKeyColumn {
    pub fn new(name: &str, value: PrimaryKeyValue) -> Self {
        Self { name: name.to_string(), value }
    }

    /// 创建字符串类型的主键列及值
    pub fn from_string(name: &str, value: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            value: PrimaryKeyValue::String(value.into()),
        }
    }

    /// 创建整数类型的主键列及值
    pub fn from_integer(name: &str, value: i64) -> Self {
        Self {
            name: name.to_string(),
            value: PrimaryKeyValue::Integer(value),
        }
    }

    /// 创建二进制类型的主键列及值
    pub fn from_binary(name: &str, value: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.to_string(),
            value: PrimaryKeyValue::Binary(value.into()),
        }
    }

    /// 创建无穷小值的主键列
    pub fn inf_min(name: &str) -> Self {
        Self {
            name: name.to_string(),
            value: PrimaryKeyValue::InfMin,
        }
    }

    /// 创建无穷大值的主键列
    pub fn inf_max(name: &str) -> Self {
        Self {
            name: name.to_string(),
            value: PrimaryKeyValue::InfMax,
        }
    }

    /// 创建自增主键列，无需填充值
    pub fn auto_increment(name: &str) -> Self {
        Self {
            name: name.to_string(),
            value: PrimaryKeyValue::AutoIncrement,
        }
    }

    /// Read the cursor from TAG_CELL_NAME. that means HEADER, TAG_ROW_PK has been read
    pub(crate) fn read_plain_buffer(cursor: &mut Cursor<Vec<u8>>) -> OtsResult<Self> {
        let mut name = String::new();
        let mut value = PrimaryKeyValue::Integer(0);
        let mut checksum = 0u8;

        loop {
            if cursor.position() >= (cursor.get_ref().len() - 1) as u64 {
                break;
            }

            let tag = cursor.read_u8()?;

            match tag {
                plain_buffer::TAG_CELL_NAME => {
                    let len = cursor.read_u32::<LittleEndian>()? as usize;
                    let mut buf: Vec<u8> = vec![0u8; len];
                    cursor.read_exact(&mut buf)?;
                    name = String::from_utf8(buf)?;
                }

                plain_buffer::TAG_CELL_VALUE => {
                    let _prefix = cursor.read_u32::<LittleEndian>()?;
                    let cell_value_type = cursor.read_u8()?;

                    value = match cell_value_type {
                        plain_buffer::VT_INTEGER => PrimaryKeyValue::Integer(cursor.read_i64::<LittleEndian>()?),

                        plain_buffer::VT_STRING => {
                            let len = cursor.read_u32::<LittleEndian>()? as usize;
                            let mut buf: Vec<u8> = vec![0u8; len];

                            cursor.read_exact(&mut buf)?;
                            PrimaryKeyValue::String(String::from_utf8(buf)?)
                        }

                        plain_buffer::VT_BLOB => {
                            let len = cursor.read_u32::<LittleEndian>()? as usize;
                            let mut buf: Vec<u8> = vec![0u8; len];

                            cursor.read_exact(&mut buf)?;
                            PrimaryKeyValue::Binary(buf)
                        }

                        _ => return Err(OtsError::PlainBufferError(format!("unknown primary key cell value type: {}", cell_value_type))),
                    };
                }

                plain_buffer::TAG_CELL_CHECKSUM => {
                    checksum = cursor.read_u8()?;
                    break;
                }

                _ => return Err(OtsError::PlainBufferError(format!("unknown tag: {}", tag))),
            }
        }

        let pk_col = Self { name, value };

        let cell_checksum = pk_col.crc8_checksum();

        if cell_checksum != checksum {
            return Err(OtsError::PlainBufferError(format!(
                "primary key cell checksum validation failed. calculated: {}, received: {}",
                cell_checksum, checksum
            )));
        }

        Ok(pk_col)
    }

    /// 主键列的校验码，列名和列值都计算在内的
    pub(crate) fn crc8_checksum(&self) -> u8 {
        let mut cell_checksum = 0u8;
        cell_checksum = crc_bytes(cell_checksum, self.name.as_bytes());
        self.value.crc8_checksum(cell_checksum)
    }

    /// 返回的长度： 1 字节 TAG_CELL + 1 字节 TAG_CELL_NAME + 4 字节名称长度 + 名称数据 + 1 字节 TAG_CELL_VALUE + 值的长度 + 2 字节校验码
    ///
    /// 0x03                - TAG_CELL
    /// 0x04                - TAG_CELL_NAME
    /// 0x00 0x00 0x00 0x00 - Cell name length u32, le 4 bytes
    /// 0x00 ...            - Cell name utf8 bytes
    /// 0x05                - TAG_CELL_VALUE
    /// 0x00 ...            - Cell value
    /// 0x0A                - TAG_CELL_CHECKSUM
    /// 0x00                - Cell checksum value
    pub(crate) fn compute_size(&self) -> u32 {
        2u32 + plain_buffer::LITTLE_ENDIAN_32_SIZE + self.name.len() as u32 + 1 + self.value.compute_size() + 2
    }

    /// 返回值是 Cell 的校验码
    pub(crate) fn write_plain_buffer(&self, cursor: &mut Cursor<Vec<u8>>) {
        let Self { name, value } = self;

        cursor.write_u8(TAG_CELL).unwrap();
        cursor.write_u8(TAG_CELL_NAME).unwrap();
        cursor.write_u32::<LittleEndian>(name.len() as u32).unwrap();
        cursor.write_all(name.as_bytes()).unwrap();
        cursor.write_u8(TAG_CELL_VALUE).unwrap();
        value.write_plain_buffer(cursor);
        cursor.write_u8(TAG_CELL_CHECKSUM).unwrap();
        cursor.write_u8(self.crc8_checksum()).unwrap();
    }
}

#[cfg(test)]
mod test_primary_key {
    use crate::{
        model::{PrimaryKey, PrimaryKeyColumn, PrimaryKeyValue},
        protos::plain_buffer::{MASK_HEADER, MASK_ROW_CHECKSUM},
    };

    #[test]
    fn test_build_primary_key() {
        let bytes_from_java_sdk = [
            0x75u8, 0x00, 0x00, 0x00, 0x01, 0x03, 0x04, 0x07, 0x00, 0x00, 0x00, 0x75, 0x73, 0x65, 0x72, 0x5F, 0x69, 0x64, 0x05, 0x29, 0x00, 0x00, 0x00, 0x03,
            0x24, 0x00, 0x00, 0x00, 0x30, 0x30, 0x30, 0x35, 0x33, 0x35, 0x38, 0x41, 0x2D, 0x44, 0x43, 0x41, 0x46, 0x2D, 0x36, 0x36, 0x35, 0x45, 0x2D, 0x45,
            0x45, 0x43, 0x46, 0x2D, 0x44, 0x39, 0x39, 0x33, 0x35, 0x45, 0x38, 0x32, 0x31, 0x42, 0x38, 0x37, 0x0A, 0xC8, 0x09, 0x45,
        ];

        let pk = PrimaryKey {
            columns: vec![PrimaryKeyColumn {
                name: "user_id".to_string(),
                value: PrimaryKeyValue::String("0005358A-DCAF-665E-EECF-D9935E821B87".to_string()),
            }],
        };

        let size = pk.compute_size(MASK_HEADER | MASK_ROW_CHECKSUM);
        assert_eq!(68, size);

        let buf = pk.encode_plain_buffer(MASK_HEADER | MASK_ROW_CHECKSUM);
        assert_eq!(bytes_from_java_sdk, &buf[..]);
        println!("{:?}", buf);
    }
}
