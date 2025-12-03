//! Praxis domain registry

use crate::ontologies::registry::OntologyRegistry;
use crate::ontologies::OntologyDescriptor;

use super::calendar::CalendarOntology;

/// Register all praxis ontologies
pub fn register_praxis_ontologies(registry: &mut OntologyRegistry) {
    registry.register(CalendarOntology::descriptor());
}
