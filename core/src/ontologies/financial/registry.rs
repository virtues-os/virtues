//! Financial domain registry

use crate::ontologies::registry::OntologyRegistry;
use crate::ontologies::OntologyDescriptor;

use super::account::FinancialAccountOntology;
use super::transaction::FinancialTransactionOntology;

/// Register all financial ontologies
pub fn register_financial_ontologies(registry: &mut OntologyRegistry) {
    registry.register(FinancialAccountOntology::descriptor());
    registry.register(FinancialTransactionOntology::descriptor());
}
