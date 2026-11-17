use prost::Message;

use super::{Duration, GeoPoint, Query};
use crate::model::rules::validate_column_name;
use crate::{
    error::OtsError,
    protos::search::{DecayFuncParamType, DecayMathFunction, FunctionModifier, MultiValueMode},
    OtsResult,
};

/// 在 [`FunctionsScoreQuery`](`crate::search::FunctionsScoreQuery`) 中使用，
/// 该函数的功能是对 doc 中的某个 field（必须为 `long` 或者 `double` 类型）简单运算打分。
/// 例如：在 [`FunctionsScoreQuery`](`crate::search::FunctionsScoreQuery`) 的 `query`
/// 中使用 [`MatchQuery`](`crate::search::MatchQuery`) 查询姓名中含有“明”的同学，
/// 但是想对返回结果按照身高进行排序，此时可以使用此函数，在 `fieldName` 字段设置身高，`factor` 与身高 field 相乘，
/// 控制权重，modifier 控制打分算法，包括平方、开方、取对数等简单运算，missing 用于设置 field 缺省值。
/// 运算举例：`fieldName：height`，`factor：1.2f`，`modifier：LOG1P`，则 `score = LOG1P(1.2f * height)`
#[derive(Debug, Clone)]
pub struct FieldValueFactorFunction {
    pub field_name: String,
    pub factor: f32,
    pub modifier: FunctionModifier,
    pub missing_value: Option<f64>,
}

impl Default for FieldValueFactorFunction {
    fn default() -> Self {
        Self {
            field_name: "".to_string(),
            factor: 1.0,
            modifier: FunctionModifier::FmNone,
            missing_value: None,
        }
    }
}

impl FieldValueFactorFunction {
    pub fn new(field_name: &str, factor: f32) -> Self {
        Self {
            field_name: field_name.to_string(),
            factor,
            ..Default::default()
        }
    }

    /// 设置字段名
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();
        self
    }

    /// 设置因子
    pub fn factor(mut self, factor: f32) -> Self {
        self.factor = factor;
        self
    }

    /// 设置打分算法
    pub fn modifier(mut self, modifier: FunctionModifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// 设置缺省值
    pub fn missing_value(mut self, missing_value: f64) -> Self {
        self.missing_value = Some(missing_value);
        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<FieldValueFactorFunction> for crate::protos::search::FieldValueFactorFunction {
    fn from(value: FieldValueFactorFunction) -> Self {
        Self {
            field_name: Some(value.field_name),
            factor: Some(value.factor),
            modifier: Some(value.modifier as i32),
            missing: value.missing_value,
        }
    }
}

/// `origin` 设置时可以选择 `i64` 类型的纳秒时间戳，或者 `String` 类型、符合时间 `format` 的字符串，请任选其一设置。
#[derive(Debug, Clone)]
pub enum DecayDateParamVariant {
    /// 纳秒时间戳
    Long(i64),

    /// 符合时间 format 的字符串
    String(String),
}

/// 适用于 `Date` 类型 field。
/// `origin` 设置时可以选择 `i64` 类型的纳秒时间戳，或者 `String` 类型、符合时间 format 的字符串，请任选其一设置。
/// `scale` 和 `offset` 为 [`Duration`](`crate::search::Duration`) 类型，表示时间间隔。
/// 最大支持的时间单位为DateTimeUnit.DAY，并且 `scale` 应大于 `0`， `offset` 应大于等于 `0`
#[derive(Debug, Clone)]
pub struct DecayDateParam {
    pub origin: DecayDateParamVariant,
    pub scale: Duration,
    pub offset: Duration,
}

impl DecayDateParam {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        match &self.scale {
            Duration::Day(value) => {
                if *value <= 0 {
                    return Err(OtsError::ValidationFailed("scale must be greater than 0".to_string()));
                }
            }
            Duration::Hour(value) => {
                if *value <= 0 {
                    return Err(OtsError::ValidationFailed("scale must be greater than 0".to_string()));
                }
            }

            Duration::Minute(value) => {
                if *value <= 0 {
                    return Err(OtsError::ValidationFailed("scale must be greater than 0".to_string()));
                }
            }

            Duration::Second(value) => {
                if *value <= 0 {
                    return Err(OtsError::ValidationFailed("scale must be greater than 0".to_string()));
                }
            }

            Duration::Millisecond(value) => {
                if *value <= 0 {
                    return Err(OtsError::ValidationFailed("scale must be greater than 0".to_string()));
                }
            }

            _ => {
                return Err(OtsError::ValidationFailed(
                    "scale support only day, hour, minute, second, millisecond".to_string(),
                ));
            }
        }

        match &self.offset {
            Duration::Day(value) => {
                if *value < 0 {
                    return Err(OtsError::ValidationFailed("offset must be greater than or equal to 0".to_string()));
                }
            }

            Duration::Hour(value) => {
                if *value < 0 {
                    return Err(OtsError::ValidationFailed("offset must be greater than or equal to 0".to_string()));
                }
            }

            Duration::Minute(value) => {
                if *value < 0 {
                    return Err(OtsError::ValidationFailed("offset must be greater than or equal to 0".to_string()));
                }
            }

            Duration::Second(value) => {
                if *value < 0 {
                    return Err(OtsError::ValidationFailed("offset must be greater than or equal to 0".to_string()));
                }
            }

            Duration::Millisecond(value) => {
                if *value < 0 {
                    return Err(OtsError::ValidationFailed("offset must be greater than or equal to 0".to_string()));
                }
            }

            _ => {
                return Err(OtsError::ValidationFailed(
                    "offset support only day, hour, minute, second, millisecond".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl From<DecayDateParam> for crate::protos::search::DecayFuncDateParam {
    fn from(value: DecayDateParam) -> Self {
        Self {
            origin_long: if let DecayDateParamVariant::Long(value) = value.origin {
                Some(value)
            } else {
                None
            },
            origin_string: if let DecayDateParamVariant::String(value) = value.origin {
                Some(value)
            } else {
                None
            },
            scale: Some(value.scale.into()),
            offset: Some(value.offset.into()),
        }
    }
}

/// 适用于Geo-point类型field
#[derive(Debug, Clone)]
pub struct DecayGeoParam {
    pub origin: GeoPoint,

    /// 以米为单位，应大于 0
    pub scale: f64,

    /// 以米为单位，应大于等于 0
    pub offset: f64,
}

impl DecayGeoParam {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        if self.scale <= 0.0 {
            return Err(OtsError::ValidationFailed("scale must be greater than 0".to_string()));
        }

        if self.offset < 0.0 {
            return Err(OtsError::ValidationFailed("offset must be greater than or equal to 0".to_string()));
        }

        Ok(())
    }
}

impl From<DecayGeoParam> for crate::protos::search::DecayFuncGeoParam {
    fn from(value: DecayGeoParam) -> Self {
        Self {
            origin: Some(format!("{}", value.origin)),
            scale: Some(value.scale),
            offset: Some(value.offset),
        }
    }
}

/// 适用于 Long 和 Double 类型 field。
/// `origin`，`scale` 和 `offset` 是 `f64` 类型值，其中 `scale` 应大于 0，`offset` 应大于等于 0
#[derive(Debug, Clone)]
pub struct DecayNumericParam {
    pub origin: f64,
    pub scale: f64,
    pub offset: f64,
}

impl DecayNumericParam {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        if self.scale <= 0.0 {
            return Err(OtsError::ValidationFailed("scale must be greater than 0".to_string()));
        }

        if self.offset < 0.0 {
            return Err(OtsError::ValidationFailed("offset must be greater than or equal to 0".to_string()));
        }

        Ok(())
    }
}

impl From<DecayNumericParam> for crate::protos::search::DecayFuncNumericParam {
    fn from(value: DecayNumericParam) -> Self {
        Self {
            origin: Some(value.origin),
            scale: Some(value.scale),
            offset: Some(value.offset),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DecayParam {
    Date(DecayDateParam),
    Geo(DecayGeoParam),
    Numeric(DecayNumericParam),
}

impl DecayParam {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        match self {
            Self::Date(param) => param.validate(),
            Self::Geo(param) => param.validate(),
            Self::Numeric(param) => param.validate(),
        }
    }
}

/// 该函数用于根据 field 与目标值的相对距离打分，可以对 Geo-point、 Date 、 Long 和 Double 类型 field 打分。
/// 与之相对应的，decayParam 分为三种类型的参数设置，请根据field类型选择对应的参数设置。
///
/// param 中的 `origin`、 `scale` 和 `offset` 与 `DecayFunction` 中的 `decay` 共同用于计算分数，
/// 其中 `origin` 是打分的参照，`scale` 和 `decay` 设置分数衰减标准。
///
/// 与 `origin` 相对距离为 `scale` 的文档获得的分值为 `decay`，与 `origin` 相距距离小于 `offset` 的文档同样会获得最高分 `1` 分。
/// 打分使用的函数包括 `EXP`、`GAUSS` 和 `LINEAR` 三种。由 `math_function` 参数控制。
/// 对于数组类型的field，使用 `multi_value_mode`` 设置打分模式，MIN表示选取数组中最小值作为打分依据，以此类推……
///
/// 对某个 field 进行打分时，如果某个文档没有对应的 field，则该文档会获得 `1` 分（最高分），为了避免受到干扰，
/// 建议在 [`FunctionsScoreQuery`](`crate::search::FunctionsScoreQuery`) 中使用 [`ExistsQuery`](crate::search::ExistsQuery) 设置缺省值。
#[derive(Debug, Clone)]
pub struct DecayFunction {
    pub field_name: String,
    pub decay_param: DecayParam,
    pub decay: f64,
    pub math_function: DecayMathFunction,
    pub multi_value_mode: MultiValueMode,
}

impl DecayFunction {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid field name: {}", self.field_name)));
        }

        self.decay_param.validate()?;

        Ok(())
    }
}

impl From<DecayFunction> for crate::protos::search::DecayFunction {
    fn from(value: DecayFunction) -> Self {
        let DecayFunction {
            field_name,
            decay_param,
            decay,
            math_function,
            multi_value_mode,
        } = value;

        Self {
            field_name: Some(field_name),
            math_function: Some(math_function as i32),
            param_type: match &decay_param {
                DecayParam::Date(_) => Some(DecayFuncParamType::DfDateParam as i32),
                DecayParam::Geo(_) => Some(DecayFuncParamType::DfGeoParam as i32),
                DecayParam::Numeric(_) => Some(DecayFuncParamType::DfNumericParam as i32),
            },
            param: match decay_param {
                DecayParam::Date(param) => {
                    let msg = crate::protos::search::DecayFuncDateParam::from(param);

                    Some(msg.encode_to_vec())
                }

                DecayParam::Geo(param) => {
                    let msg = crate::protos::search::DecayFuncGeoParam::from(param);
                    Some(msg.encode_to_vec())
                }

                DecayParam::Numeric(param) => {
                    let msg = crate::protos::search::DecayFuncNumericParam::from(param);
                    Some(msg.encode_to_vec())
                }
            },
            decay: Some(decay),
            multi_value_mode: Some(multi_value_mode as i32),
        }
    }
}

/// 该函数可以为文档随机打分，返回随机的排序序列，每次返回结果不同
#[derive(Debug, Clone)]
pub struct RandomFunction {}

impl From<RandomFunction> for crate::protos::search::RandomScoreFunction {
    fn from(_: RandomFunction) -> Self {
        Self {}
    }
}

/// - 每个 ScoreFunction 都是一个打分函数，目前最多支持三个 ScoreFunction 同时打分。
/// - 每个 ScoreFunction 中都包含三种函数，请选择其中一种设置或均不设置（只使用 `filter` 和 `weight`）。
/// - ScoreFunction 可以设置 `weight`` 和 `filter`，控制打分的权重（function 打分结果将会增加 `weight` 倍）以及筛选打分对象（仅经过 `filter` 筛选过的 doc 才会被此 function 打分）
#[derive(Debug, Clone, Default)]
pub struct ScoreFunction {
    pub weight: Option<f32>,
    pub filter: Option<Query>,
    pub field_value_function: Option<FieldValueFactorFunction>,
    pub decay_function: Option<DecayFunction>,
    pub random_function: Option<RandomFunction>,
}

impl ScoreFunction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn filter(mut self, filter: Query) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn field_value_function(mut self, field_value_function: FieldValueFactorFunction) -> Self {
        self.field_value_function = Some(field_value_function);
        self
    }

    pub fn decay_function(mut self, decay_function: DecayFunction) -> Self {
        self.decay_function = Some(decay_function);
        self
    }

    pub fn random_function(mut self, random_function: RandomFunction) -> Self {
        self.random_function = Some(random_function);
        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if let Some(function) = &self.field_value_function {
            function.validate()?;
        }

        if let Some(func) = &self.decay_function {
            func.validate()?;
        }

        Ok(())
    }
}

impl From<ScoreFunction> for crate::protos::search::Function {
    fn from(value: ScoreFunction) -> Self {
        let ScoreFunction {
            weight,
            filter,
            field_value_function,
            decay_function,
            random_function,
        } = value;

        Self {
            weight,
            field_value_factor: field_value_function.map(|f| f.into()),
            random: random_function.map(|f| f.into()),
            decay: decay_function.map(|f| f.into()),
            filter: filter.map(|f| f.into()),
        }
    }
}
