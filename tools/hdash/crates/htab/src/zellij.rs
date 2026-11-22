//! The pane's reach into the zellij session it runs in: naming its own tab, and opening a file
//! beside it.
//!
//! A tab name may lead with the session-state hook's idle (`● `) or busy (`○ `) marker; that
//! prefix is the hook's, and a rename here keeps it.

use std::path::Path;
use std::process::{Command, Stdio};

use log::warn;
use serde_json::Value;

const MARKERS: [&str; 2] = ["● ", "○ "];

/// The id and current name of the tab this process's pane is in, when it runs in a zellij pane.
pub fn own_tab() -> Option<(u64, String)> {
    let pane = std::env::var("ZELLIJ_PANE_ID").ok()?.parse::<u64>().ok()?;
    let out = Command::new("zellij")
        .args(["action", "list-panes", "-a", "--json"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    tab_of_pane(&String::from_utf8_lossy(&out.stdout), pane)
}

/// Rename tab `tab_id` to `label`, keeping the idle or busy marker that `current` leads with.
pub fn rename_tab(tab_id: u64, current: &str, label: &str) {
    let label = format!("{}{label}", marker_of(current));
    let status = Command::new("zellij")
        .args([
            "action",
            "rename-tab",
            "--tab-id",
            &tab_id.to_string(),
            &label,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => warn!("zellij rename-tab exited {s}"),
        Err(e) => warn!("cannot run zellij rename-tab: {e}"),
    }
}

/// Open `path` in Helix in a floating pane of this session that closes with the editor; outside
/// zellij, with `xdg-open`.
pub fn open_file(path: &Path) {
    if std::env::var_os("ZELLIJ").is_none() {
        hwt_core::spawn_detached(&["-f", "xdg-open", &path.to_string_lossy()]);
        return;
    }
    let dir = path.parent().unwrap_or(path);
    let spawned = Command::new("zellij")
        .args(["action", "new-pane", "--floating", "--close-on-exit"])
        .arg("--width")
        .arg("90%")
        .arg("--height")
        .arg("90%")
        .arg("--x")
        .arg("5%")
        .arg("--y")
        .arg("5%")
        .arg("--cwd")
        .arg(dir)
        .arg("--")
        .arg("hx")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if let Err(e) = spawned {
        warn!("cannot open {} in a floating pane: {e}", path.display());
    }
}

fn marker_of(name: &str) -> &'static str {
    MARKERS
        .into_iter()
        .find(|m| name.starts_with(m))
        .unwrap_or("")
}

fn tab_of_pane(list_panes_json: &str, pane: u64) -> Option<(u64, String)> {
    let panes: Value = serde_json::from_str(list_panes_json).ok()?;
    panes.as_array()?.iter().find_map(|p| {
        let is_plugin = p.get("is_plugin").and_then(Value::as_bool).unwrap_or(false);
        let id = p.get("id").and_then(Value::as_u64)?;
        if is_plugin || id != pane {
            return None;
        }
        let tab = p.get("tab_id").and_then(Value::as_u64)?;
        let name = p.get("tab_name").and_then(Value::as_str).unwrap_or("");
        Some((tab, name.to_owned()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tab_is_found_by_terminal_pane_id_not_plugin_id() {
        let json = r#"[
            {"id":1,"is_plugin":true,"tab_id":0,"tab_name":"a"},
            {"id":0,"is_plugin":false,"tab_id":0,"tab_name":"a"},
            {"id":1,"is_plugin":false,"tab_id":3,"tab_name":"● b"}
        ]"#;
        assert_eq!(tab_of_pane(json, 1), Some((3, "● b".to_owned())));
        assert_eq!(tab_of_pane(json, 9), None);
        assert_eq!(tab_of_pane("not json", 1), None);
    }

    #[test]
    fn only_a_leading_idle_or_busy_marker_is_kept() {
        assert_eq!(marker_of("● x · plan"), "● ");
        assert_eq!(marker_of("○ x"), "○ ");
        assert_eq!(marker_of("x ● y"), "");
        assert_eq!(marker_of(""), "");
    }
}
