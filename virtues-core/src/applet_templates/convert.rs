//! Converting a `manifest.toml` folder into a `SKILL.md` folder, keeping what
//! the applet does.
//!
//! The applet keeps its id, so its rows, runs, memory, state and tables stay
//! where they are: the folder's name takes hyphens for underscores, and the
//! id derived from it turns them back (`skill::id_for_dir`). An id the old
//! reader gave with hyphens in it (an import's folder) is pinned as
//! `metadata.id`. What changes:
//!
//! - `name` and `description` are the spec's; the prompt is the body.
//! - `schedule` and a `webhook` trigger become `trigger`; `until = "once"`
//!   becomes `on <the next time it fires>`; a SQL `until` is dropped, as the
//!   plan decided.
//! - `tool` in `triggers` becomes `invoke: assistant`, and `invoke` is always
//!   written, so a converted prompt applet does not appear in the `/` menu.
//! - `command` becomes `run`: a binary by name, or a script moved under
//!   `scripts/`. A `condition` with no command becomes `run: sql select 1
//!   where (…)`, which skips the run when it is false.
//! - `[config.limits]` becomes `limit`, `model_slot` becomes `model`, and a
//!   prompt with no `model_slot` keeps the background model as `model: lite`.
//!   `default_enabled = false` becomes `enabled: false`. `writes`, `secrets`
//!   and `source` keep their meaning.
//! - `schema/` becomes `tables/`; a `face/` holding only `index.html`
//!   becomes `view.html`. A face with more files stays `face/`.
//! - `forked_from` becomes `metadata.forked-from`.
//! - Dropped, with a note: `[config.delivers]` and other `[config]` keys.
//!   Dropped silently: `needs_vault_key` (a kernel list now) and `owner`
//!   (derived from where the folder is).
//!
//! The design is slice 4 of agents/plan/applet-shape.md. The catalog reads
//! only `SKILL.md`; this module is the one reader of `manifest.toml` left.

use std::path::Path;

use super::skill::SkillMd;
use super::Template;

/// What a conversion did: the folder's new path relative to its root, and
/// what it changed or dropped that a person might want to know.
#[derive(Debug)]
pub(crate) struct Converted {
    pub dir: String,
    pub notes: Vec<String>,
}

/// The folder a manifest folder becomes: underscores to hyphens in its last
/// segment.
pub(crate) fn new_dir(dir: &str) -> String {
    match dir.rsplit_once('/') {
        Some((parent, name)) => format!("{parent}/{}", name.replace('_', "-")),
        None => dir.replace('_', "-"),
    }
}

/// The `SKILL.md` text a parsed manifest becomes, and the notes. `next_once`
/// answers when a schedule next fires, for `until = "once"`. `folder` is the
/// manifest folder, to find a script to move.
pub(crate) fn skill_md_for(
    t: &Template,
    name: &str,
    folder: &Path,
    next_once: &dyn Fn(&str) -> Option<String>,
) -> Result<(String, Vec<String>, Option<(String, String)>), String> {
    let mut notes = Vec::new();
    let mut triggers: Vec<String> = Vec::new();
    let mut moved_script: Option<(String, String)> = None;

    // When it starts.
    match (t.schedule.as_deref(), t.until.as_deref().map(str::trim)) {
        (Some(cron), Some(u)) if u.eq_ignore_ascii_case("once") => match next_once(cron) {
            Some(on) => triggers.push(on),
            None => {
                notes.push(format!("`until = \"once\"` was dropped: `{cron}` has no next time to run once."));
                triggers.push(format!("cron {cron}"));
            }
        },
        (Some(cron), until) => {
            triggers.push(format!("cron {cron}"));
            if let Some(u) = until.filter(|u| !u.is_empty()) {
                notes.push(format!("`until = \"{u}\"` was dropped: an applet no longer ends on a SQL check."));
            }
        }
        (None, Some(u)) if !u.is_empty() && !u.eq_ignore_ascii_case("once") => {
            notes.push(format!("`until = \"{u}\"` was dropped: an applet no longer ends on a SQL check."));
        }
        _ => {}
    }
    if t.triggers.iter().any(|t| t == "webhook") {
        triggers.push("webhook".into());
    }
    if t.triggers.iter().any(|t| t == "api") {
        notes.push("The `api` trigger was dropped; it never fired.".into());
    }

    // What runs.
    let mut body = t.agent.clone().unwrap_or_default();
    let mut run = None;
    if let Some(command) = t.command.as_ref().filter(|c| !c.is_empty()) {
        run = Some(match command.as_slice() {
            [bin] if !bin.contains('/') => bin.clone(),
            [interp, file] if matches!(interp.as_str(), "python3" | "sh" | "bash") => {
                let file = file.trim_start_matches("./");
                let ext = Path::new(file).extension().and_then(|e| e.to_str());
                let fits = match interp.as_str() {
                    "python3" => ext == Some("py"),
                    _ => ext == Some("sh"),
                };
                if !fits || file.contains("..") || Path::new(file).is_absolute() || !folder.join(file).is_file() {
                    return Err(format!("`command` {command:?} does not name a script in the folder with its interpreter's extension"));
                }
                let to = if file.starts_with("scripts/") { file.to_string() } else { format!("scripts/{file}") };
                if to != file {
                    moved_script = Some((file.to_string(), to.clone()));
                }
                to
            }
            other => return Err(format!("`command` {other:?} has no `run` form; convert it by hand")),
        });
    }
    if let Some(condition) = t.condition.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        if run.is_none() {
            run = Some(format!("sql select 1 where ({condition})"));
        } else if !body.is_empty() {
            body = format!(
                "First, check this with sql_query, and if it is false, stop and say there was nothing to do: {condition}\n\n{body}"
            );
            notes.push("Its `condition` became the prompt's first step, since it also runs code.".into());
        } else {
            return Err("it has both `command` and `condition` and no prompt to carry the condition".into());
        }
    }

    // Who starts it by hand.
    let invoke = if t.triggers.iter().any(|t| t == "tool") { vec!["assistant".into()] } else { vec!["none".into()] };

    // Ceilings, model, the rest of config.
    let config = t.config.as_ref().and_then(|c| serde_json::to_value(c).ok()).unwrap_or_default();
    let limit = config.get("limits").and_then(|l| l.as_object()).and_then(super::skill::limit_from_config);
    let slot = config.get("model_slot").and_then(|m| m.as_str());
    let has_prompt = !body.trim().is_empty();
    if let Some(obj) = config.as_object() {
        for key in obj.keys().filter(|k| !matches!(k.as_str(), "limits" | "model_slot")) {
            notes.push(format!("`[config.{key}]` was dropped."));
        }
    }

    let md = SkillMd {
        name: name.to_string(),
        description: t.description.clone().filter(|d| !d.trim().is_empty()).unwrap_or_else(|| t.name.clone()),
        invoke,
        trigger: (!triggers.is_empty()).then(|| triggers.join(", ")),
        run,
        source: t.source.as_ref().map(|s| s.id.clone()),
        limit,
        deep: slot == Some("deep"),
        lite: has_prompt && slot.is_none_or(|s| s == "lite"),
        enabled: t.default_enabled,
        writes: t.writes.clone(),
        secrets: t.secrets.clone(),
        forked_from: t.forked_from.clone(),
        id: t.id_prefix.clone().filter(|id| *id != super::skill::id_for_dir(&new_dir(&t.dir))),
        body,
        ..Default::default()
    };
    Ok((super::skill::render(&md)?, notes, moved_script))
}

/// Convert the manifest folder at `root/dir` in place: write the `SKILL.md`
/// into a copy, move its files to their new names, check that the copy
/// loads, and only then replace the old folder with it. A folder that cannot
/// convert is left as it was.
pub(crate) fn convert_folder(
    root: &Path,
    dir: &str,
    shipped: bool,
    next_once: &dyn Fn(&str) -> Option<String>,
) -> Result<Converted, String> {
    let old = root.join(dir);
    let manifest = old.join("manifest.toml");
    let t = super::parse_template(&manifest, dir).ok_or("its manifest.toml does not parse")?;
    let new_dir = new_dir(dir);
    let name = new_dir.rsplit('/').next().unwrap_or(&new_dir).to_string();
    let new = root.join(&new_dir);
    if new_dir != dir && new.exists() {
        return Err(format!("{} already exists", new.display()));
    }
    let (text, notes, moved_script) = skill_md_for(&t, &name, &old, next_once)?;

    let key = dir.replace('/', "__");
    let staging = root.join(format!("{STAGING_PREFIX}{key}"));
    let backup = root.join(format!("{BACKUP_PREFIX}{key}"));
    let journal = root.join(format!("{JOURNAL_PREFIX}{key}"));
    let io = |what: &str, e: std::io::Error| format!("could not {what}: {e}");
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|e| io("clear an earlier conversion's copy", e))?;
    }
    if let Err(e) = copy_whole(&old, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(io("copy the folder", e));
    }
    std::fs::remove_file(staging.join("manifest.toml")).map_err(|e| io("remove the manifest", e))?;
    if let Some((from, to)) = &moved_script {
        let to = staging.join(to);
        std::fs::create_dir_all(to.parent().expect("under scripts/")).map_err(|e| io("create scripts/", e))?;
        std::fs::rename(staging.join(from), &to).map_err(|e| io("move the script", e))?;
    }
    if staging.join("schema").is_dir() && !staging.join("tables").exists() {
        std::fs::rename(staging.join("schema"), staging.join("tables")).map_err(|e| io("rename schema/", e))?;
    }
    let face = staging.join("face");
    let single_page = std::fs::read_dir(&face)
        .map(|entries| entries.flatten().map(|e| e.file_name()).collect::<Vec<_>>() == ["index.html"])
        .unwrap_or(false);
    if single_page {
        std::fs::rename(face.join("index.html"), staging.join("view.html")).map_err(|e| io("move the face", e))?;
        std::fs::remove_dir(&face).map_err(|e| io("remove face/", e))?;
    }
    std::fs::write(staging.join("SKILL.md"), &text).map_err(|e| io("write SKILL.md", e))?;

    // It must load where it is going, or it does not replace anything.
    if let Err(problems) = super::skill::parse(&text, &staging, &new_dir, shipped) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("the converted SKILL.md does not load: {}", problems.join(" ")));
    }
    sync_tree(&staging).map_err(|e| io("write the converted folder to disk", e))?;

    // The swap is two renames, journaled, so a crash at any point leaves
    // either the old folder or the converted one, and `recover` at the next
    // start finishes or undoes it. The old folder is deleted only after the
    // converted one is in place.
    std::fs::write(&journal, format!("{dir}\n{new_dir}\n")).map_err(|e| io("write the conversion journal", e))?;
    sync_file(&journal).map_err(|e| io("write the conversion journal", e))?;
    sync_dir(root).map_err(|e| io("write the conversion journal", e))?;
    std::fs::rename(&old, &backup).map_err(|e| io("move the old folder aside", e))?;
    if let Err(e) = std::fs::rename(&staging, &new) {
        let _ = std::fs::rename(&backup, &old);
        return Err(io("move the converted folder into place", e));
    }
    sync_dir(root).map_err(|e| io("move the converted folder into place", e))?;
    if let Some(parent) = new.parent() {
        let _ = sync_dir(parent);
    }
    std::fs::remove_dir_all(&backup).map_err(|e| io("remove the old folder", e))?;
    std::fs::remove_file(&journal).map_err(|e| io("remove the conversion journal", e))?;
    Ok(Converted { dir: new_dir, notes })
}

const STAGING_PREFIX: &str = ".converting-";
const BACKUP_PREFIX: &str = ".convert-backup-";
const JOURNAL_PREFIX: &str = ".convert-journal-";

/// Finish or undo a conversion a crash interrupted, from its journal. With
/// the old folder still aside and nothing at the new path, the old folder
/// goes back, to convert again; with the converted folder in place, the old
/// one is deleted. A copy that never moved is deleted either way.
fn recover(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        let Some(key) = file.strip_prefix(JOURNAL_PREFIX) else { continue };
        let journal = entry.path();
        let text = match std::fs::read_to_string(&journal) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(journal = %journal.display(), error = %e, "an applet conversion journal could not be read");
                continue;
            }
        };
        let mut lines = text.lines();
        let (Some(dir), Some(new_dir)) = (lines.next(), lines.next()) else {
            tracing::error!(journal = %journal.display(), "an applet conversion journal is incomplete; it was written before the folder moved, so it is removed");
            let _ = std::fs::remove_file(&journal);
            continue;
        };
        let backup = root.join(format!("{BACKUP_PREFIX}{key}"));
        let staging = root.join(format!("{STAGING_PREFIX}{key}"));
        let result = if !backup.exists() {
            Ok(())
        } else if root.join(new_dir).join("SKILL.md").is_file() && !root.join(new_dir).join("manifest.toml").exists() {
            std::fs::remove_dir_all(&backup)
        } else {
            std::fs::rename(&backup, root.join(dir))
        };
        match result {
            Ok(()) => {
                let _ = std::fs::remove_dir_all(&staging);
                let _ = std::fs::remove_file(&journal);
                tracing::info!(dir, "recovered an interrupted applet conversion");
            }
            Err(e) => tracing::error!(dir, error = %e, "an interrupted applet conversion could not be recovered; its old folder is at {}", backup.display()),
        }
    }
}

/// Copy a folder whole: dotfiles, `.git` and symlinks included (a link is
/// copied as a link), and any entry that cannot be read fails the copy.
/// Conversion replaces the folder, so anything this skipped would be lost.
fn copy_whole(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let meta = std::fs::symlink_metadata(&src)?;
        if meta.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(&src)?, &dst)?;
        } else if meta.is_dir() {
            copy_whole(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Flush every file and folder under `dir` to disk.
fn sync_tree(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = std::fs::symlink_metadata(entry.path())?;
        if meta.is_dir() {
            sync_tree(&entry.path())?;
        } else if meta.is_file() {
            sync_file(&entry.path())?;
        }
    }
    sync_dir(dir)
}

fn sync_file(path: &Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

fn sync_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

/// `until = "once"` on a schedule as the `on` trigger: the next time the
/// schedule fires after `now`, in the box's timezone, to the minute.
pub(crate) fn next_once(cron: &str, tz: chrono_tz::Tz, now: chrono::DateTime<chrono::Utc>) -> Option<String> {
    let parsed = crate::scheduler::slots::parse(cron)?;
    let at = crate::scheduler::slots::next_after(&parsed, tz, now)?.with_timezone(&tz);
    Some(at.format("on %Y-%m-%d at %H:%M").to_string())
}

/// Convert every `manifest.toml` folder in the state root: the person's own
/// applets, imports, and anything an older box wrote. Run before the catalog
/// reads the root, at boot and on each reload, so a folder that arrives as a
/// manifest (an import, a restored backup) is a `SKILL.md` by the time it
/// loads. A folder that cannot convert is logged and left as it was, and
/// the catalog does not load it. Returns how many converted.
pub async fn convert_state_root(pool: &sqlx::PgPool) -> usize {
    let root = super::state_root();
    recover(&root);
    let mut dirs: Vec<String> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else { return 0 };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == crate::api::chat_applets::SAVED_SKILLS_DIR || !entry.path().is_dir() {
            continue;
        }
        if entry.path().join("manifest.toml").is_file() {
            dirs.push(name);
            continue;
        }
        for inner in std::fs::read_dir(entry.path()).into_iter().flatten().flatten() {
            if inner.path().join("manifest.toml").is_file() {
                dirs.push(format!("{name}/{}", inner.file_name().to_string_lossy()));
            }
        }
    }
    if dirs.is_empty() {
        return 0;
    }
    let tz = crate::scheduler::resolve_schedule_tz(pool).await;
    let now = chrono::Utc::now();
    let next = |cron: &str| next_once(cron, tz, now);
    let mut converted = 0;
    for dir in dirs {
        match convert_folder(&root, &dir, false, &next) {
            Ok(c) => {
                tracing::info!(from = %dir, to = %c.dir, notes = %c.notes.join(" "), "converted an applet to SKILL.md");
                converted += 1;
            }
            Err(why) => tracing::error!(dir = %dir, why = %why, "an applet's manifest.toml could not convert to SKILL.md; it is not loaded"),
        }
    }
    converted
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped tree was converted with `convert_folder` on 2026-10-27,
    /// and checked then to load with the same id, schedule, prompt, command,
    /// source, writes, owner, triggers, limits and model. No manifest is
    /// left, every applet loads, and none landed in the `/` menu by mistake.
    #[test]
    fn the_shipped_tree_is_all_skill_md() {
        let shipped = super::super::shipped_root();
        let mut loaded = 0;
        for entry in std::fs::read_dir(&shipped).unwrap().flatten() {
            let dir = entry.file_name().to_string_lossy().into_owned();
            assert!(!entry.path().join("manifest.toml").exists(), "{dir} still has a manifest.toml");
            let Ok(text) = std::fs::read_to_string(entry.path().join("SKILL.md")) else { continue };
            let t = super::super::skill::parse(&text, &entry.path(), &dir, true)
                .unwrap_or_else(|e| panic!("{dir}: {e:?}"));
            let chat =
                ["admin", "deep-research", "meeting-prep", "morning-brief", "trip-planner", "weekly-review", "new-applet"];
            assert_eq!(t.invoke_chat, chat.contains(&dir.as_str()), "{dir}");
            loaded += 1;
        }
        assert!(loaded >= 30, "only {loaded} applets loaded");
    }

    /// A one-off becomes `on <date>`, a condition becomes `run` SQL, and a
    /// face and tables move to their new names.
    #[test]
    fn a_one_off_a_condition_a_face_and_tables_convert() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("user/water_log");
        std::fs::create_dir_all(folder.join("face")).unwrap();
        std::fs::create_dir_all(folder.join("schema")).unwrap();
        std::fs::write(folder.join("face/index.html"), "<p>water</p>").unwrap();
        std::fs::write(folder.join("schema/0001_schema.sql"), "create table applet_user__water_log.cups (id int);").unwrap();
        std::fs::write(
            folder.join("manifest.toml"),
            "name = \"Water log\"\ndescription = \"Counts the water you drink.\"\nowner = \"ai\"\n\
             triggers = [\"cron\", \"manual\", \"tool\"]\nschedule = \"0 0 9 * * *\"\nuntil = \"once\"\n\
             condition = \"extract(dow from now()) <> 0\"\nagent = \"Remind them to drink water.\"\n\
             default_enabled = false\n[config]\nchat_id = \"chat_x\"\n[config.limits]\nmax_llm_cost = 0.2\n",
        )
        .unwrap();
        let next = |cron: &str| (cron == "0 0 9 * * *").then(|| "on 2031-03-04 at 09:00".to_string());
        let c = convert_folder(root.path(), "user/water_log", false, &next).unwrap();
        assert_eq!(c.dir, "user/water-log");
        assert!(c.notes.iter().any(|n| n.contains("[config.chat_id]")), "{:?}", c.notes);
        let new = root.path().join("user/water-log");
        assert!(!root.path().join("user/water_log").exists());
        assert!(new.join("view.html").is_file() && !new.join("face").exists());
        assert!(new.join("tables/0001_schema.sql").is_file());
        let t = super::super::skill::parse(&std::fs::read_to_string(new.join("SKILL.md")).unwrap(), &new, &c.dir, false)
            .unwrap();
        assert_eq!(t.id_prefix.as_deref(), Some("applet_user__water_log"));
        assert_eq!(t.owner, "ai");
        assert_eq!(t.schedule.as_deref(), Some("0 0 9 4 3 *"));
        assert_eq!(t.until.as_deref(), Some("once"));
        assert_eq!(t.run_sql.as_deref(), Some("select 1 where (extract(dow from now()) <> 0)"));
        assert!(t.triggers.contains(&"tool".to_string()));
        assert!(!t.default_enabled);
    }

    #[test]
    fn a_one_off_runs_at_the_schedules_next_time() {
        let now = chrono::DateTime::parse_from_rfc3339("2031-03-03T18:00:00Z").unwrap().with_timezone(&chrono::Utc);
        let ny: chrono_tz::Tz = "America/New_York".parse().unwrap();
        assert_eq!(next_once("0 0 9 * * *", ny, now).as_deref(), Some("on 2031-03-04 at 09:00"));
        assert_eq!(next_once("not a cron", ny, now), None);
    }

    /// A folder that cannot convert is left exactly as it was.
    #[test]
    fn a_conversion_keeps_dotfiles_and_links() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("pack/keep_me");
        std::fs::create_dir_all(folder.join(".git")).unwrap();
        std::fs::write(folder.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(folder.join(".env"), "COLOR=blue\n").unwrap();
        std::fs::write(folder.join("manifest.toml"), "name = \"Keep\"\nowner = \"user\"\ntriggers = [\"manual\"]\nagent = \"Say hello.\"\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("notes.txt", folder.join("link")).unwrap();
        let c = convert_folder(root.path(), "pack/keep_me", false, &|_| None).unwrap();
        let new = root.path().join(&c.dir);
        assert_eq!(c.dir, "pack/keep-me");
        assert!(new.join("SKILL.md").is_file());
        assert_eq!(std::fs::read_to_string(new.join(".git/HEAD")).unwrap(), "ref: refs/heads/main\n");
        assert!(new.join(".env").is_file());
        #[cfg(unix)]
        assert_eq!(std::fs::read_link(new.join("link")).unwrap(), std::path::PathBuf::from("notes.txt"));
        assert!(!folder.exists());
        let left: Vec<_> = std::fs::read_dir(root.path()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, ["pack"], "no staging, backup or journal is left");
    }

    /// An import's folder kept its hyphens in the old id, so the converted
    /// file pins it; a chat-made applet's id needs no pin.
    #[test]
    fn an_imported_applet_keeps_its_id() {
        let root = tempfile::tempdir().unwrap();
        let manifest = "name = \"Odd\"\nowner = \"user\"\ntriggers = [\"manual\"]\nagent = \"Say hello.\"\n";
        for (dir, id) in [
            ("github-com-nick-pack/daily_note", "applet_github-com-nick-pack__daily_note"),
            ("github-com-nick-one", "applet_github-com-nick-one"),
        ] {
            std::fs::create_dir_all(root.path().join(dir)).unwrap();
            std::fs::write(root.path().join(dir).join("manifest.toml"), manifest).unwrap();
            let c = convert_folder(root.path(), dir, false, &|_| None).unwrap();
            let new = root.path().join(&c.dir);
            let text = std::fs::read_to_string(new.join("SKILL.md")).unwrap();
            let t = super::super::skill::parse(&text, &new, &c.dir, false).unwrap();
            assert_eq!(t.id_prefix.as_deref(), Some(id), "{text}");
        }
    }

    /// A crash between the two renames leaves the old folder aside: recovery
    /// puts it back. A crash after them leaves the converted one in place:
    /// recovery deletes the old copy.
    #[test]
    fn an_interrupted_conversion_recovers() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        std::fs::create_dir_all(r.join(".convert-backup-a_b")).unwrap();
        std::fs::write(r.join(".convert-backup-a_b/manifest.toml"), "x").unwrap();
        std::fs::create_dir_all(r.join(".converting-a_b")).unwrap();
        std::fs::write(r.join(".convert-journal-a_b"), "a_b\na-b\n").unwrap();
        recover(r);
        assert!(r.join("a_b/manifest.toml").is_file(), "the old folder came back");
        assert!(!r.join(".converting-a_b").exists() && !r.join(".convert-journal-a_b").exists());

        std::fs::create_dir_all(r.join(".convert-backup-c_d")).unwrap();
        std::fs::create_dir_all(r.join("c-d")).unwrap();
        std::fs::write(r.join("c-d/SKILL.md"), "x").unwrap();
        std::fs::write(r.join(".convert-journal-c_d"), "c_d\nc-d\n").unwrap();
        recover(r);
        assert!(!r.join(".convert-backup-c_d").exists(), "the old copy went once the new one was in place");
        assert!(r.join("c-d/SKILL.md").is_file());
        assert!(!r.join("c_d").exists());
    }

    #[test]
    fn a_folder_that_cannot_convert_is_untouched() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("pack/odd_one");
        std::fs::create_dir_all(&folder).unwrap();
        let manifest = "name = \"Odd\"\nowner = \"user\"\ntriggers = [\"manual\"]\ncommand = [\"node\", \"main.js\", \"--flag\"]\n";
        std::fs::write(folder.join("manifest.toml"), manifest).unwrap();
        assert!(convert_folder(root.path(), "pack/odd_one", false, &|_| None).is_err());
        assert_eq!(std::fs::read_to_string(folder.join("manifest.toml")).unwrap(), manifest);
        assert!(!root.path().join("pack/odd-one").exists());
    }
}

