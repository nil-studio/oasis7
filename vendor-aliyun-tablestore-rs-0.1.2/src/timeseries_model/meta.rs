use std::collections::HashMap;

use super::{build_tags_string, parse_tags, TimeseriesKey};

#[derive(Debug, Default, Clone)]
pub struct TimeseriesMeta {
    pub key: TimeseriesKey,
    pub attributes: HashMap<String, String>,
    pub update_time_us: Option<u64>,
}

impl TimeseriesMeta {
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置键的度量名称
    pub fn measurement_name(mut self, measurement: impl Into<String>) -> Self {
        self.key.measurement_name = Some(measurement.into());

        self
    }

    /// 设置键的源
    pub fn datasource(mut self, source: impl Into<String>) -> Self {
        self.key.datasource = Some(source.into());

        self
    }

    /// 给键增加一个标签
    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.key.tags.insert(key.into(), value.into());

        self
    }

    /// 给键设置所有标签
    pub fn tags(mut self, tags: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>) -> Self {
        self.key.tags = tags.into_iter().map(|(k, v)| (k.into(), v.into())).collect();

        self
    }

    /// 增加一个属性
    pub fn attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());

        self
    }

    /// 设置属性
    pub fn attributes(mut self, pairs: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>) -> Self {
        self.attributes = pairs.into_iter().map(|(k, v)| (k.into(), v.into())).collect();

        self
    }

    /// 设置更新时间，微秒
    pub fn update_time_us(mut self, ts_us: u64) -> Self {
        self.update_time_us = Some(ts_us);

        self
    }
}

impl From<TimeseriesMeta> for crate::protos::timeseries::TimeseriesMeta {
    fn from(value: TimeseriesMeta) -> Self {
        let TimeseriesMeta {
            key,
            attributes,
            update_time_us,
        } = value;

        crate::protos::timeseries::TimeseriesMeta {
            time_series_key: crate::protos::timeseries::TimeseriesKey::from(key),
            attributes: Some(build_tags_string(attributes.iter())),
            update_time: update_time_us.map(|ts_us| ts_us as i64),
        }
    }
}

impl From<crate::protos::timeseries::TimeseriesMeta> for TimeseriesMeta {
    fn from(value: crate::protos::timeseries::TimeseriesMeta) -> Self {
        let crate::protos::timeseries::TimeseriesMeta {
            time_series_key,
            attributes,
            update_time,
        } = value;

        Self {
            key: TimeseriesKey::from(time_series_key),
            attributes: if let Some(s) = attributes { parse_tags(&s) } else { HashMap::new() },
            update_time_us: update_time.map(|n| n as u64),
        }
    }
}
