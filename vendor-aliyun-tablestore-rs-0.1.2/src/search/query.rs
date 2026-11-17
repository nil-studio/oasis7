use prost::Message;

use super::{Aggregation, GeoPoint, GroupBy, ScoreFunction, Sort, Sorter};
use crate::model::rules::validate_column_name;
use crate::{
    error::OtsError,
    model::ColumnValue,
    protos::search::{
        Collapse, FunctionCombineMode, FunctionScoreMode, HighlightEncoder, HighlightFragmentOrder, QueryOperator, QueryType, ScoreMode, SearchFilter,
    },
    OtsResult,
};

/// 包括模糊匹配和短语或邻近查询
#[derive(Debug, Default, Clone)]
pub struct MatchQuery {
    /// 要匹配的字段
    pub field_name: String,

    /// 模糊匹配的值
    ///
    /// 当要匹配的列为 `Text` 类型时，查询关键词会被分词成多个词，分词类型为创建多元索引时设置的分词器类型。如果创建多元索引时未设置分词器类型，则默认分词类型为单字分词。
    /// 例如当要匹配的列为 `Text` 类型时，分词类型为单字分词。例如查询词为 `"this is"`，可以匹配到:
    ///
    /// - `"..., this is tablestore"`
    /// - `"is this tablestore"`
    /// - `"tablestore is cool"`
    /// - `"this"`
    /// - `"is"`
    /// - 等
    pub text: String,

    /// 最小匹配个数，必须与逻辑运算符 `OR` 配合使用。
    /// 只有当某一行数据的 `field_name` 列的值中至少包括最小匹配个数的词时，才会返回该行数据。
    pub minimum_should_match: Option<u32>,

    /// 查询操作符。取值范围为逻辑运算符 `AND` 和 `OR`。
    /// 默认值为 `OR`，表示当分词后的多个词只要有部分匹配时，则行数据满足查询条件。
    pub operator: Option<QueryOperator>,

    /// 查询条件的权重配置。
    pub weight: Option<f32>,
}

impl MatchQuery {
    pub fn new(field_name: &str, text: impl Into<String>) -> Self {
        Self {
            field_name: field_name.to_string(),
            text: text.into(),
            ..Default::default()
        }
    }

    /// 设置查询列名
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置查询值
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();

        self
    }

    /// 设置最小匹配个数
    pub fn minimum_should_match(mut self, min_should_match: u32) -> Self {
        self.minimum_should_match = Some(min_should_match);

        self
    }

    /// 设置查询操作符
    pub fn operator(mut self, operator: QueryOperator) -> Self {
        self.operator = Some(operator);

        self
    }

    /// 设置查询权重
    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<MatchQuery> for crate::protos::search::MatchQuery {
    fn from(value: MatchQuery) -> Self {
        let MatchQuery {
            field_name,
            text,
            minimum_should_match,
            operator,
            weight,
        } = value;

        Self {
            field_name: Some(field_name),
            text: Some(text),
            minimum_should_match: minimum_should_match.map(|v| v as i32),
            operator: operator.map(|o| o as i32),
            weight,
        }
    }
}

/// 表示全匹配查询配置。MatchAllQuery 可以匹配所有行，常用于查询表中数据总行数，或者随机返回几条数据。
#[derive(Debug, Default, Clone)]
pub struct MatchAllQuery {}

impl MatchAllQuery {
    pub fn new() -> Self {
        Self::default()
    }
}

impl From<MatchAllQuery> for crate::protos::search::MatchAllQuery {
    fn from(_: MatchAllQuery) -> Self {
        Self {}
    }
}

/// 表示短语匹配查询配置。短语匹配查询采用近似匹配的方式查询表中的数据，
/// 但是分词后多个词的位置关系会被考虑，
/// 只有分词后的多个词在行数据中以同样的顺序和位置存在时，才表示行数据满足查询条件。
#[derive(Debug, Default, Clone)]
pub struct MatchPhraseQuery {
    /// 要匹配的列
    ///
    /// 短语匹配查询可应用于 `Text 类型。
    field_name: String,

    /// 查询关键词，即要匹配的值
    ///
    /// 当要匹配的列为 `Text` 类型时，
    /// 查询关键词会被分词成多个词，
    /// 分词类型为创建多元索引时设置的分词器类型。
    /// 如果创建多元索引时未设置分词器类型，则默认分词类型为单字分词。
    text: String,

    /// 查询条件的权重配置
    weight: Option<f32>,
}

impl MatchPhraseQuery {
    pub fn new(field_name: &str, text: impl Into<String>) -> Self {
        Self {
            field_name: field_name.to_string(),
            text: text.into(),
            ..Default::default()
        }
    }

    /// 设置查询列名
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置查询值
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();

        self
    }

    /// 设置查询权重
    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<MatchPhraseQuery> for crate::protos::search::MatchPhraseQuery {
    fn from(value: MatchPhraseQuery) -> Self {
        let MatchPhraseQuery { field_name, text, weight } = value;

        Self {
            field_name: Some(field_name),
            text: Some(text),
            weight,
        }
    }
}

/// 查询条件包含一个或者多个子查询条件，根据子查询条件来判断一行数据是否满足查询条件。
/// 每个子查询条件可以是任意一种 Query 类型，包括 `BoolQuery`。
#[derive(Debug, Default, Clone)]
pub struct BoolQuery {
    /// 多个 Query 列表，行数据必须满足所有的子查询条件才算匹配，等价于 And 操作符。
    pub must_queries: Vec<Query>,

    /// 多个 Query 列表，行数据必须不能满足任何的子查询条件才算匹配，等价于 Not 操作符。
    pub must_not_queries: Vec<Query>,

    /// 多个 Query 列表，行数据必须满足所有的子 filter 才算匹配，filter 类似于 query，区别是 filter 不会根据满足的 `filter_queries` 个数进行相关性算分。
    pub filter_queries: Vec<Query>,

    /// 多个 Query 列表，行数据只要满足一个子查询条件就算匹配，等价于 Or 操作符。
    pub should_queries: Vec<Query>,

    /// `should_queries` 子查询条件的最小匹配个数。当同级没有其他 Query，只有 `should_queries` 时，默认值为 `1`；
    /// 当同级已有其他 Query，例如 `must_queries`，`must_not_queries` 和 `filter_queries` 时，默认值为 `0`。
    pub minimum_should_match: Option<u32>,
}

impl BoolQuery {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加一个 `must_queries` 子查询条件
    pub fn must_query(mut self, q: Query) -> Self {
        self.must_queries.push(q);

        self
    }

    /// 设置 `must_queries` 子查询条件
    pub fn must_queries(mut self, qs: impl IntoIterator<Item = Query>) -> Self {
        self.must_queries = qs.into_iter().collect();

        self
    }

    /// 添加一个 `must_not_queries` 子查询条件
    pub fn must_not_query(mut self, q: Query) -> Self {
        self.must_not_queries.push(q);

        self
    }

    /// 设置 `must_not_queries` 子查询条件
    pub fn must_not_queries(mut self, qs: impl IntoIterator<Item = Query>) -> Self {
        self.must_not_queries = qs.into_iter().collect();

        self
    }

    /// 添加一个 `filter_queries` 子查询条件
    pub fn filter_query(mut self, q: Query) -> Self {
        self.filter_queries.push(q);

        self
    }

    /// 设置 `filter_queries` 子查询条件
    pub fn filter_queries(mut self, qs: impl IntoIterator<Item = Query>) -> Self {
        self.filter_queries = qs.into_iter().collect();

        self
    }

    /// 添加一个 `should_queries` 子查询条件
    pub fn should_query(mut self, q: Query) -> Self {
        self.should_queries.push(q);

        self
    }

    /// 设置 `should_queries` 子查询条件
    pub fn should_queries(mut self, qs: impl IntoIterator<Item = Query>) -> Self {
        self.should_queries = qs.into_iter().collect();

        self
    }

    /// 设置子查询最小匹配
    pub fn minimum_should_match(mut self, n: u32) -> Self {
        self.minimum_should_match = Some(n);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if self.must_queries.is_empty() && self.must_not_queries.is_empty() && self.filter_queries.is_empty() && self.should_queries.is_empty() {
            return Err(OtsError::ValidationFailed("bool query must have at least one query".to_string()));
        }

        if let Some(n) = self.minimum_should_match {
            if n > self.should_queries.len() as u32 {
                return Err(OtsError::ValidationFailed(format!("minimum_should_match is too large {}", n)));
            }
        }

        Ok(())
    }
}

impl From<BoolQuery> for crate::protos::search::BoolQuery {
    fn from(value: BoolQuery) -> Self {
        let BoolQuery {
            must_queries,
            must_not_queries,
            filter_queries,
            should_queries,
            minimum_should_match,
        } = value;

        Self {
            must_queries: must_queries.into_iter().map(crate::protos::search::Query::from).collect(),
            must_not_queries: must_not_queries.into_iter().map(crate::protos::search::Query::from).collect(),
            filter_queries: filter_queries.into_iter().map(crate::protos::search::Query::from).collect(),
            should_queries: should_queries.into_iter().map(crate::protos::search::Query::from).collect(),
            minimum_should_match: minimum_should_match.map(|n| n as i32),
        }
    }
}

/// 当我们不关心检索词频率 TF（Term Frequency）对搜索结果排序的影响时，
/// 可以使用 `constant_score` 将查询语句 `query` 或者过滤语句 `filter` 包装起来，
/// 达到提高搜索速度。
///
/// 举例：我们班有 100 个人，有一个字段叫 `"name"`，我们想要获得名字中包含 `"王"`的人，
/// 我们并不关心排序结果，使用 `ConstScoreQuery`（将原来的 Query 放在 `filter` 中）将会大大提高搜索速度。
#[derive(Debug, Clone)]
pub struct ConstScoreQuery {
    pub filter: Query,
}

impl ConstScoreQuery {
    pub fn new(filter: Query) -> Self {
        Self { filter }
    }

    pub fn filter(mut self, filter: Query) -> Self {
        self.filter = filter;

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        self.filter.validate()
    }
}

impl From<ConstScoreQuery> for crate::protos::search::ConstScoreQuery {
    fn from(value: ConstScoreQuery) -> Self {
        Self {
            filter: Some(crate::protos::search::Query::from(value.filter)),
        }
    }
}

/// 用于处理文档分值的 Query
/// 它会在查询结束后对每一个匹配的文档重新打分，并以最终分数排序。
#[derive(Debug, Clone)]
pub struct FunctionsScoreQuery {
    pub query: Query,

    /// 打分函数列表。每个 function 都包含一个打分函数，`weight` 权重以及筛选打分条件的 Filter
    pub functions: Vec<ScoreFunction>,

    /// 各打分函数结合计算方式
    pub score_mode: Option<FunctionScoreMode>,

    /// 打分函数的分数和查询分数的合并模式
    pub combine_mode: Option<FunctionCombineMode>,

    /// 最小打分。低于此分值的文档不会显示
    pub min_score: Option<f32>,

    /// 限制打分函数结合计算后的最大分数，避免打分函数的分过高
    pub max_score: Option<f32>,
}

impl FunctionsScoreQuery {
    pub fn new(query: Query) -> Self {
        Self {
            query,
            functions: vec![],
            score_mode: None,
            combine_mode: None,
            min_score: None,
            max_score: None,
        }
    }

    /// 添加一个打分函数
    pub fn function(mut self, function: ScoreFunction) -> Self {
        self.functions.push(function);

        self
    }

    /// 设置打分函数
    pub fn functions(mut self, functions: impl IntoIterator<Item = ScoreFunction>) -> Self {
        self.functions = functions.into_iter().collect();

        self
    }

    pub fn score_mode(mut self, score_mode: FunctionScoreMode) -> Self {
        self.score_mode = Some(score_mode);

        self
    }

    pub fn combine_mode(mut self, combine_mode: FunctionCombineMode) -> Self {
        self.combine_mode = Some(combine_mode);

        self
    }

    pub fn min_score(mut self, min_score: f32) -> Self {
        self.min_score = Some(min_score);

        self
    }

    pub fn max_score(mut self, max_score: f32) -> Self {
        self.max_score = Some(max_score);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        self.query.validate()?;

        for f in &self.functions {
            f.validate()?;
        }

        Ok(())
    }
}

impl From<FunctionsScoreQuery> for crate::protos::search::FunctionsScoreQuery {
    fn from(value: FunctionsScoreQuery) -> Self {
        Self {
            query: Some(crate::protos::search::Query::from(value.query)),
            functions: value.functions.into_iter().map(crate::protos::search::Function::from).collect(),
            score_mode: value.score_mode.map(|m| m as i32),
            combine_mode: value.combine_mode.map(|m| m as i32),
            min_score: value.min_score,
            max_score: value.max_score,
        }
    }
}

/// 表示列存在性查询配置。`ExistsQuery` 也叫 NULL 查询或者空值查询，
/// 一般用于判断稀疏数据中某一行的某一列是否存在。例如查询所有数据中 `address` 列不为空的行。
#[derive(Debug, Clone, Default)]
pub struct ExistsQuery {
    pub field_name: String,
}

impl ExistsQuery {
    pub fn new(field_name: &str) -> Self {
        Self {
            field_name: field_name.to_string(),
        }
    }

    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid exists field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<ExistsQuery> for crate::protos::search::ExistsQuery {
    fn from(value: ExistsQuery) -> Self {
        Self {
            field_name: Some(value.field_name),
        }
    }
}

/// 表示地理长方形范围查询配置。`GeoBoundingBoxQuery` 根据一个长方形范围的地理位置边界条件查询表中的数据。
/// 当一个地理位置点落在给出的长方形范围内时满足查询条件。
#[derive(Debug, Clone, Default)]
pub struct GeoBoundingBoxQuery {
    pub field_name: String,
    pub top_left: GeoPoint,
    pub bottom_right: GeoPoint,
}

impl GeoBoundingBoxQuery {
    pub fn new(field_name: &str, top_left: GeoPoint, bottom_right: GeoPoint) -> Self {
        Self {
            field_name: field_name.to_string(),
            top_left,
            bottom_right,
        }
    }

    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    pub fn top_left(mut self, top_left: GeoPoint) -> Self {
        self.top_left = top_left;

        self
    }

    pub fn bottom_right(mut self, bottom_right: GeoPoint) -> Self {
        self.bottom_right = bottom_right;

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid geo bounding box field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<GeoBoundingBoxQuery> for crate::protos::search::GeoBoundingBoxQuery {
    fn from(value: GeoBoundingBoxQuery) -> Self {
        Self {
            field_name: Some(value.field_name),
            top_left: Some(format!("{}", value.top_left)),
            bottom_right: Some(format!("{}", value.bottom_right)),
        }
    }
}

/// 表示地理距离查询配置。`GeoDistanceQuery` 根据一个地理位置点与给定中心点之间的距离查询表中的数据。
/// 当一个地理位置点落在给定的距离范围内时满足查询条件。
#[derive(Debug, Clone, Default)]
pub struct GeoDistanceQuery {
    /// 字段名称
    pub field_name: String,
    /// 中心点
    pub center: GeoPoint,
    /// 距离，以米为单位
    pub distance_in_meter: f64,
}

impl GeoDistanceQuery {
    pub fn new(field_name: &str, center: GeoPoint, distance_in_meter: f64) -> Self {
        Self {
            field_name: field_name.to_string(),
            center,
            distance_in_meter,
        }
    }

    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    pub fn center(mut self, center: GeoPoint) -> Self {
        self.center = center;

        self
    }

    pub fn distance_in_meter(mut self, distance_in_meter: f64) -> Self {
        self.distance_in_meter = distance_in_meter;

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid geo distance field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<GeoDistanceQuery> for crate::protos::search::GeoDistanceQuery {
    fn from(value: GeoDistanceQuery) -> Self {
        Self {
            field_name: Some(value.field_name),
            center_point: Some(format!("{}", value.center)),
            distance: Some(value.distance_in_meter),
        }
    }
}

/// 表示地理多边形范围查询配置。`GeoPolygonQuery` 根据一个多边形范围的地理位置边界条件查询表中的数据。
/// 当一个地理位置点落在给出的多边形范围内时满足查询条件。
#[derive(Debug, Clone, Default)]
pub struct GeoPolygonQuery {
    pub field_name: String,
    pub points: Vec<GeoPoint>,
}

impl GeoPolygonQuery {
    pub fn new(field_name: &str, points: impl IntoIterator<Item = GeoPoint>) -> Self {
        Self {
            field_name: field_name.to_string(),
            points: points.into_iter().collect(),
        }
    }

    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 添加一个点
    pub fn point(mut self, point: GeoPoint) -> Self {
        self.points.push(point);

        self
    }

    /// 设置所有的点
    pub fn points(mut self, points: impl IntoIterator<Item = GeoPoint>) -> Self {
        self.points = points.into_iter().collect();

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid geo polygon field name: {}", self.field_name)));
        }

        if self.points.len() < 3 {
            return Err(OtsError::ValidationFailed(format!("invalid geo polygon points: {}", self.points.len())));
        }

        Ok(())
    }
}

impl From<GeoPolygonQuery> for crate::protos::search::GeoPolygonQuery {
    fn from(value: GeoPolygonQuery) -> Self {
        Self {
            field_name: Some(value.field_name),
            points: value.points.into_iter().map(|p| format!("{}", p)).collect(),
        }
    }
}
/// `KnnVectorQuery` 使用数值向量进行近似最近邻查询，可以在大规模数据集中找到最相似的数据项。
#[derive(Debug, Clone, Default)]
pub struct KnnVectorQuery {
    /// 向量字段名称
    pub field_name: String,

    /// 要查询相似度的向量
    pub vector: Vec<f32>,

    /// 查询最邻近的 topK 个值
    pub top_k: u32,

    /// 查询过滤器，支持组合使用任意的非向量检索的查询条件。
    pub filter: Option<Query>,

    /// 查询条件的权重配置
    pub weight: Option<f32>,

    /// 最小得分
    ///
    /// 控制向量查询（当前子查询）返回向量的最小分数门限，选填，默认值为 `0`（表示不过滤任何向量），取值范围大于等于 `0`。
    /// 若设置此项，查询返回的结果集中将不会出现与查询向量计算距离得分小于 `min_score` 的向量。
    pub min_score: Option<f32>,

    /// 候选数量
    ///
    /// 控制向量查询放大，选填，取值范围为 `[topK, maxTopK]`。
    /// `num_candidates` 的值越大，引擎查询时访问的数据越多，返回结果的召回率也就越高，但是查询耗时可能会变长。
    pub num_candidates: Option<u32>,
}

impl KnnVectorQuery {
    pub fn new(field_name: &str, vector: impl Into<Vec<f32>>, top_k: u32) -> Self {
        Self {
            field_name: field_name.to_string(),
            vector: vector.into(),
            top_k,
            ..Default::default()
        }
    }

    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    pub fn vector(mut self, vector: impl Into<Vec<f32>>) -> Self {
        self.vector = vector.into();

        self
    }

    pub fn top_k(mut self, top_k: u32) -> Self {
        self.top_k = top_k;

        self
    }

    pub fn filter(mut self, filter: Query) -> Self {
        self.filter = Some(filter);

        self
    }

    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);

        self
    }

    pub fn min_score(mut self, min_score: f32) -> Self {
        self.min_score = Some(min_score);

        self
    }

    pub fn num_candidates(mut self, num_candidates: u32) -> Self {
        self.num_candidates = Some(num_candidates);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid knn vector field name: {}", self.field_name)));
        }

        if self.vector.is_empty() {
            return Err(OtsError::ValidationFailed("invalid knn vector: empty vector".to_string()));
        }

        if self.top_k == 0 {
            return Err(OtsError::ValidationFailed(format!("invalid knn top k: {}", self.top_k)));
        }

        if self.top_k > i32::MAX as u32 {
            return Err(OtsError::ValidationFailed(format!("invalid knn top k: {}", self.top_k)));
        }

        if let Some(n) = self.min_score {
            if n < 0.0 {
                return Err(OtsError::ValidationFailed(format!("invalid knn min score: {}", n)));
            }
        }

        if let Some(n) = self.num_candidates {
            if n < self.top_k {
                return Err(OtsError::ValidationFailed(format!("invalid knn num candidates: {}", n)));
            }

            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("invalid knn num candidates: {}", n)));
            }
        }

        Ok(())
    }
}

impl From<KnnVectorQuery> for crate::protos::search::KnnVectorQuery {
    fn from(value: KnnVectorQuery) -> Self {
        Self {
            field_name: Some(value.field_name),
            float32_query_vector: value.vector,
            top_k: Some(value.top_k as i32),
            filter: value.filter.map(|f| f.into()),
            weight: value.weight,
            min_score: value.min_score,
            num_candidates: value.num_candidates.map(|n| n as i32),
        }
    }
}

/// `NestedQuery` 数据类型定义，表示嵌套类型查询配置。`NestedQuery` 用于查询嵌套类型字段中子行的数据。
/// 嵌套类型不能直接查询，需要通过 `NestedQuery` 包装，`NestedQuery` 中需要指定嵌套类型字段的路径和一个子查询，其中子查询可以是任意 `Query` 类型。
#[derive(Debug, Clone)]
pub struct NestedQuery {
    /// 嵌套类型字段的路径。
    pub path: String,

    /// 子查询。
    pub query: Query,

    /// 评分模式。
    pub score_mode: Option<ScoreMode>,

    /// 权重配置。
    pub weight: Option<f32>,

    /// 嵌套类型字段的子列的配置参数。
    pub inner_hits: Option<InnerHits>,
}

impl NestedQuery {
    pub fn new(path: &str, query: Query) -> Self {
        Self {
            path: path.to_string(),
            query,
            score_mode: None,
            weight: None,
            inner_hits: None,
        }
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();

        self
    }

    pub fn query(mut self, query: Query) -> Self {
        self.query = query;

        self
    }

    pub fn score_mode(mut self, score_mode: ScoreMode) -> Self {
        self.score_mode = Some(score_mode);

        self
    }

    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);

        self
    }

    pub fn inner_hits(mut self, inner_hits: InnerHits) -> Self {
        self.inner_hits = Some(inner_hits);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        self.query.validate()?;

        if let Some(inner_hits) = &self.inner_hits {
            inner_hits.validate()?;
        }

        Ok(())
    }
}

impl From<NestedQuery> for crate::protos::search::NestedQuery {
    fn from(value: NestedQuery) -> Self {
        let NestedQuery {
            path,
            query,
            score_mode,
            weight,
            inner_hits,
        } = value;

        Self {
            path: Some(path),
            query: Some(query.into()),
            score_mode: score_mode.map(|m| m as i32),
            weight,
            inner_hits: inner_hits.map(|h| h.into()),
        }
    }
}

/// 表示前缀匹配配置。`PrefixQuery` 根据前缀条件查询表中的数据。对于 `Text` 类型字段，只要分词后的词条中有词条满足前缀条件即可。
#[derive(Debug, Clone, Default)]
pub struct PrefixQuery {
    /// 字段名称。
    pub field_name: String,

    /// 前缀。
    pub prefix: String,

    /// 权重配置。
    pub weight: Option<f32>,
}

impl PrefixQuery {
    pub fn new(field_name: &str, prefix: impl Into<String>) -> Self {
        Self {
            field_name: field_name.to_string(),
            prefix: prefix.into(),
            weight: None,
        }
    }

    pub fn field_name(mut self, field_name: impl Into<String>) -> Self {
        self.field_name = field_name.into();
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid field name: {}", self.field_name)));
        }
        Ok(())
    }
}

impl From<PrefixQuery> for crate::protos::search::PrefixQuery {
    fn from(value: PrefixQuery) -> Self {
        let PrefixQuery { field_name, prefix, weight } = value;

        Self {
            field_name: Some(field_name),
            prefix: Some(prefix),
            weight,
        }
    }
}

/// `SuffixQuery` 数据类型定义，表示后缀查询配置。`SuffixQuery` 通过指定后缀条件查询索引中的数据，例如通过手机尾号后4位查询快递。
#[derive(Debug, Clone, Default)]
pub struct SuffixQuery {
    /// 字段名称。
    pub field_name: String,

    /// 后缀。
    pub suffix: String,

    /// 权重配置。
    pub weight: Option<f32>,
}

impl SuffixQuery {
    pub fn new(field_name: &str, suffix: impl Into<String>) -> Self {
        Self {
            field_name: field_name.to_string(),
            suffix: suffix.into(),
            weight: None,
        }
    }

    pub fn field_name(mut self, field_name: impl Into<String>) -> Self {
        self.field_name = field_name.into();
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid field name: {}", self.field_name)));
        }
        Ok(())
    }
}

impl From<SuffixQuery> for crate::protos::search::SuffixQuery {
    fn from(value: SuffixQuery) -> Self {
        let SuffixQuery { field_name, suffix, weight } = value;

        Self {
            field_name: Some(field_name),
            suffix: Some(suffix),
            weight,
        }
    }
}

/// 表示范围查询配置。`RangeQuery` 根据范围条件查询表中的数据。对于 `Text` 类型字段，只要分词后的词条中有词条满足范围条件即可。
#[derive(Debug, Clone, Default)]
pub struct RangeQuery {
    /// 字段名称。
    pub field_name: String,

    /// 范围查询的开始值。
    pub value_from: ColumnValue,

    /// 范围查询的结束值。
    pub value_to: ColumnValue,

    /// 取值范围是否包括 `value_from` 值。
    pub include_lower: bool,

    /// 取值范围是否包括 `value_to` 值。
    pub include_upper: bool,
}

impl RangeQuery {
    pub fn new(field_name: &str, value_from: ColumnValue, value_to: ColumnValue) -> Self {
        Self {
            field_name: field_name.to_string(),
            value_from,
            value_to,
            include_lower: false,
            include_upper: false,
        }
    }

    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 包含起始值
    pub fn value_from_inclusive(mut self, value_from: ColumnValue) -> Self {
        self.value_from = value_from;
        self.include_lower = true;

        self
    }

    /// 不包含起始值
    pub fn value_from_exclusive(mut self, value_from: ColumnValue) -> Self {
        self.value_from = value_from;
        self.include_lower = false;

        self
    }

    /// 包含结束值
    pub fn value_to_inclusive(mut self, value_to: ColumnValue) -> Self {
        self.value_to = value_to;
        self.include_upper = true;

        self
    }

    /// 不包含结束值
    pub fn value_to_exclusive(mut self, value_to: ColumnValue) -> Self {
        self.value_to = value_to;
        self.include_upper = false;

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid field name: {}", self.field_name)));
        }

        if self.value_from == ColumnValue::Null {
            return Err(OtsError::ValidationFailed("value from can not be null".to_string()));
        }

        if self.value_to == ColumnValue::Null {
            return Err(OtsError::ValidationFailed("value to can not be null".to_string()));
        }

        Ok(())
    }
}

impl From<RangeQuery> for crate::protos::search::RangeQuery {
    fn from(value: RangeQuery) -> Self {
        let RangeQuery {
            field_name,
            value_from,
            value_to,
            include_lower,
            include_upper,
        } = value;

        Self {
            field_name: Some(field_name),
            range_from: Some(value_from.encode_plain_buffer()),
            range_to: Some(value_to.encode_plain_buffer()),
            include_lower: Some(include_lower),
            include_upper: Some(include_upper),
        }
    }
}

/// 表示精确查询配置。`TermQuery` 采用完整精确匹配的方式查询表中的数据，类似于字符串匹配。对于 `Text` 类型字段，只要分词后有词条可以精确匹配即可。
#[derive(Debug, Clone, Default)]
pub struct TermQuery {
    /// 字段名称。
    pub field_name: String,

    /// 值。
    pub value: ColumnValue,

    /// 权重配置。
    pub weight: Option<f32>,
}

impl TermQuery {
    pub fn new(field_name: &str, value: ColumnValue) -> Self {
        Self {
            field_name: field_name.to_string(),
            value,
            weight: None,
        }
    }

    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();
        self
    }

    pub fn value(mut self, value: ColumnValue) -> Self {
        self.value = value;
        self
    }

    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid field name: {}", self.field_name)));
        }

        if self.value == ColumnValue::Null {
            return Err(OtsError::ValidationFailed("value can not be null".to_string()));
        }

        Ok(())
    }
}

impl From<TermQuery> for crate::protos::search::TermQuery {
    fn from(value: TermQuery) -> Self {
        let TermQuery { field_name, value, weight } = value;

        Self {
            field_name: Some(field_name),
            term: Some(value.encode_plain_buffer()),
            weight,
        }
    }
}

/// 表示多值精确查询配置。TermsQuery采用完整精确匹配的方式查询表中的数据，类似于字符串匹配。对于Text类型字段，只要分词后有词条可以精确匹配即可。
#[derive(Debug, Clone, Default)]
pub struct TermsQuery {
    /// 字段名称。
    pub field_name: String,

    /// 值列表。
    pub values: Vec<ColumnValue>,

    /// 权重配置。
    pub weight: Option<f32>,
}

impl TermsQuery {
    pub fn new(field_name: &str, values: impl IntoIterator<Item = ColumnValue>) -> Self {
        Self {
            field_name: field_name.to_string(),
            values: values.into_iter().collect(),
            weight: None,
        }
    }

    /// 设置字段名
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();
        self
    }

    /// 添加一个值
    pub fn value(mut self, value: ColumnValue) -> Self {
        self.values.push(value);
        self
    }

    /// 设置值列表
    pub fn values(mut self, values: impl IntoIterator<Item = ColumnValue>) -> Self {
        self.values = values.into_iter().collect();
        self
    }

    /// 设置权重
    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid field name: {}", self.field_name)));
        }

        if self.values.is_empty() {
            return Err(OtsError::ValidationFailed("values can not be empty".to_string()));
        }

        for value in &self.values {
            if value == &ColumnValue::Null {
                return Err(OtsError::ValidationFailed("value can not be null".to_string()));
            }
        }

        Ok(())
    }
}

impl From<TermsQuery> for crate::protos::search::TermsQuery {
    fn from(value: TermsQuery) -> Self {
        let TermsQuery { field_name, values, weight } = value;

        Self {
            field_name: Some(field_name),
            terms: values.iter().map(|v| v.encode_plain_buffer()).collect(),
            weight,
        }
    }
}

/// 表示通配符查询配置。WildcardQuery 中要匹配的值可以是一个带有通配符的字符串，
/// 目前支持星号（`*`）和半角问号（`?`）两种通配符。
/// 要匹配的值中可以用星号（`*`）代表任意字符序列，
/// 或者用半角问号（`?`）代表任意单个字符，
/// 且支持以星号（`*`）或半角问号（`?`）开头。
/// 例如查询 `"table*e"`，可以匹配到 `"tablestore"`。
#[derive(Debug, Clone, Default)]
pub struct WildcardQuery {
    pub field_name: String,
    pub value: String,
    pub weight: Option<f32>,
}

impl WildcardQuery {
    pub fn new(field_name: &str, value: impl Into<String>) -> Self {
        Self {
            field_name: field_name.to_string(),
            value: value.into(),
            weight: None,
        }
    }

    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("Invalid field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<WildcardQuery> for crate::protos::search::WildcardQuery {
    fn from(value: WildcardQuery) -> Self {
        let WildcardQuery { field_name, value, weight } = value;

        Self {
            field_name: Some(field_name),
            value: Some(value),
            weight,
        }
    }
}

/// 多元索引查询条件枚举
#[derive(Debug, Clone)]
pub enum Query {
    Match(MatchQuery),
    MatchAll(MatchAllQuery),
    MatchPhrase(MatchPhraseQuery),
    Bool(BoolQuery),
    ConstScore(Box<ConstScoreQuery>),
    FunctionsScore(Box<FunctionsScoreQuery>),
    Exists(ExistsQuery),
    GeoBoundingBox(GeoBoundingBoxQuery),
    GeoDistance(GeoDistanceQuery),
    GeoPolygon(GeoPolygonQuery),
    KnnVector(Box<KnnVectorQuery>),
    Nested(Box<NestedQuery>),
    Prefix(PrefixQuery),
    Suffix(SuffixQuery),
    Range(RangeQuery),
    Term(TermQuery),
    Terms(TermsQuery),
    Wildcard(WildcardQuery),
}

impl From<Query> for crate::protos::search::Query {
    fn from(value: Query) -> Self {
        match value {
            Query::Match(mq) => Self {
                r#type: Some(QueryType::MatchQuery as i32),
                query: Some(crate::protos::search::MatchQuery::from(mq).encode_to_vec()),
            },

            Query::MatchAll(mq) => Self {
                r#type: Some(QueryType::MatchAllQuery as i32),
                query: Some(crate::protos::search::MatchAllQuery::from(mq).encode_to_vec()),
            },

            Query::MatchPhrase(mq) => Self {
                r#type: Some(QueryType::MatchPhraseQuery as i32),
                query: Some(crate::protos::search::MatchPhraseQuery::from(mq).encode_to_vec()),
            },

            Query::Bool(bq) => Self {
                r#type: Some(QueryType::BoolQuery as i32),
                query: Some(crate::protos::search::BoolQuery::from(bq).encode_to_vec()),
            },

            Query::ConstScore(cq) => Self {
                r#type: Some(QueryType::ConstScoreQuery as i32),
                query: Some(crate::protos::search::ConstScoreQuery::from(*cq).encode_to_vec()),
            },

            Query::FunctionsScore(fq) => Self {
                r#type: Some(QueryType::FunctionsScoreQuery as i32),
                query: Some(crate::protos::search::FunctionsScoreQuery::from(*fq).encode_to_vec()),
            },

            Query::Exists(eq) => Self {
                r#type: Some(QueryType::ExistsQuery as i32),
                query: Some(crate::protos::search::ExistsQuery::from(eq).encode_to_vec()),
            },

            Query::GeoBoundingBox(gbq) => Self {
                r#type: Some(QueryType::GeoBoundingBoxQuery as i32),
                query: Some(crate::protos::search::GeoBoundingBoxQuery::from(gbq).encode_to_vec()),
            },

            Query::GeoDistance(gdq) => Self {
                r#type: Some(QueryType::GeoDistanceQuery as i32),
                query: Some(crate::protos::search::GeoDistanceQuery::from(gdq).encode_to_vec()),
            },

            Query::GeoPolygon(gpq) => Self {
                r#type: Some(QueryType::GeoPolygonQuery as i32),
                query: Some(crate::protos::search::GeoPolygonQuery::from(gpq).encode_to_vec()),
            },

            Query::KnnVector(kq) => Self {
                r#type: Some(QueryType::KnnVectorQuery as i32),
                query: Some(crate::protos::search::KnnVectorQuery::from(*kq).encode_to_vec()),
            },

            Query::Nested(nq) => Self {
                r#type: Some(QueryType::NestedQuery as i32),
                query: Some(crate::protos::search::NestedQuery::from(*nq).encode_to_vec()),
            },

            Query::Prefix(pq) => Self {
                r#type: Some(QueryType::PrefixQuery as i32),
                query: Some(crate::protos::search::PrefixQuery::from(pq).encode_to_vec()),
            },

            Query::Suffix(sq) => Self {
                r#type: Some(QueryType::SuffixQuery as i32),
                query: Some(crate::protos::search::SuffixQuery::from(sq).encode_to_vec()),
            },

            Query::Range(rq) => Self {
                r#type: Some(QueryType::RangeQuery as i32),
                query: Some(crate::protos::search::RangeQuery::from(rq).encode_to_vec()),
            },

            Query::Term(tq) => Self {
                r#type: Some(QueryType::TermQuery as i32),
                query: Some(crate::protos::search::TermQuery::from(tq).encode_to_vec()),
            },

            Query::Terms(tq) => Self {
                r#type: Some(QueryType::TermsQuery as i32),
                query: Some(crate::protos::search::TermsQuery::from(tq).encode_to_vec()),
            },

            Query::Wildcard(wq) => Self {
                r#type: Some(QueryType::WildcardQuery as i32),
                query: Some(crate::protos::search::WildcardQuery::from(wq).encode_to_vec()),
            },
        }
    }
}

impl Query {
    pub fn validate(&self) -> OtsResult<()> {
        match self {
            Query::Match(mq) => mq.validate(),
            Query::MatchAll(_) => Ok(()),
            Query::MatchPhrase(mq) => mq.validate(),
            Query::Bool(bq) => bq.validate(),
            Query::ConstScore(cq) => cq.validate(),
            Query::FunctionsScore(fq) => fq.validate(),
            Query::Exists(eq) => eq.validate(),
            Query::GeoBoundingBox(gbq) => gbq.validate(),
            Query::GeoDistance(gdq) => gdq.validate(),
            Query::GeoPolygon(gpq) => gpq.validate(),
            Query::KnnVector(kq) => kq.validate(),
            Query::Nested(nq) => nq.validate(),
            Query::Prefix(pq) => pq.validate(),
            Query::Suffix(sq) => sq.validate(),
            Query::Range(rq) => rq.validate(),
            Query::Term(tq) => tq.validate(),
            Query::Terms(tq) => tq.validate(),
            Query::Wildcard(wq) => wq.validate(),
        }
    }
}

/// 嵌套类型字段的子列的配置参数。
#[derive(Debug, Clone, Default)]
pub struct InnerHits {
    /// nested 子列的返回时的排序规则
    pub sort: Option<Sort>,

    /// 当 nested 子列为数组形式时，子列分页返回的起始位置
    pub offset: Option<u32>,

    /// 当 nested 子列为数组形式时，子列分页返回的行数
    pub limit: Option<u32>,

    /// nested 子列高亮参数配置
    pub highlight: Option<Highlight>,
}

impl InnerHits {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort = Some(sort);

        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);

        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);

        self
    }

    pub fn highlight(mut self, highlight: Highlight) -> Self {
        self.highlight = Some(highlight);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if let Some(sort) = &self.sort {
            sort.validate()?;
        }

        if let Some(highlight) = &self.highlight {
            highlight.validate()?;
        }

        if let Some(n) = self.offset {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("invalid inner hits offset: {}", n)));
            }
        }

        if let Some(n) = self.limit {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("invalid inner hits limit: {}", n)));
            }
        }

        Ok(())
    }
}

impl From<InnerHits> for crate::protos::search::InnerHits {
    fn from(value: InnerHits) -> Self {
        let InnerHits {
            sort,
            offset,
            limit,
            highlight,
        } = value;

        Self {
            sort: sort.map(|s| s.into()),
            offset: offset.map(|o| o as i32),
            limit: limit.map(|l| l as i32),
            highlight: highlight.map(|h| h.into()),
        }
    }
}

/// HighlightParameter数据类型定义，表示高亮参数。
#[derive(Debug, Default, Clone)]
pub struct HighlightParameter {
    /// 字段名称。请确保在创建多元索引时已为该字段开启查询摘要与高亮。
    pub field_name: String,

    /// 返回高亮分片的最大数量，推荐设置为1。
    pub number_of_fragments: Option<u32>,

    /// 每个分片的长度
    pub fragment_size: Option<u32>,

    /// 查询词高亮的前置 Tag，例如 `<em>`、 `<b>`。
    /// 默认值为 `<em>`，您可以按需自定义前置 Tag。
    /// 支持的字符集包括 `< > " ' /、a-z、A-Z、0-9`。
    pub pre_tag: Option<String>,

    /// 查询词高亮的后置 Tag，例如 `</em>`、 `</b>`。
    /// 默认值为 `</em>`，您可以按需自定义后置Tag。
    /// 支持的字符集包括 `< > " ' /、a-z、A-Z、0-9`。
    pub post_tag: Option<String>,

    /// 当高亮字段返回多个分片时，分片的排序规则。
    pub fragments_order: Option<HighlightFragmentOrder>,
}

impl HighlightParameter {
    pub fn new(field_name: &str) -> Self {
        Self {
            field_name: field_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置字段名称
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置返回分片的最大数量
    pub fn number_of_fragments(mut self, n: u32) -> Self {
        self.number_of_fragments = Some(n);

        self
    }

    /// 设置每个分片的长度
    pub fn fragment_size(mut self, n: u32) -> Self {
        self.fragment_size = Some(n);

        self
    }

    /// 设置开始标签
    pub fn pre_tag(mut self, pre_tag: impl Into<String>) -> Self {
        self.pre_tag = Some(pre_tag.into());

        self
    }

    /// 设置结束标签
    pub fn post_tag(mut self, post_tag: impl Into<String>) -> Self {
        self.post_tag = Some(post_tag.into());

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid highlight field name: {}", self.field_name)));
        }

        if let Some(n) = self.number_of_fragments {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("invalid number of fragments: {}", n)));
            }
        }

        if let Some(n) = self.fragment_size {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("invalid fragment size: {}", n)));
            }
        }

        Ok(())
    }
}

impl From<HighlightParameter> for crate::protos::search::HighlightParameter {
    fn from(value: HighlightParameter) -> Self {
        let HighlightParameter {
            field_name,
            number_of_fragments,
            fragment_size,
            pre_tag,
            post_tag,
            fragments_order,
        } = value;

        Self {
            field_name: Some(field_name),
            number_of_fragments: number_of_fragments.map(|n| n as i32),
            fragment_size: fragment_size.map(|n| n as i32),
            pre_tag,
            post_tag,
            fragments_order: fragments_order.map(|o| o as i32),
        }
    }
}

/// 查询摘要与高亮配置。
#[derive(Debug, Default, Clone)]
pub struct Highlight {
    /// 高亮参数。仅支持设置SearchQuery中包含关键词查询的字段。
    parameters: Vec<HighlightParameter>,

    /// 对高亮分片原文内容的编码方式。
    encoder: Option<HighlightEncoder>,
}

impl Highlight {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加一个高亮参数
    pub fn parameter(mut self, param: HighlightParameter) -> Self {
        self.parameters.push(param);

        self
    }

    /// 设置高亮参数
    pub fn parameters(mut self, params: impl IntoIterator<Item = HighlightParameter>) -> Self {
        self.parameters = params.into_iter().collect();

        self
    }

    /// 设置编码方式
    pub fn encoder(mut self, encoder: HighlightEncoder) -> Self {
        self.encoder = Some(encoder);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        for p in &self.parameters {
            p.validate()?;
        }

        Ok(())
    }
}

impl From<Highlight> for crate::protos::search::Highlight {
    fn from(value: Highlight) -> Self {
        let Highlight { parameters, encoder } = value;

        Self {
            highlight_parameters: parameters.into_iter().map(|h| h.into()).collect(),
            highlight_encoder: encoder.map(|e| e as i32),
        }
    }
}

/// 多元索引数据查询配置
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// 查询条件。
    pub query: Query,

    /// 过滤器中的过滤条件
    pub filter: Option<Query>,

    /// 本次查询的开始位置。
    pub offset: Option<u32>,

    /// 本次查询需要返回的最大数量。
    pub limit: Option<u32>,

    /// 当符合查询条件的数据未读取完时，服务端会返回 `next_token`，此时可以使用 `next_token` 继续读取后面的数据。
    pub token: Vec<u8>,

    /// 按照指定列对返回结果进行去重。
    ///
    /// 按该列对结果集做折叠，只支持应用于整型、浮点数和 `Keyword` 类型的列，不支持数组类型的列。
    pub collapse_field_name: Option<String>,

    /// 返回结果的排序方式。
    pub sorters: Vec<Sorter>,

    /// 当指定非 PrimaryKeySort 的 sorter 时，默认情况下会主动添加 PrimaryKeySort，
    /// 通过该参数可禁止主动添加 PrimaryKeySort
    pub disable_default_pk_sorter: bool,

    /// 是否返回匹配的总行数，默认为 `false`，表示不返回。
    /// 返回匹配的总行数会影响查询性能。
    pub track_total_count: bool,

    /// 分组配置。
    pub group_bys: Vec<GroupBy>,

    /// 统计聚合配置。
    pub aggregations: Vec<Aggregation>,

    /// 查询摘要与高亮配置
    pub highlight: Option<Highlight>,
}

impl SearchQuery {
    pub fn new(query: Query) -> Self {
        Self {
            query,
            filter: None,
            offset: None,
            limit: None,
            token: vec![],
            collapse_field_name: None,
            sorters: vec![],
            disable_default_pk_sorter: false,
            track_total_count: false,
            group_bys: vec![],
            aggregations: vec![],
            highlight: None,
        }
    }

    /// 设置查询条件
    pub fn query(mut self, query: Query) -> Self {
        self.query = query;

        self
    }

    /// 设置查询中的过滤器
    pub fn filter(mut self, filter: Query) -> Self {
        self.filter = Some(filter);

        self
    }

    /// 设置偏移量
    pub fn offset(mut self, n: u32) -> Self {
        self.offset = Some(n);

        self
    }

    /// 设置本次查询需要返回的最大数量
    pub fn limit(mut self, n: u32) -> Self {
        self.limit = Some(n);

        self
    }

    /// 设置查询的游标
    pub fn token(mut self, token: impl Into<Vec<u8>>) -> Self {
        self.token = token.into();

        self
    }

    /// 设置对返回结果进行去重的列名
    pub fn collapse_field_name(mut self, field_name: impl Into<String>) -> Self {
        self.collapse_field_name = Some(field_name.into());

        self
    }

    /// 设置是否返回匹配的总行数
    pub fn track_total_count(mut self, total_count: bool) -> Self {
        self.track_total_count = total_count;

        self
    }

    /// 添加一个排序设置
    pub fn sorter(mut self, sorter: Sorter) -> Self {
        self.sorters.push(sorter);

        self
    }

    /// 设置排序
    pub fn sorters(mut self, sorters: impl IntoIterator<Item = Sorter>) -> Self {
        self.sorters = sorters.into_iter().collect();

        self
    }

    /// 设置是否禁用主动添加 PrimaryKeySort，
    pub fn disable_default_pk_sorter(mut self, disable_default_pk_sorter: bool) -> Self {
        self.disable_default_pk_sorter = disable_default_pk_sorter;

        self
    }

    /// 添加一个聚合配置
    pub fn aggregation(mut self, aggr: Aggregation) -> Self {
        self.aggregations.push(aggr);

        self
    }

    /// 设置聚合配置
    pub fn aggregations(mut self, aggrs: impl IntoIterator<Item = Aggregation>) -> Self {
        self.aggregations = aggrs.into_iter().collect();

        self
    }

    /// 添加一个分组配置
    pub fn group_by(mut self, group: GroupBy) -> Self {
        self.group_bys.push(group);

        self
    }

    /// 设置分组
    pub fn group_bys(mut self, groups: impl IntoIterator<Item = GroupBy>) -> Self {
        self.group_bys = groups.into_iter().collect();

        self
    }

    /// 设置高亮
    pub fn highlight(mut self, highlight: Highlight) -> Self {
        self.highlight = Some(highlight);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        self.query.validate()?;

        if let Some(n) = self.offset {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("invalid offset: {}", n)));
            }
        }

        if let Some(n) = self.limit {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed(format!("invalid limit: {}", n)));
            }
        }

        if let Some(s) = &self.collapse_field_name {
            if !validate_column_name(s) {
                return Err(OtsError::ValidationFailed(format!("invalid collapse field name: {}", s)));
            }
        }

        if let Some(h) = &self.highlight {
            h.validate()?;
        }

        for s in &self.sorters {
            s.validate()?;
        }

        for g in &self.group_bys {
            g.validate()?;
        }

        for a in &self.aggregations {
            a.validate()?;
        }

        Ok(())
    }
}

impl From<SearchQuery> for crate::protos::search::SearchQuery {
    fn from(value: SearchQuery) -> Self {
        let SearchQuery {
            query,
            filter,
            offset,
            limit,
            token,
            collapse_field_name,
            sorters,
            disable_default_pk_sorter,
            track_total_count,
            group_bys,
            aggregations,
            highlight,
        } = value;

        let sort = crate::protos::search::Sort::from(Sort::with_sorters(sorters, disable_default_pk_sorter));

        Self {
            offset: offset.map(|n| n as i32),
            limit: limit.map(|n| n as i32),
            query: Some(query.into()),
            collapse: collapse_field_name.map(|f| Collapse { field_name: Some(f) }),
            sort: Some(sort),
            token: if !token.is_empty() { Some(token) } else { None },
            aggs: Some(aggregations.into()),
            group_bys: Some(group_bys.into()),
            highlight: highlight.map(|h| h.into()),
            track_total_count: if track_total_count { Some(i32::MAX) } else { Some(-1) }, // Copy from Java SDK
            filter: filter.map(|f| SearchFilter { query: Some(f.into()) }),
        }
    }
}
