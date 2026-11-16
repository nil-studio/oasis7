//! Compiler-inferred nominal codec witnesses for the original exhaustive
//! field registry. No second field-name/type table or accepted owner is made.
use super::*;
use crate::incremental_semantic_key::SemanticKeyV1;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, BTreeSet};
/// Each callback receives one actual immutable typed field borrow. A consumer
/// may register closed member converters; callbacks never select field roots,
/// alter the field registry, or confer a qualification on decoded DATA.
pub trait WorldStateColumnCodecVisitorV1 {
    type Error;
    fn value<T: Serialize + DeserializeOwned>(
        &mut self,
        field: FieldDescriptor,
        value: &T,
    ) -> Result<(), Self::Error>;
    fn map<K: SemanticKeyV1 + DeserializeOwned + Ord, V: Serialize + DeserializeOwned>(
        &mut self,
        field: FieldDescriptor,
        value: &BTreeMap<K, V>,
    ) -> Result<(), Self::Error>;
    fn nested<
        K: SemanticKeyV1 + DeserializeOwned + Ord,
        J: SemanticKeyV1 + DeserializeOwned + Ord,
        V: Serialize + DeserializeOwned,
    >(
        &mut self,
        field: FieldDescriptor,
        value: &BTreeMap<K, BTreeMap<J, V>>,
    ) -> Result<(), Self::Error>;
    fn set<K: SemanticKeyV1 + DeserializeOwned + Ord>(
        &mut self,
        field: FieldDescriptor,
        value: &BTreeSet<K>,
    ) -> Result<(), Self::Error>;
    fn mapset<
        K: SemanticKeyV1 + DeserializeOwned + Ord,
        V: SemanticKeyV1 + DeserializeOwned + Ord,
    >(
        &mut self,
        field: FieldDescriptor,
        value: &BTreeMap<K, BTreeSet<V>>,
    ) -> Result<(), Self::Error>;
    fn bytes<K: SemanticKeyV1 + DeserializeOwned + Ord>(
        &mut self,
        field: FieldDescriptor,
        value: &BTreeMap<K, Vec<u8>>,
    ) -> Result<(), Self::Error>;
    fn ordered<V: Serialize + DeserializeOwned>(
        &mut self,
        field: FieldDescriptor,
        value: &Vec<V>,
    ) -> Result<(), Self::Error>;
    fn side<T: Serialize + DeserializeOwned>(
        &mut self,
        field: FieldDescriptor,
        value: &T,
    ) -> Result<(), Self::Error> {
        self.value(field, value)
    }
    fn agents(
        &mut self,
        field: FieldDescriptor,
        value: &BTreeMap<String, crate::AgentCell>,
    ) -> Result<(), Self::Error> {
        self.map(field, value)
    }
    fn module_instances(
        &mut self,
        field: FieldDescriptor,
        value: &BTreeMap<String, crate::ModuleInstanceState>,
    ) -> Result<(), Self::Error> {
        self.map(field, value)
    }
}
