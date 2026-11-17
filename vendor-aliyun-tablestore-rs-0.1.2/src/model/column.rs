use std::io::{Cursor, Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    crc8::{crc_bytes, crc_f64, crc_i64, crc_u32, crc_u64, crc_u8},
    error::OtsError,
    protos::plain_buffer::{
        self, CELL_OP_DELETE_ALL_VERSION, CELL_OP_DELETE_ONE_VERSION, CELL_OP_INCREMENT, LITTLE_ENDIAN_32_SIZE, LITTLE_ENDIAN_64_SIZE, TAG_CELL,
        TAG_CELL_CHECKSUM, TAG_CELL_NAME, TAG_CELL_OP, TAG_CELL_TIMESTAMP, TAG_CELL_VALUE, VT_BLOB, VT_BOOLEAN, VT_DOUBLE, VT_INF_MAX, VT_INF_MIN, VT_INTEGER,
        VT_STRING,
    },
    OtsResult,
};

/// 列操作
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ColumnOp {
    /// 此时该 value 必须为空，需要指定 timestamp。表示删除该列特定版本的数据。
    Delete,

    /// 此时该 value 和 timestamp 都必须为空。表示删除该列所有版本的数据。
    DeleteAll,

    /// 某一列的值进行递增操作
    Increment,
}

impl ColumnOp {
    pub fn as_u8(&self) -> u8 {
        match self {
            ColumnOp::Delete => CELL_OP_DELETE_ONE_VERSION,
            ColumnOp::DeleteAll => CELL_OP_DELETE_ALL_VERSION,
            ColumnOp::Increment => CELL_OP_INCREMENT,
        }
    }
}

impl From<ColumnOp> for u8 {
    fn from(value: ColumnOp) -> Self {
        value.as_u8()
    }
}

impl From<&ColumnOp> for u8 {
    fn from(value: &ColumnOp) -> Self {
        value.as_u8()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum ColumnValue {
    /// 这个值是内部使用的，仅仅是用来标记在 UpdateRow 的时候要标记删除某个列的所有版本使用的。
    #[default]
    Null,

    Integer(i64),
    Double(f64),
    Boolean(bool),
    String(String),
    Blob(Vec<u8>),
    InfMin,
    InfMax,
}

impl PartialOrd for ColumnValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            // Null is considered less than all other values
            (Self::Null, Self::Null) => Some(std::cmp::Ordering::Equal),
            (Self::Null, _) => Some(std::cmp::Ordering::Less),
            (_, Self::Null) => Some(std::cmp::Ordering::Greater),

            // InfMin is less than all other values except Null
            (Self::InfMin, Self::InfMin) => Some(std::cmp::Ordering::Equal),
            (Self::InfMin, _) => Some(std::cmp::Ordering::Less),
            (_, Self::InfMin) => Some(std::cmp::Ordering::Greater),

            // InfMax is greater than all other values except Null
            (Self::InfMax, Self::InfMax) => Some(std::cmp::Ordering::Equal),
            (Self::InfMax, _) => Some(std::cmp::Ordering::Greater),
            (_, Self::InfMax) => Some(std::cmp::Ordering::Less),

            // Compare same types
            (Self::Integer(a), Self::Integer(b)) => a.partial_cmp(b),
            (Self::Double(a), Self::Double(b)) => a.partial_cmp(b),
            (Self::Boolean(a), Self::Boolean(b)) => a.partial_cmp(b),
            (Self::String(a), Self::String(b)) => a.partial_cmp(b),
            (Self::Blob(a), Self::Blob(b)) => a.partial_cmp(b),

            // Compare different types
            (_, _) => None,
        }
    }
}

impl ColumnValue {
    /// 返回的长度包含：4 字节前缀 + 1 字节类型 + 4 字节值的长度（仅针对 String 和 Binary）+ 值的实际数据长度
    pub(crate) fn compute_size(&self) -> u32 {
        // 4 bytes for total length,
        // 1 byte for cell value type
        let size = LITTLE_ENDIAN_32_SIZE + 1;

        match self {
            // 8 bytes for i64
            Self::Integer(_) => size + LITTLE_ENDIAN_64_SIZE,

            // 4 bytes for string length, and n bytes for string bytes
            Self::String(s) => size + LITTLE_ENDIAN_32_SIZE + s.len() as u32,

            // 8 bytes for double value
            Self::Double(_) => size + LITTLE_ENDIAN_64_SIZE,

            // 1 byte for boolean value
            Self::Boolean(_) => size + 1,

            // 4 bytes for buf length, and n bytes for buf bytes
            Self::Blob(buf) => size + LITTLE_ENDIAN_32_SIZE + buf.len() as u32,

            // cell value type has been set at the beginning
            Self::InfMin | Self::InfMax => size,

            // 空值的不写 Value 部分（用来在 UpdateRow 的时候删除一个列）
            Self::Null => 0,
        }
    }

    /// 仅仅解码列值部分
    pub(crate) fn decode_plain_buffer(bytes: impl Into<Vec<u8>>) -> OtsResult<Self> {
        let mut cursor = Cursor::new(bytes.into());
        let value_type = cursor.read_u8()?;

        match value_type {
            VT_INF_MIN => Ok(Self::InfMin),

            VT_INF_MAX => Ok(Self::InfMax),

            VT_INTEGER => {
                let n = cursor.read_i64::<LittleEndian>()?;
                Ok(Self::Integer(n))
            }

            VT_DOUBLE => {
                let n = cursor.read_f64::<LittleEndian>()?;
                Ok(Self::Double(n))
            }

            VT_BOOLEAN => {
                let n = cursor.read_u8()?;
                Ok(Self::Boolean(n != 0))
            }

            VT_STRING => {
                let len = cursor.read_u32::<LittleEndian>()?;
                let mut buf = vec![0u8; len as usize];
                cursor.read_exact(&mut buf)?;
                Ok(Self::String(String::from_utf8(buf)?))
            }

            VT_BLOB => {
                let len = cursor.read_u32::<LittleEndian>()?;
                let mut buf = vec![0u8; len as usize];
                cursor.read_exact(&mut buf)?;
                Ok(Self::Blob(buf))
            }

            _ => Err(OtsError::ValidationFailed(format!("invalid column value type: {}", value_type))),
        }
    }

    /// 仅编码列值部分。返回的数据不包含列值前缀的 4 字节。
    /// 这个方法仅适用于只写出列值的场景
    pub(crate) fn encode_plain_buffer(&self) -> Vec<u8> {
        let size = self.compute_size();
        let buf = vec![0u8; size as usize];
        let mut cursor = Cursor::new(buf);
        self.write_plain_buffer(&mut cursor);
        // 去掉前缀的 4 字节，也就是从类型字节开始
        cursor.into_inner()[4..].into()
    }

    /// Consume self values and write to cursor *WITHOUT* TAG_CELL_VALUE byte.
    pub(crate) fn write_plain_buffer(&self, cursor: &mut Cursor<Vec<u8>>) {
        // 空值不写出
        if let Self::Null = self {
            return;
        }

        // 实际写入的前缀，要减去前缀所占用的 4 个字节
        let size = self.compute_size() - LITTLE_ENDIAN_32_SIZE;
        cursor.write_u32::<LittleEndian>(size).unwrap();

        match self {
            Self::Null => {}
            Self::InfMin => cursor.write_u8(VT_INF_MIN).unwrap(),
            Self::InfMax => cursor.write_u8(VT_INF_MAX).unwrap(),

            Self::Integer(n) => {
                cursor.write_u8(VT_INTEGER).unwrap();
                cursor.write_i64::<LittleEndian>(*n).unwrap();
            }
            Self::Double(d) => {
                cursor.write_u8(VT_DOUBLE).unwrap();
                cursor.write_f64::<LittleEndian>(*d).unwrap();
            }
            Self::Boolean(b) => {
                cursor.write_u8(VT_BOOLEAN).unwrap();
                cursor.write_u8(if *b { 1u8 } else { 0u8 }).unwrap();
            }
            Self::String(s) => {
                cursor.write_u8(VT_STRING).unwrap();
                cursor.write_u32::<LittleEndian>(s.len() as u32).unwrap();
                cursor.write_all(s.as_bytes()).unwrap();
            }
            Self::Blob(bytes) => {
                cursor.write_u8(VT_BLOB).unwrap();
                cursor.write_u32::<LittleEndian>(bytes.len() as u32).unwrap();
                cursor.write_all(bytes).unwrap();
            }
        }
    }

    /// Calculate the cell checksum
    pub(crate) fn crc8_checksum(&self, input_checksum: u8) -> u8 {
        let mut checksum = input_checksum;

        match self {
            Self::Null => checksum,
            Self::InfMin => crc_u8(checksum, VT_INF_MIN),
            Self::InfMax => crc_u8(checksum, VT_INF_MAX),

            Self::Integer(n) => {
                checksum = crc_u8(checksum, VT_INTEGER);
                crc_i64(checksum, *n)
            }

            Self::Double(d) => {
                checksum = crc_u8(checksum, VT_DOUBLE);
                crc_f64(checksum, *d)
            }

            Self::Boolean(b) => {
                checksum = crc_u8(checksum, VT_BOOLEAN);
                crc_u8(checksum, if *b { 1u8 } else { 0u8 })
            }

            Self::String(s) => {
                checksum = crc_u8(checksum, VT_STRING);
                checksum = crc_u32(checksum, s.len() as u32);
                crc_bytes(checksum, s.as_bytes())
            }

            Self::Blob(buf) => {
                checksum = crc_u8(checksum, VT_BLOB);
                checksum = crc_u32(checksum, buf.len() as u32);
                crc_bytes(checksum, buf)
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Column {
    pub name: String,
    pub value: ColumnValue,
    pub op: Option<ColumnOp>,
    pub timestamp: Option<u64>,
}

impl Column {
    /// 返回的长度：
    ///
    /// - 1 字节 TAG_CELL
    /// - 1 字节 TAG_CELL_NAME
    /// - 4 字节名称长度
    /// - 名称数据
    /// - 1 字节 TAG_CELL_VALUE
    /// - 值的 plain buffer 长度
    /// - 可选：操作 TAG_CELL_OP + 值共 2 字节
    /// - 可选：时间戳 TAG_CELL_TIMESTAMP + 值共 9 字节
    /// -  2 字节校验码
    pub(crate) fn compute_size(&self) -> u32 {
        let mut size = 1 + 1 + LITTLE_ENDIAN_32_SIZE + (self.name.len() as u32) + 1 + self.value.compute_size() + 2;

        // Null 值不写出，所以要减去 TAG_CELL_VALUE 的 1 字节
        if ColumnValue::Null != self.value {
            size -= 1;
        }

        if self.op.is_some() {
            size += 2;
        }

        if self.timestamp.is_some() {
            size += 9;
        }
        size
    }

    /// 消费掉自己的数据，写出 plain buffer。
    /// 返回 Cell 的校验码
    pub(crate) fn write_plain_buffer(&self, cursor: &mut Cursor<Vec<u8>>) {
        let Self { name, value, op, timestamp } = self;

        cursor.write_u8(TAG_CELL).unwrap();
        cursor.write_u8(TAG_CELL_NAME).unwrap();
        cursor.write_u32::<LittleEndian>(name.len() as u32).unwrap();
        cursor.write_all(name.as_bytes()).unwrap();

        if &ColumnValue::Null != value {
            cursor.write_u8(TAG_CELL_VALUE).unwrap();
            value.write_plain_buffer(cursor);
        }

        if let Some(cell_op) = op {
            cursor.write_u8(TAG_CELL_OP).unwrap();
            cursor.write_u8(cell_op.into()).unwrap();
        }

        if let Some(ts) = timestamp {
            cursor.write_u8(TAG_CELL_TIMESTAMP).unwrap();
            cursor.write_u64::<LittleEndian>(*ts).unwrap();
        }

        cursor.write_u8(TAG_CELL_CHECKSUM).unwrap();
        cursor.write_u8(self.crc8_checksum()).unwrap();
    }

    pub(crate) fn read_plain_buffer(cursor: &mut Cursor<Vec<u8>>) -> OtsResult<Self> {
        let mut name = String::new();
        let mut value = ColumnValue::Integer(0);
        let mut checksum = 0u8;
        let mut ts: Option<u64> = None;

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
                    let _previx = cursor.read_u32::<LittleEndian>()?;
                    let cell_value_type = cursor.read_u8()?;

                    value = match cell_value_type {
                        plain_buffer::VT_INTEGER => ColumnValue::Integer(cursor.read_i64::<LittleEndian>()?),

                        plain_buffer::VT_DOUBLE => ColumnValue::Double(cursor.read_f64::<LittleEndian>()?),

                        plain_buffer::VT_BOOLEAN => {
                            let b = cursor.read_u8()?;
                            ColumnValue::Boolean(b == 0x01)
                        }

                        plain_buffer::VT_STRING => {
                            let len = cursor.read_u32::<LittleEndian>()? as usize;
                            let mut buf: Vec<u8> = vec![0u8; len];
                            cursor.read_exact(&mut buf)?;
                            ColumnValue::String(String::from_utf8(buf)?)
                        }

                        plain_buffer::VT_BLOB => {
                            let len = cursor.read_u32::<LittleEndian>()? as usize;
                            let mut buf: Vec<u8> = vec![0u8; len];
                            cursor.read_exact(&mut buf)?;
                            ColumnValue::Blob(buf)
                        }

                        _ => return Err(OtsError::PlainBufferError(format!("unknown data data cell value type: {}", cell_value_type))),
                    };
                }

                plain_buffer::TAG_CELL_TIMESTAMP => {
                    ts = Some(cursor.read_u64::<LittleEndian>()?);
                }

                plain_buffer::TAG_CELL_CHECKSUM => {
                    checksum = cursor.read_u8()?;
                    break;
                }

                _ => return Err(OtsError::PlainBufferError(format!("unknown tag: {}", tag))),
            }
        }

        let col = Self {
            name,
            value,
            timestamp: ts,
            ..Default::default()
        };

        let cell_checksum = col.crc8_checksum();

        // log::debug!("cell {}, calculated checksum {}, received checksum {}", col.name, cell_checksum, checksum);

        if cell_checksum != checksum {
            return Err(OtsError::PlainBufferError(format!(
                "data data cell checksum validation failed. calculated: {}, received: {}",
                cell_checksum, checksum
            )));
        }

        Ok(col)
    }

    /// 一个列，包含名、值、删除标记、时间戳的校验码
    pub(crate) fn crc8_checksum(&self) -> u8 {
        let mut cell_checksum = 0u8;
        cell_checksum = crc_bytes(cell_checksum, self.name.as_bytes());
        cell_checksum = self.value.crc8_checksum(cell_checksum);
        if let Some(d) = &self.timestamp {
            cell_checksum = crc_u64(cell_checksum, *d);
        }

        // NOTE：这里特别注意下计算crc的顺序， cell_op/cell_type 在 cell_timestamp 之后，虽然数据是在前面 - 摘自 PHP 版本 SDK.
        if let Some(op) = &self.op {
            cell_checksum = crc_u8(cell_checksum, op.as_u8());
        }

        cell_checksum
    }

    /// 构造整数列
    pub fn from_integer(name: &str, value: i64) -> Self {
        Self {
            name: name.to_string(),
            value: ColumnValue::Integer(value),
            ..Default::default()
        }
    }

    /// 构造双精度列
    pub fn from_double(name: &str, value: f64) -> Self {
        Self {
            name: name.to_string(),
            value: ColumnValue::Double(value),
            ..Default::default()
        }
    }

    /// 构造布尔值列
    pub fn from_bool(name: &str, value: bool) -> Self {
        Self {
            name: name.to_string(),
            value: ColumnValue::Boolean(value),
            ..Default::default()
        }
    }

    /// 构造字符串列
    pub fn from_string(name: &str, value: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            value: ColumnValue::String(value.into()),
            ..Default::default()
        }
    }

    /// 构造二进制列
    pub fn from_blob(name: &str, value: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.to_string(),
            value: ColumnValue::Blob(value.into()),
            ..Default::default()
        }
    }

    /// 构造空值列
    pub fn null(name: &str) -> Self {
        Self {
            name: name.to_string(),
            value: ColumnValue::Null,
            ..Default::default()
        }
    }

    /// 构造极小值列
    pub fn inf_min(name: &str) -> Self {
        Self {
            name: name.to_string(),
            value: ColumnValue::InfMin,
            ..Default::default()
        }
    }

    /// 构造极大值列
    pub fn inf_max(name: &str) -> Self {
        Self {
            name: name.to_string(),
            value: ColumnValue::InfMax,
            ..Default::default()
        }
    }
}
