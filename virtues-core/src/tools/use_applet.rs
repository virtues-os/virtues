//! `use_applet`: the assistant loads an applet into the turn that calls it.
//!
//! The Agent Skills spec loads a skill in three stages: its name and
//! description up front, its body when it is used, and its other files when
//! the body asks for them. Chat lists the applets whose `invoke` includes
//! `assistant` in the tool's description ([`definition`]); a call with a name
//! returns that applet's prompt, which the model follows with the turn's own
//! tools; a call with `file` returns one file from its `references/` or
//! `assets/`. An applet's own turn or run, whose prompt is already loaded,
//! gets the tool for its files alone ([`own_files_definition`]).
//!
//! The prompt runs inside the calling turn's limits. An applet's `limit`,
//! `model` and `allowed-tools` shape the turns and runs it starts itself,
//! not a turn that loads it.

use super::executor::{ToolError, ToolResult};
use crate::applet_templates::UsableApplet;
use virtues_registry::tools::UsableEntry;

pub(crate) const TOOL: &str = "use_applet";

/// The largest file `use_applet` returns. A reference longer than this is
/// more than a turn should carry; split it into files the prompt names.
const MAX_FILE_BYTES: u64 = 64 * 1024;

fn entry(a: &UsableApplet) -> UsableEntry<'_> {
    UsableEntry { name: &a.name, description: &a.description, files: &a.files }
}

/// The tool for chat, listing every applet the assistant may load, or none
/// when there is no such applet.
pub(crate) fn definition() -> Option<serde_json::Value> {
    let applets = crate::applet_templates::assistant_applets();
    if applets.is_empty() {
        return None;
    }
    let entries: Vec<UsableEntry> = applets.iter().map(entry).collect();
    Some(super::llm_definition(virtues_registry::tools::use_applet_tool(&entries)))
}

/// The tool for an applet's own run, by its row id, when its folder has
/// files.
pub(crate) fn own_files_definition_for_id(applet_id: &str) -> Option<serde_json::Value> {
    let dir = crate::applet_templates::dir_for_applet_id(applet_id)?;
    own_files_definition(dir.rsplit('/').next()?)
}

/// The tool for an applet's own turn or run, when its folder has files.
pub(crate) fn own_files_definition(name: &str) -> Option<serde_json::Value> {
    let applet = crate::applet_templates::usable_applet_named(name)?;
    if applet.files.is_empty() {
        return None;
    }
    Some(super::llm_definition(virtues_registry::tools::applet_files_tool(&entry(&applet))))
}

pub(crate) fn execute(arguments: &serde_json::Value) -> Result<ToolResult, ToolError> {
    let name = arguments
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ToolError::InvalidParameters("name is required".into()))?;
    let file = arguments.get("file").and_then(|v| v.as_str()).map(str::trim).filter(|f| !f.is_empty());
    let applet = crate::applet_templates::usable_applet_named(name)
        .ok_or_else(|| ToolError::InvalidParameters(format!("there is no applet named {name} with a prompt")))?;
    match file {
        Some(file) => read_file(&applet, file),
        None => load(&applet),
    }
}

/// The second stage: the applet's prompt, for the model to follow now.
fn load(applet: &UsableApplet) -> Result<ToolResult, ToolError> {
    if !applet.for_assistant {
        return Err(ToolError::InvalidParameters(format!(
            "{} is not one the assistant loads. The owner starts it with /{} in the composer, or it runs on its own.",
            applet.name, applet.name
        )));
    }
    Ok(ToolResult::success(serde_json::json!({
        "applet": applet.name,
        "instructions": applet.body,
        "files": applet.files,
        "next": "Follow these instructions now, in this turn, with the tools you have. Read a listed file with use_applet and its path only when the instructions need it.",
    })))
}

/// The third stage: one of the applet's listed files, as text. Only a path
/// the reader listed is read, so nothing outside the folder can be named.
fn read_file(applet: &UsableApplet, file: &str) -> Result<ToolResult, ToolError> {
    if !applet.files.iter().any(|f| f == file) {
        let listed = if applet.files.is_empty() { "none".to_string() } else { applet.files.join(", ") };
        return Err(ToolError::InvalidParameters(format!(
            "{} has no file {file}. Its files: {listed}.",
            applet.name
        )));
    }
    let path = applet.folder.join(file);
    let size = std::fs::metadata(&path)
        .map_err(|e| ToolError::ExecutionFailed(format!("{file} could not be read: {e}")))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(ToolError::ExecutionFailed(format!(
            "{file} is {size} bytes, more than the {MAX_FILE_BYTES} a turn reads. Answer without it, or ask the owner to split it."
        )));
    }
    let bytes = std::fs::read(&path).map_err(|e| ToolError::ExecutionFailed(format!("{file} could not be read: {e}")))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| ToolError::ExecutionFailed(format!("{file} is not text, so it cannot be read into the turn")))?;
    Ok(ToolResult::success(serde_json::json!({
        "applet": applet.name,
        "file": file,
        "text": text,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn water(folder: &std::path::Path, for_assistant: bool) -> UsableApplet {
        std::fs::create_dir_all(folder.join("references")).unwrap();
        std::fs::write(folder.join("references/units.md"), "One glass is 250 ml.").unwrap();
        std::fs::write(folder.join("SKILL.md"), "---\nname: water\n---\nsecret").unwrap();
        UsableApplet {
            name: "water".into(),
            description: "Tracks glasses of water. Use when someone says they drank water.".into(),
            body: "Log what they drank in the drinks table.".into(),
            files: vec!["references/units.md".into()],
            folder: folder.to_path_buf(),
            for_assistant,
        }
    }

    #[test]
    fn loading_returns_the_prompt_and_its_files() {
        let dir = tempfile::tempdir().unwrap();
        let r = load(&water(dir.path(), true)).unwrap();
        assert!(r.success);
        assert_eq!(r.data["instructions"], "Log what they drank in the drinks table.");
        assert_eq!(r.data["files"][0], "references/units.md");

        let e = load(&water(dir.path(), false)).unwrap_err().to_string();
        assert!(e.contains("/water"), "{e}");
    }

    /// Only a listed path is read, so `..`, an absolute path or the folder's
    /// own SKILL.md cannot be named.
    #[test]
    fn a_file_is_read_only_when_listed() {
        let dir = tempfile::tempdir().unwrap();
        let a = water(dir.path(), true);
        let r = read_file(&a, "references/units.md").unwrap();
        assert_eq!(r.data["text"], "One glass is 250 ml.");
        for bad in ["SKILL.md", "../SKILL.md", "/etc/hosts", "references/../SKILL.md"] {
            let e = read_file(&a, bad).unwrap_err().to_string();
            assert!(e.contains("has no file"), "{bad}: {e}");
        }
    }

    #[test]
    fn a_large_or_binary_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = water(dir.path(), true);
        std::fs::write(dir.path().join("references/big.md"), vec![b'a'; MAX_FILE_BYTES as usize + 1]).unwrap();
        std::fs::write(dir.path().join("references/pic.png"), [0xff, 0xd8, 0xff, 0x00]).unwrap();
        a.files.extend(["references/big.md".into(), "references/pic.png".into()]);
        assert!(read_file(&a, "references/big.md").unwrap_err().to_string().contains("more than"));
        assert!(read_file(&a, "references/pic.png").unwrap_err().to_string().contains("not text"));
    }

    /// Chat's tool names each applet with its description and files; an
    /// applet's own turn gets a tool that only reads its files.
    #[test]
    fn the_definitions_list_names_descriptions_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let a = water(dir.path(), true);
        let chat = super::super::llm_definition(virtues_registry::tools::use_applet_tool(&[entry(&a)]));
        let text = chat["function"]["description"].as_str().unwrap();
        assert!(text.contains("- water: Tracks glasses of water."), "{text}");
        assert!(!text.contains("references/units.md"), "loading lists the files: {text}");
        assert_eq!(chat["function"]["parameters"]["properties"]["name"]["enum"][0], "water");
        assert_eq!(chat["function"]["name"], TOOL);

        let own = super::super::llm_definition(virtues_registry::tools::applet_files_tool(&entry(&a)));
        let text = own["function"]["description"].as_str().unwrap();
        assert!(text.contains("Files: references/units.md."), "{text}");
        assert_eq!(own["function"]["parameters"]["required"], serde_json::json!(["name", "file"]));
        assert_eq!(own["function"]["parameters"]["properties"]["file"]["enum"][0], "references/units.md");
    }

    /// No shipped applet is a prompt the assistant loads today, so chat on a
    /// box with only shipped applets is offered no `use_applet`.
    #[test]
    fn chat_offers_it_only_when_an_applet_is_loadable() {
        let shipped_only = crate::applet_templates::assistant_applets().is_empty();
        assert_eq!(definition().is_none(), shipped_only);
    }
}
