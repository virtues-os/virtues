//! Calendar domain registry

use crate::ontologies::registry::OntologyRegistry;
use crate::ontologies::OntologyDescriptor;

use super::event::CalendarEventOntology;

/// Register all calendar ontologies
pub fn register_calendar_ontologies(registry: &mut OntologyRegistry) {
    registry.register(CalendarEventOntology::descriptor());
}
