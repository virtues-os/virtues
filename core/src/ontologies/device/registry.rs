//! Device ontology registration

use super::battery::BatteryOntology;
use crate::ontologies::registry::OntologyRegistry;
use crate::ontologies::OntologyDescriptor;

pub fn register_device_ontologies(registry: &mut OntologyRegistry) {
    registry.register(BatteryOntology::descriptor());
}
