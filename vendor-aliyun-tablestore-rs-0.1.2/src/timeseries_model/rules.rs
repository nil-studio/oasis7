/// 自定义时间线标识字段个数
pub const MAX_TIMESERIES_KEY_COUNT: usize = 6;

/// 可作为主键的数据字段个数
pub const MAX_FIELD_PRIMARY_KEY_COUNT: usize = 4;

/// 单行写入的属性列个数不能超过 1024 列
pub const MAX_FIELD_COUNT: usize = 1024;

/// 一次写入的行数不能超过 200 行
pub const MAX_ROW_COUNT: usize = 200;

/// 一次写入数据大小上限为 `4 MB`
pub const MAX_DATA_SIZE: usize = 1024 * 1024 * 4;

/// 分析存储的默认名称
pub const DEFAULT_ANALYTICAL_NAME: &str = "default_analytical_store";

/// 数据 TTL 最小值
pub const MIN_DATA_TTL_SECONDS: i32 = 86400;

/// 元数据 TTL 最小值
pub const MIN_META_TTL_SECONDS: i32 = 604800;

/// 分析存储 TTL 最小值
pub const MIN_ANALYTICAL_STORE_TTL_SECONDS: i32 = 2592000;

/// 验证时序表名称
///
/// - 由英文字母、数字或下划线组成
/// - 大小写敏感
/// - 长度限制为1~128个字符
/// - 首字母必须为英文字母或下划线
pub fn validate_timeseries_table_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    if name.len() > 128 {
        return false;
    }

    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return false;
    }

    for c in name.chars() {
        if !c.is_ascii_alphanumeric() && c != '_' {
            return false;
        }
    }

    true
}

/// 验证时序表 lastpoint 索引名称
pub fn validate_lastpoint_index_name(name: &str) -> bool {
    validate_timeseries_table_name(name)
}

/// 验证时序表分析存储名称
pub fn validate_analytical_store_name(name: &str) -> bool {
    validate_timeseries_table_name(name)
}

/// 验证度量名称
///
/// - UTF-8 字符串
/// - 长度限制为 1~128 字节
/// - 不能出现不可见字符（包括空格）和 `#`
pub fn validate_timeseries_measurement(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    if name.len() > 128 {
        return false;
    }

    for c in name.chars() {
        if c.is_whitespace() || c == '#' {
            return false;
        }
    }

    true
}

/// 验证数据源名称。 UTF-8 字符串，长度限制为 0 - 256 字节
pub fn validate_timeseries_datasource(s: &str) -> bool {
    s.len() <= 256
}

/// 验证时序列名称
///
/// - 由小写字母、数字和下划线组成
/// - 大小写敏感
/// - 长度限制为1~128个字符
/// - 首字母不能为数字
/// - 不能包含特定的保留字段名
pub fn validate_timeseries_field_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    if name.len() > 128 {
        return false;
    }

    let first_char = name.chars().next().unwrap();
    if first_char.is_ascii_digit() {
        return false;
    }

    for c in name.chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' {
            return false;
        }
    }

    // Check for reserved field names
    let reserved = ["_m_name", "_data_source", "_tags", "_time", "_meta_update_time", "_attributes"];
    for field in reserved {
        if name.contains(field) {
            return false;
        }
    }

    true
}

/// 验证标签名称
///
/// - 可见 ASCII 字符
/// - 长度不能超过128个字符
/// - 不能包含双引号和等号
pub fn validate_timeseries_tag_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    if name.len() > 128 {
        return false;
    }

    for c in name.chars() {
        if !('!'..='~').contains(&c) {
            return false;
        }
    }

    true
}

/// 验证标签值
///
/// - 支持 UTF-8 编码的字符串
/// - 长度不能超过256个字符
/// - 不能包含双引号和等号
pub fn validate_timeseries_tag_value(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }

    if value.len() > 256 {
        return false;
    }

    if value.contains('"') || value.contains('=') {
        return false;
    }

    true
}
