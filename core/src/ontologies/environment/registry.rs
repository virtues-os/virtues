//! Environment ontology registration

use super::pressure::PressureOntology;
use crate::ontologies::registry::OntologyRegistry;
use crate::ontologies::OntologyDescriptor;

pub fn register_environment_ontologies(registry: &mut OntologyRegistry) {
    registry.register(PressureOntology::descriptor());
}
