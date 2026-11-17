/// 一个宽表至少有 1 个主键列
pub const MIN_PRIMARY_KEY_COUNT: usize = 1;

/// 一个宽表最多 4 个主键列
pub const MAX_PRIMARY_KEY_COUNT: usize = 4;

/// 读请求一行数据中获取的列的最大个数
pub const MAX_COLUMNS_TO_GET: usize = 128;

/// 约束条件：
///
/// - 由英文字母、数字或下划线（_）组成，大小写敏感，长度限制为1~255字节。
/// - 首字母必须为英文字母或下划线（_）。
pub fn validate_table_name(table_name: &str) -> bool {
    if table_name.is_empty() || table_name.len() > 255 {
        return false;
    }

    let first_char = match table_name.chars().next() {
        Some(c) => c,
        None => return false,
    };

    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return false;
    }

    table_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 和表名的约束条件一样
pub fn validate_column_name(col_name: &str) -> bool {
    validate_table_name(col_name)
}

pub fn validate_index_name(idx_name: &str) -> bool {
    validate_table_name(idx_name)
}
