//! Consume a temporary host kernel after deriving a read-only application projection.
use crate::simulator::WorldKernel;
use oasis7_simulator::WorldModel;

pub fn into_projection_model(kernel: WorldKernel) -> WorldModel {
    kernel.into_model()
}
