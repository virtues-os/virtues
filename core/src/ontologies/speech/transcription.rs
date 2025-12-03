//! Transcription ontology
//!
//! Audio transcriptions from iOS microphone recordings.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct TranscriptionOntology;

impl OntologyDescriptor for TranscriptionOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("speech_transcription")
            .display_name("Voice Transcriptions")
            .description("Transcribed audio from microphone recordings")
            .domain("speech")
            .table_name("speech_transcription")
            .source_streams(vec!["stream_ios_microphone"])
            .build()
    }
}
