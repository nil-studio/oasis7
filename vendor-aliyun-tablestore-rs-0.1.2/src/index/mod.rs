//! 二级索引
//!

use crate::protos::{IndexMeta, IndexSyncPhase, IndexType, IndexUpdateMode};

mod create_index;
mod drop_index;

pub use create_index::*;
pub use drop_index::*;

/// Builder for [`IndexMeta`]
#[derive(Debug, Clone, Default)]
pub struct IndexMetaBuilder {
    idx_meta: IndexMeta,
}

impl IndexMetaBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            idx_meta: IndexMeta {
                name: name.to_string(),
                ..Default::default()
            },
        }
    }

    /// 设置索引的名称
    pub fn name(mut self, name: &str) -> Self {
        self.idx_meta.name = name.into();

        self
    }

    /// 添加一个主键列的名字
    pub fn primary_key(mut self, pk_name: &str) -> Self {
        self.idx_meta.primary_key.push(pk_name.into());

        self
    }

    /// 设置索引中包含的主键列名字
    pub fn primary_keys(mut self, pk_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.idx_meta.primary_key = pk_names.into_iter().map(|s| s.into()).collect();

        self
    }

    /// 添加一个预定义列的名字
    pub fn defined_column(mut self, col_name: &str) -> Self {
        self.idx_meta.defined_column.push(col_name.into());

        self
    }

    /// 设置预定义列的名字
    pub fn defined_columns(mut self, col_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.idx_meta.defined_column = col_names.into_iter().map(|s| s.into()).collect();

        self
    }

    pub fn index_update_mode(mut self, mode: IndexUpdateMode) -> Self {
        self.idx_meta.index_update_mode = mode as i32;
        self
    }

    pub fn index_type(mut self, idx_type: IndexType) -> Self {
        self.idx_meta.index_type = idx_type as i32;
        self
    }

    pub fn index_sync_phase(mut self, phase: IndexSyncPhase) -> Self {
        self.idx_meta.index_sync_phase = Some(phase as i32);
        self
    }

    pub fn build(self) -> IndexMeta {
        self.idx_meta
    }
}

/// Add `builder` method to [`IndexMeta`]
impl IndexMeta {
    /// `name` 是索引的名称
    pub fn builder(name: &str) -> IndexMetaBuilder {
        IndexMetaBuilder::new(name)
    }
}

#[cfg(test)]
mod test_index {
    use crate::{index::CreateIndexRequest, test_util::setup, OtsClient};

    async fn test_create_index_impl() {
        setup();
        let client = OtsClient::from_env();

        let resp = client
            .create_index(CreateIndexRequest::new("ccs2", "index_cn").primary_key_name("cc_id"))
            .send()
            .await;

        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn test_create_index() {
        test_create_index_impl().await;
    }

    async fn test_drop_index_impl() {
        setup();
        let client = OtsClient::from_env();

        let resp = client.drop_index("ccs2", "idx_cn").send().await;

        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn test_drop_index() {
        test_drop_index_impl().await;
    }
}
