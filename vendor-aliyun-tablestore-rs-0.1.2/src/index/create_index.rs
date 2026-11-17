use prost::Message;

use crate::{
    add_per_request_options,
    error::OtsError,
    model::rules::{validate_index_name, validate_table_name},
    protos::{IndexSyncPhase, IndexType, IndexUpdateMode},
    OtsClient, OtsOp, OtsRequest, OtsRequestOptions, OtsResult,
};

/// 创建二级索引。仅 `max_versions = 1` 的表可以创建二级索引
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/createindex>
#[derive(Debug, Clone, Default)]
pub struct CreateIndexRequest {
    /// 表名
    pub table_name: String,

    /// 索引名称
    pub index_name: String,

    /// 索引表的索引列，索引列为数据表主键和预定义列的组合。
    /// 使用本地二级索引时，索引表的第一个主键列必须与数据表的第一个主键列相同
    pub primary_key_names: Vec<String>,

    /// 索引表的属性列，是数据表的预定义列的组合
    pub defined_column_names: Vec<String>,

    /// 索引更新模式，支持同步更新和异步更新
    pub index_update_mode: IndexUpdateMode,

    /// 索引类型，支持全局二级索引和本地二级索引
    pub index_type: IndexType,

    /// 索引同步的阶段
    pub index_sync_phase: Option<IndexSyncPhase>,

    /// 是否包含在创建索引表前数据表的存量数据
    pub include_base_data: Option<bool>,
}

impl CreateIndexRequest {
    pub fn new(table_name: &str, index_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            index_name: index_name.to_string(),
            ..Default::default()
        }
    }

    /// 添加一个主键列到索引
    pub fn primary_key_name(mut self, pk_name: &str) -> Self {
        self.primary_key_names.push(pk_name.to_string());

        self
    }

    /// 设置索引包含的主键列
    pub fn primary_key_names(mut self, pk_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.primary_key_names = pk_names.into_iter().map(|s| s.into()).collect();

        self
    }

    /// 添加一个预定义列到索引
    pub fn defined_column_name(mut self, col_name: &str) -> Self {
        self.defined_column_names.push(col_name.to_string());

        self
    }

    /// 设置索引包含的预定义列
    pub fn defined_column_names(mut self, col_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.defined_column_names = col_names.into_iter().map(|s| s.into()).collect();

        self
    }

    /// 设置索引更新模式
    pub fn index_update_mode(mut self, index_update_mode: IndexUpdateMode) -> Self {
        self.index_update_mode = index_update_mode;

        self
    }

    /// 设置索引类型
    pub fn index_type(mut self, index_type: IndexType) -> Self {
        self.index_type = index_type;

        self
    }

    /// 设置同步阶段
    pub fn index_sync_phase(mut self, index_sync_phase: IndexSyncPhase) -> Self {
        self.index_sync_phase = Some(index_sync_phase);

        self
    }

    /// 设置是否包含存量数据
    pub fn include_base_data(mut self, include_base_data: bool) -> Self {
        self.include_base_data = Some(include_base_data);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_table_name(&self.table_name) {
            return Err(OtsError::ValidationFailed(format!("invalid table name: {}", self.table_name)));
        }

        if !validate_index_name(&self.index_name) {
            return Err(OtsError::ValidationFailed(format!("invalid index name: {}", self.index_name)));
        }

        if self.primary_key_names.is_empty() {
            return Err(OtsError::ValidationFailed(
                "primary key columns can not be empty while creating index".to_string(),
            ));
        }

        Ok(())
    }
}

impl From<CreateIndexRequest> for crate::protos::CreateIndexRequest {
    fn from(value: CreateIndexRequest) -> Self {
        let CreateIndexRequest {
            table_name,
            index_name,
            primary_key_names,
            defined_column_names,
            index_update_mode,
            index_type,
            index_sync_phase,
            include_base_data,
        } = value;

        Self {
            main_table_name: table_name,
            index_meta: crate::protos::IndexMeta {
                name: index_name,
                primary_key: primary_key_names,
                defined_column: defined_column_names,
                index_update_mode: index_update_mode as i32,
                index_type: index_type as i32,
                index_sync_phase: index_sync_phase.map(|t| t as i32),
            },
            include_base_data,
        }
    }
}

#[derive(Clone)]
pub struct CreateIndexOperation {
    client: OtsClient,
    request: CreateIndexRequest,
    options: OtsRequestOptions,
}

add_per_request_options!(CreateIndexOperation);

impl CreateIndexOperation {
    pub(crate) fn new(client: OtsClient, request: CreateIndexRequest) -> Self {
        Self {
            client,
            request,
            options: OtsRequestOptions::default(),
        }
    }

    pub async fn send(self) -> OtsResult<()> {
        self.request.validate()?;

        let Self { client, request, options } = self;

        let msg = crate::protos::CreateIndexRequest::from(request);

        let req = OtsRequest {
            operation: OtsOp::CreateIndex,
            body: msg.encode_to_vec(),
            options,
            ..Default::default()
        };

        let resp = client.send(req).await?;
        resp.bytes().await?;

        Ok(())
    }
}
