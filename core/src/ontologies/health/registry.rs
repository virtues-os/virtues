//! Health domain registry

use crate::ontologies::registry::OntologyRegistry;
use crate::ontologies::OntologyDescriptor;

use super::heart_rate::HeartRateOntology;
use super::hrv::HrvOntology;
use super::sleep::SleepOntology;
use super::steps::StepsOntology;
use super::workout::WorkoutOntology;

/// Register all health ontologies
pub fn register_health_ontologies(registry: &mut OntologyRegistry) {
    registry.register(SleepOntology::descriptor());
    registry.register(HeartRateOntology::descriptor());
    registry.register(HrvOntology::descriptor());
    registry.register(StepsOntology::descriptor());
    registry.register(WorkoutOntology::descriptor());
}
