#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeseriesFieldType {
    Long = 1,
    Boolean = 2,
    Double = 3,
    String = 4,
    Binary = 5,
}

#[derive(Debug, Clone)]
pub struct TimeseriesFieldToGet {
    pub name: String,
    pub field_type: TimeseriesFieldType,
}

impl TimeseriesFieldToGet {
    pub fn new(name: &str, field_type: TimeseriesFieldType) -> Self {
        Self {
            name: name.to_string(),
            field_type,
        }
    }
}

impl From<TimeseriesFieldToGet> for crate::protos::timeseries::TimeseriesFieldsToGet {
    fn from(value: TimeseriesFieldToGet) -> Self {
        Self {
            name: Some(value.name),
            r#type: Some(value.field_type as i32),
        }
    }
}
