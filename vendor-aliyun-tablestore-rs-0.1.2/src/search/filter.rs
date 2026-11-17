use super::Query;

#[derive(Debug, Clone)]
pub struct NestedFilter {
    /// 字段路径。
    pub path: String,

    /// 查询条件。
    pub filter: Query,
}

impl NestedFilter {
    pub fn new(path: impl Into<String>, filter: Query) -> Self {
        Self { path: path.into(), filter }
    }
}

impl From<NestedFilter> for crate::protos::search::NestedFilter {
    fn from(value: NestedFilter) -> Self {
        let NestedFilter { path, filter } = value;

        crate::protos::search::NestedFilter {
            path: Some(path),
            filter: Some(crate::protos::search::Query::from(filter)),
        }
    }
}
