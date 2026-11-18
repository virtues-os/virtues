//! Keep recordings (Settings · Your data): deleting audio the owner no longer
//! wants kept.
//!
//! `app_user_profile.keep_recordings_days` is NULL (forever, the default) or
//! 30, 90 or 365. The maintenance sweeper calls [`delete_expired_audio`]; it
//! deletes the audio file of each recording older than that, but only once
//! the recording is transcribed (or was silent), so no speech is lost before
//! its words are kept. The transcript, its summary and everything written
//! from it stay. Afterwards `audio_url` is cleared on the recording and its
//! transcript, so nothing points at a file that is gone.
//!
//! Copies already written to a backup drive stay there: backups are append
//! only and the server cannot reach into them to delete.

use std::path::{Component, Path, PathBuf};

use sqlx::PgPool;

use crate::error::{Error, Result};

/// How many recordings one sweep handles, so a first run over a long record
/// spreads across ticks instead of holding the database.
const BATCH: i64 = 200;

/// The choices Settings offers, in days.
pub const CHOICES: &[i32] = &[30, 90, 365];

/// A lake-relative path as a path under `root`, or None when it would leave
/// the lake (an absolute path or a `..`), which is never a file to delete.
fn inside(root: &Path, relative: &str) -> Option<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(root.join(rel))
}

/// Deletes the audio of transcribed recordings older than the owner's choice.
/// Returns how many recordings lost their audio. Does nothing when the owner
/// keeps recordings forever.
pub async fn delete_expired_audio(pool: &PgPool) -> Result<u64> {
    let days: Option<i32> = sqlx::query_scalar("SELECT keep_recordings_days FROM app_user_profile LIMIT 1")
        .fetch_one(pool)
        .await?;
    let Some(days) = days else { return Ok(0) };

    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT r.id, r.source_stream_id, r.audio_url FROM data_audio_recording r \
         WHERE r.audio_url IS NOT NULL \
           AND r.started_at < now() - make_interval(days => $1) \
           AND (r.is_silent OR EXISTS (SELECT 1 FROM data_communication_transcription t \
                                       WHERE t.source_stream_id = r.source_stream_id)) \
         ORDER BY r.started_at LIMIT $2",
    )
    .bind(days)
    .bind(BATCH)
    .fetch_all(pool)
    .await?;

    let root = crate::storage::lake::lake_root();
    let mut deleted = 0u64;
    for (id, stream_id, audio_url) in rows {
        let Some(path) = inside(&root, &audio_url) else {
            tracing::warn!(recording = %id, "keep recordings: audio path leaves the lake; left alone");
            continue;
        };
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            // Already gone: still clear the path, so it is not tried again.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(recording = %id, error = %e, "keep recordings: couldn't delete audio; will try again");
                continue;
            }
        }
        sqlx::query(
            "UPDATE data_audio_recording SET audio_url = NULL, \
             metadata = metadata || jsonb_build_object('audio_deleted_at', now()), updated_at = now() \
             WHERE id = $1",
        )
        .bind(&id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("keep recordings: clearing audio of {id}: {e}")))?;
        sqlx::query("UPDATE data_communication_transcription SET audio_url = NULL WHERE source_stream_id = $1")
            .bind(&stream_id)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("keep recordings: clearing transcript audio of {id}: {e}")))?;
        deleted += 1;
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_paths_inside_the_lake_are_deleted() {
        let root = Path::new("/var/lib/virtues/lake");
        assert_eq!(inside(root, "audio/2026/a.m4a"), Some(root.join("audio/2026/a.m4a")));
        assert_eq!(inside(root, "../env/virtues.env"), None);
        assert_eq!(inside(root, "/etc/passwd"), None);
        assert_eq!(inside(root, "audio/../../x"), None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn forever_deletes_nothing_and_untranscribed_audio_is_kept(pool: PgPool) {
        sqlx::query(
            "INSERT INTO data_audio_recording (id, source_stream_id, started_at, audio_url, source_table, source_provider) \
             VALUES ('r1', 's1', now() - interval '400 days', 'audio/test-missing.m4a', 'microphone', 'ios')",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(delete_expired_audio(&pool).await.unwrap(), 0, "forever is the default");

        sqlx::query("UPDATE app_user_profile SET keep_recordings_days = 30").execute(&pool).await.unwrap();
        assert_eq!(delete_expired_audio(&pool).await.unwrap(), 0, "no transcript yet, so the audio stays");
        let still: Option<String> =
            sqlx::query_scalar("SELECT audio_url FROM data_audio_recording WHERE id = 'r1'").fetch_one(&pool).await.unwrap();
        assert!(still.is_some());

        // Transcribed: the audio goes (the file is already missing here), the
        // transcript stays, and neither points at the file any more.
        sqlx::query(
            "INSERT INTO data_communication_transcription (id, text, started_at, source_stream_id, source_table, source_provider, audio_url) \
             VALUES ('t1', 'Invented words.', now() - interval '400 days', 's1', 'microphone', 'ios', 'audio/test-missing.m4a')",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(delete_expired_audio(&pool).await.unwrap(), 1);
        let (rec, tr): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT (SELECT audio_url FROM data_audio_recording WHERE id = 'r1'), \
                    (SELECT audio_url FROM data_communication_transcription WHERE id = 't1')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((rec, tr), (None, None));
        let text: String =
            sqlx::query_scalar("SELECT text FROM data_communication_transcription WHERE id = 't1'").fetch_one(&pool).await.unwrap();
        assert_eq!(text, "Invented words.");
    }
}
