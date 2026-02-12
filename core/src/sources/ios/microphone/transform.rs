//! iOS Microphone → Speech Transcription transform
//!
//! Downloads audio chunks from MinIO, sends them to Gemini 2.5 Flash-Lite
//! via Tollbooth for transcription + entity extraction, and inserts
//! structured results into data_communication_transcription.

use async_trait::async_trait;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::database::Database;
use crate::error::{Error, Result};
use crate::http_client;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformResult};
use crate::tollbooth;

/// Standard tier: Gemini 2.5 Flash-Lite (cost-optimized, ~$3/month)
const MODEL_STANDARD: &str = "google/gemini-2.5-flash-lite";
/// Pro tier: Gemini 2.5 Flash (higher accuracy on noisy audio, ~$10/month)
const _MODEL_PRO: &str = "google/gemini-2.5-flash";

/// System prompt instructing Gemini to transcribe and extract entities
const SYSTEM_PROMPT: &str = r#"You are a verbatim audio transcription system. Output ONLY a raw JSON object — no markdown, no code fences, no explanation.

Schema:
{"title":"string max 10 words","summary":"string 1-2 sentences","text":"string verbatim transcript","language":"string ISO 639-1","confidence":0.0-1.0,"speaker_count":integer,"tags":["max 5 strings"],"entities":{"people":[],"places":[],"organizations":[]}}

Rules:
- text: Exact words spoken. No paraphrasing. Include filler words (um, uh, ah). Use "[Speaker 1]:", "[Speaker 2]:" if multiple speakers.
- entities: Only extract names explicitly spoken. Use "[unclear]" if a name is ambiguous.
- confidence: 0.0 for silence/unintelligible, 0.5+ for partial, 0.9+ for clear speech.
- tags: 1-5 topic labels maximum.
- Silence/noise: Return {"title":"Silence","summary":"No speech detected","text":"","language":"en","confidence":0.0,"speaker_count":0,"tags":[],"entities":{"people":[],"places":[],"organizations":[]}}
"#;

/// Parsed response from Gemini transcription
#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    title: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    text: String,
    language: Option<String>,
    confidence: Option<f64>,
    speaker_count: Option<i32>,
    tags: Option<Vec<String>>,
    entities: Option<serde_json::Value>,
}

/// Transform that sends iOS microphone audio to Gemini for transcription
pub struct IosMicrophoneTransform {
    tollbooth_url: String,
    tollbooth_secret: String,
    http_client: reqwest::Client,
    model: String,
}

impl IosMicrophoneTransform {
    /// Create with standard tier model (Flash-Lite)
    pub fn from_env() -> Result<Self> {
        Self::from_env_with_model(MODEL_STANDARD)
    }

    /// Create with a specific model (for tier-based selection)
    pub fn from_env_with_model(model: &str) -> Result<Self> {
        let tollbooth_url =
            std::env::var("TOLLBOOTH_URL").unwrap_or_else(|_| "http://localhost:9002".to_string());
        let tollbooth_secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
            .map_err(|_| Error::Configuration("TOLLBOOTH_INTERNAL_SECRET not set".to_string()))?;
        tollbooth::validate_secret(&tollbooth_secret)?;

        // Use streaming client (300s timeout) since Gemini audio processing can take 30-90s
        let http_client = http_client::tollbooth_streaming_client();

        Ok(Self {
            tollbooth_url,
            tollbooth_secret,
            http_client,
            model: model.to_string(),
        })
    }

    /// Map audio file extension to MIME type for data URL
    fn audio_mime_type(format: &str) -> &'static str {
        match format {
            "m4a" | "mp4" | "aac" => "audio/mp4",
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "ogg" => "audio/ogg",
            "flac" => "audio/flac",
            _ => "audio/mp4",
        }
    }

    /// Call Gemini via Tollbooth to transcribe audio
    async fn transcribe_audio(
        &self,
        audio_b64: &str,
        audio_format: &str,
    ) -> Result<TranscriptionResponse> {
        let mime_type = Self::audio_mime_type(audio_format);
        let request_body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": SYSTEM_PROMPT
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:{mime_type};base64,{audio_b64}")
                            }
                        },
                        {
                            "type": "text",
                            "text": "Transcribe this audio recording and extract structured data."
                        }
                    ]
                }
            ],
            "max_tokens": 8192,
            "temperature": 0.0,
            "response_format": { "type": "json_object" }
        });

        let url = format!("{}/v1/chat/completions", self.tollbooth_url);
        let response =
            tollbooth::with_system_auth(self.http_client.post(&url), &self.tollbooth_secret)
                .header("Content-Type", "application/json")
                .json(&request_body)
                .send()
                .await
                .map_err(|e| Error::Network(format!("Tollbooth request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(Error::ExternalApi(
                "Rate limited by Tollbooth (429)".to_string(),
            ));
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::ExternalApi(format!(
                "Tollbooth returned {status}: {body}"
            )));
        }

        let resp_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| Error::ExternalApi(format!("Failed to parse Tollbooth response: {e}")))?;

        // Extract the content string from choices[0].message.content
        let content_str = resp_json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                Error::ExternalApi("Missing choices[0].message.content in response".to_string())
            })?;

        // Strip markdown code fencing if present (Gemini sometimes wraps in ```json ... ```)
        let json_str = content_str.trim();
        let json_str = if json_str.starts_with("```") {
            let stripped = json_str
                .strip_prefix("```json")
                .or_else(|| json_str.strip_prefix("```"))
                .unwrap_or(json_str);
            stripped.strip_suffix("```").unwrap_or(stripped).trim()
        } else {
            json_str
        };

        // Parse the JSON content string into our response struct
        let transcription: TranscriptionResponse = serde_json::from_str(json_str).map_err(|e| {
            Error::ExternalApi(format!(
                "Failed to parse Gemini JSON output: {e}. Raw: {}",
                &json_str[..json_str.len().min(200)]
            ))
        })?;

        Ok(transcription)
    }
}

#[async_trait]
impl OntologyTransform for IosMicrophoneTransform {
    fn source_table(&self) -> &str {
        "stream_ios_microphone"
    }

    fn target_table(&self) -> &str {
        "communication_transcription"
    }

    fn domain(&self) -> &str {
        "communication"
    }

    #[tracing::instrument(skip(self, db, context), fields(source_table = %self.source_table(), target_table = %self.target_table()))]
    async fn transform(
        &self,
        db: &Database,
        context: &TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut records_failed = 0;
        let mut last_processed_id: Option<String> = None;

        let data_source = context
            .get_data_source()
            .ok_or_else(|| Error::Other("No data source available for transform".to_string()))?;

        let checkpoint_key = "ios_microphone_to_communication_transcription";
        let batches = data_source
            .read_with_checkpoint(&source_id, "microphone", checkpoint_key)
            .await?;

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                let stream_id = record
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| Uuid::new_v4().to_string());

                last_processed_id = Some(stream_id.clone());

                // Skip records without uploaded audio
                let audio_key = match record
                    .get("uploaded_audio_file_key")
                    .and_then(|v| v.as_str())
                {
                    Some(key) => key.to_string(),
                    None => {
                        tracing::debug!(stream_id = %stream_id, "No audio file key, skipping");
                        continue;
                    }
                };

                // Extract timestamps from source record
                let start_time = record
                    .get("timestamp_start")
                    .or_else(|| record.get("timestamp"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                    .unwrap_or_else(Utc::now);

                let end_time = record
                    .get("timestamp_end")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<DateTime<Utc>>().ok());

                let duration_seconds = record.get("duration_seconds").and_then(|v| v.as_f64());

                // Extract audio format from the storage key extension (e.g. "ios/microphone/.../uuid.m4a")
                let audio_format = audio_key.rsplit('.').next().unwrap_or("m4a");

                // Download audio from MinIO
                let audio_bytes = match context.storage.download(&audio_key).await {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        tracing::warn!(stream_id = %stream_id, error = %e, "Failed to download audio, skipping");
                        records_failed += 1;
                        continue;
                    }
                };

                // Base64 encode for Gemini
                let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);

                // Call Gemini via Tollbooth
                let transcription = match self.transcribe_audio(&audio_b64, audio_format).await {
                    Ok(t) => t,
                    Err(Error::ExternalApi(msg)) if msg.contains("429") => {
                        tracing::warn!("Rate limited, stopping transform early to retry later");
                        return Ok(TransformResult {
                            records_read,
                            records_written,
                            records_failed,
                            last_processed_id,
                            chained_transforms: vec![],
                        });
                    }
                    Err(e) => {
                        tracing::warn!(stream_id = %stream_id, error = %e, "Transcription failed, skipping");
                        records_failed += 1;
                        continue;
                    }
                };

                // Insert into ontology table
                let id = Uuid::new_v4().to_string();
                let tags_json = transcription
                    .tags
                    .map(|t| serde_json::to_string(&t).unwrap_or_else(|_| "[]".to_string()))
                    .unwrap_or_else(|| "[]".to_string());
                let entities_json = transcription
                    .entities
                    .map(|e| serde_json::to_string(&e).unwrap_or_else(|_| "{}".to_string()))
                    .unwrap_or_else(|| "{}".to_string());

                let result = sqlx::query(
                    r#"INSERT INTO data_communication_transcription (
                        id, audio_url, text, title, summary, language,
                        duration_seconds, start_time, end_time,
                        speaker_count, confidence, tags, entities,
                        source_stream_id, source_table, source_provider, metadata
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6,
                        $7, $8, $9,
                        $10, $11, $12, $13,
                        $14, $15, $16, $17
                    ) ON CONFLICT (source_stream_id) DO NOTHING"#,
                )
                .bind(&id)
                .bind(&audio_key)
                .bind(&transcription.text)
                .bind(&transcription.title)
                .bind(&transcription.summary)
                .bind(&transcription.language)
                .bind(duration_seconds)
                .bind(start_time.to_rfc3339())
                .bind(end_time.map(|t| t.to_rfc3339()))
                .bind(transcription.speaker_count)
                .bind(transcription.confidence)
                .bind(&tags_json)
                .bind(&entities_json)
                .bind(&stream_id)
                .bind("stream_ios_microphone")
                .bind("ios")
                .bind("{}")
                .execute(db.pool())
                .await;

                match result {
                    Ok(r) => {
                        if r.rows_affected() > 0 {
                            records_written += 1;
                            tracing::debug!(
                                stream_id = %stream_id,
                                title = ?transcription.title,
                                confidence = ?transcription.confidence,
                                "Transcription saved"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(stream_id = %stream_id, error = %e, "Failed to insert transcription");
                        records_failed += 1;
                    }
                }
            }

            // Update checkpoint after each batch
            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "microphone", checkpoint_key, max_ts)
                    .await?;
            }
        }

        tracing::info!(
            records_read = records_read,
            records_written = records_written,
            records_failed = records_failed,
            "Microphone transcription transform complete"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

// Self-registration via inventory
struct IosMicrophoneTransformRegistration;
impl crate::sources::base::TransformRegistration for IosMicrophoneTransformRegistration {
    fn source_table(&self) -> &'static str {
        "stream_ios_microphone"
    }
    fn target_table(&self) -> &'static str {
        "communication_transcription"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(IosMicrophoneTransform::from_env()?))
    }
}
inventory::submit! { &IosMicrophoneTransformRegistration as &dyn crate::sources::base::TransformRegistration }
