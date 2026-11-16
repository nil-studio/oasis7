//! Same immutable GEN State supplies the two dedicated logical entity maps.
//! The physical GEN visitor intentionally emits their original scalar fields;
//! this DATA capture preserves the original complete AgentCell/instance row.
use super::*;
use oasis7_world_state::incremental_projection::WorldStateColumnCodecVisitorV1;
use serde::{Serialize, de::DeserializeOwned};
struct Dedicated<'a, 'b, 'c>(&'a mut NormalProjectionColumnInitializerV1<'b, 'c>);
impl Dedicated<'_, '_, '_> {
    fn skip(&mut self) -> Result<(), BodyMutationError> {
        self.0.common.consume_steps(1).map_err(paid)
    }
}
impl WorldStateColumnCodecVisitorV1 for Dedicated<'_, '_, '_> {
    type Error = BodyMutationError;
    fn value<T: Serialize + DeserializeOwned>(
        &mut self,
        _: FieldDescriptor,
        _: &T,
    ) -> Result<(), Self::Error> {
        self.skip()
    }
    fn map<K: SemanticKeyV1 + DeserializeOwned + Ord, V: Serialize + DeserializeOwned>(
        &mut self,
        _: FieldDescriptor,
        _: &BTreeMap<K, V>,
    ) -> Result<(), Self::Error> {
        self.skip()
    }
    fn nested<
        K: SemanticKeyV1 + DeserializeOwned + Ord,
        J: SemanticKeyV1 + DeserializeOwned + Ord,
        V: Serialize + DeserializeOwned,
    >(
        &mut self,
        _: FieldDescriptor,
        _: &BTreeMap<K, BTreeMap<J, V>>,
    ) -> Result<(), Self::Error> {
        self.skip()
    }
    fn set<K: SemanticKeyV1 + DeserializeOwned + Ord>(
        &mut self,
        _: FieldDescriptor,
        _: &BTreeSet<K>,
    ) -> Result<(), Self::Error> {
        self.skip()
    }
    fn mapset<
        K: SemanticKeyV1 + DeserializeOwned + Ord,
        V: SemanticKeyV1 + DeserializeOwned + Ord,
    >(
        &mut self,
        _: FieldDescriptor,
        _: &BTreeMap<K, BTreeSet<V>>,
    ) -> Result<(), Self::Error> {
        self.skip()
    }
    fn bytes<K: SemanticKeyV1 + DeserializeOwned + Ord>(
        &mut self,
        _: FieldDescriptor,
        _: &BTreeMap<K, Vec<u8>>,
    ) -> Result<(), Self::Error> {
        self.skip()
    }
    fn ordered<V: Serialize + DeserializeOwned>(
        &mut self,
        _: FieldDescriptor,
        _: &[V],
    ) -> Result<(), Self::Error> {
        self.skip()
    }
    fn agents(
        &mut self,
        field: FieldDescriptor,
        value: &BTreeMap<String, oasis7_world_state::AgentCell>,
    ) -> Result<(), Self::Error> {
        self.0.begin(field, Some(value.len()))?;
        for (key, value) in value {
            self.0.row(field, key, value)?;
            self.0.common.consume_steps(1).map_err(paid)?;
            let intent = super::capture(self.0.common, |out| {
                serde_json::to_writer(out, &value.intent).map_err(paid)
            })?;
            let row = self
                .0
                .fields
                .get_mut(&(field.owner, field.name))
                .and_then(|field| field.rows.last_mut())
                .ok_or_else(|| invalid("GEN agent projection row"))?;
            row.agent_electricity = Some(
                value
                    .state
                    .resources
                    .get(oasis7_agent_api::world_action::ResourceKind::Electricity),
            );
            row.agent_intent = Some(intent);
        }
        self.0.end(field)
    }
    fn module_instances(
        &mut self,
        field: FieldDescriptor,
        value: &BTreeMap<String, oasis7_world_state::ModuleInstanceState>,
    ) -> Result<(), Self::Error> {
        self.0.begin(field, Some(value.len()))?;
        for (key, value) in value {
            self.0.row(field, key, value)?
        }
        self.0.end(field)
    }
}
impl NormalProjectionColumnInitializerV1<'_, '_> {
    pub(super) fn capture_dedicated_parent_columns_v1(
        &mut self,
        state: &oasis7_world_state::WorldState,
    ) -> Result<(), BodyMutationError> {
        state.visit_iws_column_codecs_v1(&mut Dedicated(self))
    }
}
