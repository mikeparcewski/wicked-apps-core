//! Shared event-emit seam for the four apps — the single path every app calls to publish a
//! `wicked.*` event to the bus.
//!
//! ## Why wicked-apps-core ships its own seam
//! The `wicked-estate` crate has an emit seam at `src/emit.rs`, but it is declared `mod emit;` in
//! the estate **binary** (`main.rs`) — it is NOT part of the `wicked-estate` library API, so a
//! path dependency cannot import it. This module mirrors that seam's shape and contract
//! (`EmitEvent::new` + `emit_event`, fire-and-forget toward the bus, loud-and-durable on failure)
//! for the apps domain.
//!
//! ## Contract (identical to the estate seam)
//! [`emit_event`] spawns the canonical `wicked-bus emit` CLI. Emit is fire-and-forget by design
//! (it must never block or fail the caller), but the failure path is loud and durable: if the bus
//! CLI cannot be spawned or exits non-zero, the event is appended as one NDJSON line to a
//! dead-letter spool and a greppable [`DEADLETTER_MARKER`] is written to stderr. A dropped event
//! is a defect, never silent.
//!
//! ## Cross-platform
//! The spool root resolves via `std::env::var_os("HOME")` / `USERPROFILE` joined with
//! `std::path::Path` segments (never a hardcoded `~`), and is overridable via
//! [`DEADLETTER_ENV`]. The bus program is overridable via [`EMIT_PROGRAM_ENV`] (tests point it at
//! a guaranteed-missing command to exercise the failure path with no network and no real bus).

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

/// Overrides the dead-letter spool file path. When unset, the spool defaults to
/// `<home>/.something-wicked/wicked-apps/emit-deadletter.ndjson`.
pub const DEADLETTER_ENV: &str = "WICKED_APPS_EMIT_DEADLETTER";

/// Overrides the bus-emit program. When unset, the seam spawns `wicked-bus` on `PATH`.
pub const EMIT_PROGRAM_ENV: &str = "WICKED_APPS_EMIT_PROGRAM";

/// Loud, greppable marker written to stderr whenever an event is dead-lettered.
pub const DEADLETTER_MARKER: &str = "EMIT-DEADLETTER:";

/// A coarse wicked-bus event ready to publish through the shared seam.
///
/// `event_type` follows the ecosystem convention `wicked.<noun>.<verb>` (validate with
/// [`crate::validate_event_type`] before constructing). `payload` is an already-built JSON object.
#[derive(Debug, Clone)]
pub struct EmitEvent {
    /// `wicked.<noun>.<verb>` — e.g. `wicked.policy.evaluated`.
    pub event_type: String,
    /// Top-level bus domain — e.g. `wicked-governance`.
    pub domain: String,
    /// Bus subdomain — e.g. `governance.evaluation`.
    pub subdomain: String,
    /// Structured event payload (a JSON object).
    pub payload: serde_json::Value,
}

impl EmitEvent {
    /// Construct an event. `domain` is the producing app (e.g. `wicked-governance`); `subdomain`
    /// is the dotted subdomain (e.g. `governance.evaluation`); `event_type` is the full
    /// `wicked.<noun>.<verb>` name.
    pub fn new(
        event_type: impl Into<String>,
        domain: impl Into<String>,
        subdomain: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            domain: domain.into(),
            subdomain: subdomain.into(),
            payload,
        }
    }

    /// The full dead-letter record: the envelope the bus would have received, plus the reason it
    /// was spooled. Serialized as one NDJSON line.
    fn deadletter_record(&self, reason: &str) -> serde_json::Value {
        serde_json::json!({
            "type": self.event_type,
            "domain": self.domain,
            "subdomain": self.subdomain,
            "payload": self.payload,
            "deadletter_reason": reason,
        })
    }
}

/// Resolve the home directory cross-platform without external deps: `HOME` (unix) or `USERPROFILE`
/// (Windows).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Resolve the dead-letter spool path: the [`DEADLETTER_ENV`] override if set, else
/// `<home>/.something-wicked/wicked-apps/emit-deadletter.ndjson`.
///
/// Returns `None` only when no override is set AND the home directory cannot be resolved.
pub fn deadletter_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(DEADLETTER_ENV) {
        return Some(PathBuf::from(p));
    }
    let home = home_dir()?;
    Some(
        home.join(".something-wicked")
            .join("wicked-apps")
            .join("emit-deadletter.ndjson"),
    )
}

/// Append one NDJSON line for `event` to the dead-letter spool, creating parent dirs as needed.
fn dead_letter(event: &EmitEvent, reason: &str) -> std::io::Result<PathBuf> {
    let path = deadletter_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot resolve dead-letter spool path (no HOME/USERPROFILE and no WICKED_APPS_EMIT_DEADLETTER)",
        )
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(&event.deadletter_record(reason))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(path)
}

/// The bus-emit program to spawn: the [`EMIT_PROGRAM_ENV`] override if set, else `wicked-bus`.
fn emit_program() -> String {
    std::env::var(EMIT_PROGRAM_ENV).unwrap_or_else(|_| "wicked-bus".to_string())
}

/// Publish `event` through the shared seam.
///
/// Fire-and-forget toward the bus by design, but never silent on failure: if the bus CLI cannot
/// be spawned, or exits non-zero, the event is dead-lettered to the spool and a loud
/// [`DEADLETTER_MARKER`] line is written to stderr.
///
/// Returns `true` if the bus accepted the event (child exited zero), `false` if it was
/// dead-lettered.
pub fn emit_event(event: &EmitEvent) -> bool {
    let program = emit_program();
    let payload = event.payload.to_string();
    let result = Command::new(&program)
        .arg("emit")
        .arg("--type")
        .arg(&event.event_type)
        .arg("--domain")
        .arg(&event.domain)
        .arg("--subdomain")
        .arg(&event.subdomain)
        .arg("--payload")
        .arg(&payload)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let reason = match result {
        Ok(status) if status.success() => return true,
        Ok(status) => format!("bus exited non-zero: {status}"),
        Err(e) => format!("spawn `{program}` failed: {e}"),
    };

    eprintln!(
        "{DEADLETTER_MARKER} event `{}` not delivered ({reason}); spooling to dead-letter",
        event.event_type
    );
    match dead_letter(event, &reason) {
        Ok(path) => eprintln!(
            "{DEADLETTER_MARKER} spooled `{}` to {}",
            event.event_type,
            path.display()
        ),
        Err(e) => eprintln!(
            "{DEADLETTER_MARKER} FAILED to spool `{}` to dead-letter: {e}",
            event.event_type
        ),
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    // `emit_event` reads process-global env vars; serialize the env-mutating tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_env() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn read_lines(path: &std::path::Path) -> Vec<serde_json::Value> {
        let body = std::fs::read_to_string(path).expect("spool file must exist");
        body.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("each spool line must be valid JSON"))
            .collect()
    }

    /// A dropped event lands as a parseable NDJSON line in the spool with its payload intact.
    /// Falsifier: a dropped event leaving no spool line → `read_lines` empty / file absent → fail.
    #[test]
    fn dropped_event_lands_in_deadletter_spool() {
        let _guard = lock_env();
        let dir = std::env::temp_dir().join(format!("wicked-apps-emit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let spool = dir.join("emit-deadletter.ndjson");
        let _ = std::fs::remove_file(&spool);

        // SAFETY: env access is serialized by ENV_LOCK; vars are restored before unlock.
        unsafe {
            std::env::set_var(DEADLETTER_ENV, &spool);
            std::env::set_var(EMIT_PROGRAM_ENV, "wicked-bus-does-not-exist-xyzzy-9000");
        }

        let event = EmitEvent::new(
            crate::EV_POLICY_EVALUATED,
            "wicked-governance",
            "governance.evaluation",
            serde_json::json!({ "claim_id": "c1", "decision": "allow" }),
        );
        let accepted = emit_event(&event);

        let lines = read_lines(&spool);

        unsafe {
            std::env::remove_var(DEADLETTER_ENV);
            std::env::remove_var(EMIT_PROGRAM_ENV);
        }
        let _ = std::fs::remove_file(&spool);

        assert!(!accepted, "spawn-failed emit must report not-accepted");
        assert_eq!(lines.len(), 1, "exactly one NDJSON line must be spooled");
        let rec = &lines[0];
        assert_eq!(rec["type"], crate::EV_POLICY_EVALUATED);
        assert_eq!(rec["domain"], "wicked-governance");
        assert_eq!(rec["subdomain"], "governance.evaluation");
        assert_eq!(rec["payload"]["claim_id"], "c1");
        assert!(
            rec["deadletter_reason"].is_string(),
            "the spooled record records why it was dropped"
        );
    }

    /// Default spool path is derived from home (cross-platform) and ends with the documented
    /// suffix — never a hardcoded `~`.
    #[test]
    fn default_deadletter_path_is_under_home() {
        let _guard = lock_env();
        unsafe {
            std::env::remove_var(DEADLETTER_ENV);
        }
        if let Some(p) = deadletter_path() {
            let s = p.to_string_lossy().replace('\\', "/");
            assert!(
                s.ends_with(".something-wicked/wicked-apps/emit-deadletter.ndjson"),
                "unexpected default spool path: {s}"
            );
            assert!(!s.contains('~'), "path must be expanded, not literal ~");
        }
    }
}
