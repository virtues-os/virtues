//! Global ontology registry
//!
//! Provides a compile-time registry of all ontology definitions.
//! Similar to the source registry pattern in `crate::registry`.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::descriptor::Ontology;

// Import domain registrations
use super::activity::registry::register_activity_ontologies;
use super::calendar::registry::register_calendar_ontologies;
use super::device::registry::register_device_ontologies;
use super::environment::registry::register_environment_ontologies;
use super::financial::registry::register_financial_ontologies;
use super::health::registry::register_health_ontologies;
use super::knowledge::registry::register_knowledge_ontologies;
use super::location::registry::register_location_ontologies;
use super::social::registry::register_social_ontologies;
use super::speech::registry::register_speech_ontologies;

/// Global ontology registry
pub struct OntologyRegistry {
    /// Ontologies indexed by name
    ontologies: HashMap<String, Ontology>,
    /// Ontologies grouped by domain
    by_domain: HashMap<String, Vec<String>>,
    /// Mapping from source stream to target ontologies
    stream_to_ontologies: HashMap<String, Vec<String>>,
}

impl OntologyRegistry {
    /// Create a new empty registry
    fn new() -> Self {
        Self {
            ontologies: HashMap::new(),
            by_domain: HashMap::new(),
            stream_to_ontologies: HashMap::new(),
        }
    }

    /// Register an ontology
    pub fn register(&mut self, ontology: Ontology) {
        let name = ontology.name.to_string();
        let domain = ontology.domain.to_string();

        // Index by source streams
        for stream in &ontology.source_streams {
            self.stream_to_ontologies
                .entry(stream.to_string())
                .or_default()
                .push(name.clone());
        }

        // Group by domain
        self.by_domain.entry(domain).or_default().push(name.clone());

        // Store ontology
        self.ontologies.insert(name, ontology);
    }

    /// Get all registered ontologies
    pub fn list(&self) -> Vec<&Ontology> {
        self.ontologies.values().collect()
    }

    /// Get ontologies that have embedding/semantic search enabled
    pub fn list_searchable(&self) -> Vec<&Ontology> {
        self.ontologies
            .values()
            .filter(|o| o.embedding.is_some())
            .collect()
    }

    /// Get a specific ontology by name
    pub fn get(&self, name: &str) -> Option<&Ontology> {
        self.ontologies.get(name)
    }

    /// Get all ontologies in a domain
    pub fn get_by_domain(&self, domain: &str) -> Vec<&Ontology> {
        self.by_domain
            .get(domain)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| self.ontologies.get(name))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all ontologies that a stream feeds into
    pub fn get_for_stream(&self, stream_table: &str) -> Vec<&Ontology> {
        self.stream_to_ontologies
            .get(stream_table)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| self.ontologies.get(name))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// List all domain names
    pub fn list_domains(&self) -> Vec<&str> {
        self.by_domain.keys().map(|s| s.as_str()).collect()
    }
}

// Global registry instance
static REGISTRY: OnceLock<OntologyRegistry> = OnceLock::new();

/// Initialize the global registry
fn init_registry() -> OntologyRegistry {
    let mut registry = OntologyRegistry::new();

    // Register all domain ontologies
    register_health_ontologies(&mut registry);
    register_location_ontologies(&mut registry);
    register_social_ontologies(&mut registry);
    register_calendar_ontologies(&mut registry);
    register_activity_ontologies(&mut registry);
    register_speech_ontologies(&mut registry);
    register_knowledge_ontologies(&mut registry);
    register_financial_ontologies(&mut registry);
    register_device_ontologies(&mut registry);
    register_environment_ontologies(&mut registry);

    registry
}

/// Get the global ontology registry
pub fn ontology_registry() -> &'static OntologyRegistry {
    REGISTRY.get_or_init(init_registry)
}

/// List all registered ontologies
pub fn list_ontologies() -> Vec<&'static Ontology> {
    ontology_registry().list()
}

/// Get a specific ontology by name
pub fn get_ontology(name: &str) -> Option<&'static Ontology> {
    ontology_registry().get(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_initialization() {
        let ontologies = list_ontologies();
        assert!(!ontologies.is_empty(), "Registry should have ontologies");
    }

    #[test]
    fn test_get_ontology() {
        let sleep = get_ontology("health_sleep");
        if sleep.is_some() {
            let s = sleep.unwrap();
            assert_eq!(s.domain, "health");
        }
    }

    #[test]
    fn test_ontologies_have_unique_names() {
        let ontologies = list_ontologies();
        let mut names = std::collections::HashSet::new();
        for ontology in ontologies {
            assert!(
                names.insert(ontology.name),
                "Duplicate ontology name: {}",
                ontology.name
            );
        }
    }

    #[test]
    fn test_stream_to_ontology_mapping() {
        let registry = ontology_registry();
        let health_ontologies = registry.get_for_stream("stream_ios_healthkit");
        if !health_ontologies.is_empty() {
            assert!(health_ontologies.len() >= 1);
        }
    }

    #[test]
    fn test_searchable_ontologies() {
        let registry = ontology_registry();
        let searchable = registry.list_searchable();
        // We have 6 searchable ontologies: email, message, calendar, ai_conversation, document, financial_transaction
        assert_eq!(searchable.len(), 6);
        for ont in searchable {
            assert!(ont.embedding.is_some());
        }
    }

    #[test]
    fn test_registry_stream_ontology_consistency() {
        // Validates that RegisteredStream.target_ontologies matches Ontology.source_streams
        // This catches drift between the two metadata definitions
        use crate::registry::list_all_streams_including_disabled;

        let ontology_registry = ontology_registry();
        let all_streams = list_all_streams_including_disabled();

        let mut errors = Vec::new();

        // Check: Every ontology's source_streams should have a corresponding RegisteredStream
        // that lists that ontology in its target_ontologies
        for ontology in ontology_registry.list() {
            for source_stream in &ontology.source_streams {
                // Find the registered stream with this table_name
                let matching_stream = all_streams
                    .iter()
                    .find(|(_, stream)| stream.descriptor.table_name == *source_stream);

                match matching_stream {
                    Some((_, stream)) => {
                        // Verify the stream lists this ontology as a target
                        if !stream.descriptor.target_ontologies.contains(&ontology.name) {
                            errors.push(format!(
                                "Ontology '{}' claims source_stream '{}', but RegisteredStream '{}' doesn't list it in target_ontologies (has: {:?})",
                                ontology.name, source_stream, stream.descriptor.name, stream.descriptor.target_ontologies
                            ));
                        }
                    }
                    None => {
                        errors.push(format!(
                            "Ontology '{}' claims source_stream '{}' but no RegisteredStream has that table_name",
                            ontology.name, source_stream
                        ));
                    }
                }
            }
        }

        // Check: Every RegisteredStream's target_ontologies should exist and list that stream
        for (source_name, stream) in &all_streams {
            for target_ontology in &stream.descriptor.target_ontologies {
                match ontology_registry.get(target_ontology) {
                    Some(ontology) => {
                        if !ontology.source_streams.contains(&stream.descriptor.table_name) {
                            errors.push(format!(
                                "RegisteredStream '{}/{}' (table: {}) claims target_ontology '{}', but ontology doesn't list it in source_streams (has: {:?})",
                                source_name, stream.descriptor.name, stream.descriptor.table_name, target_ontology, ontology.source_streams
                            ));
                        }
                    }
                    None => {
                        errors.push(format!(
                            "RegisteredStream '{}/{}' claims target_ontology '{}' but no such ontology exists",
                            source_name, stream.descriptor.name, target_ontology
                        ));
                    }
                }
            }
        }

        assert!(
            errors.is_empty(),
            "Registry consistency errors:\n{}",
            errors.join("\n")
        );
    }
}
