use crate::platforms::{self, ResumeCommand};
use crate::state::{ResumeSelection, resolve};
use tracing::warn;

/// Resume the selected agent session while leaving the monitor running.
///
/// The launcher chooses the OS-specific new-window mechanism and detaches its
/// stdio from the TUI. When no new window can be opened, it restores the
/// current terminal and exec-replaces `aum` with the agent CLI instead.
pub fn resume(selection: ResumeSelection) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let target = ResumeTarget::from_selection(selection);

    if let Err(e) = spawn_new_window(&target) {
        warn!(
            "could not open a new terminal window ({e}); \
             handing off the current terminal instead"
        );
        return exec_handoff(&target);
    }

    Ok(())
}

/// Fully resolved process launch, kept private so callers only need to know
/// about `ResumeSelection` and `resume`.
#[derive(Debug)]
struct ResumeTarget {
    command: ResumeCommand,
    cwd: String,
    session_id: String,
}

impl ResumeTarget {
    fn from_selection(selection: ResumeSelection) -> Self {
        let session_id = resolve(selection.session_id).to_owned();
        debug_assert!(
            !session_id.is_empty(),
            "AppState only creates resume selections with a session id"
        );

        Self {
            command: platforms::entry_for_platform(selection.platform).resume_command(&session_id),
            cwd: resolve(selection.cwd).to_owned(),
            session_id,
        }
    }
}

/// Spawn `cmd` fully detached and reap it on a background thread. The new
/// terminal window owns its own I/O, so we neither wait for it nor hold its
/// `Child`; the reaper thread prevents the exited launcher (notably macOS
/// `open`, which returns immediately) from lingering as a zombie for the life
/// of the long-running monitor.
fn spawn_detached(cmd: &mut std::process::Command) -> std::io::Result<()> {
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Single-quote a string for `/bin/sh`, escaping embedded quotes — so a cwd
/// or arg containing spaces or shell metacharacters is passed through literally.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The `/bin/sh` command that resumes `target`: `cd <cwd> && exec <prog> …`.
/// The `cd` is omitted when the working directory is unknown. This is shared
/// by the macOS `.command` script and Linux `sh -c` invocation.
fn resume_shell_line(target: &ResumeTarget) -> String {
    let mut line = String::new();
    if !target.cwd.is_empty() {
        line.push_str(&format!("cd {} && ", shell_quote(&target.cwd)));
    }
    line.push_str("exec ");
    line.push_str(&shell_quote(target.command.program));
    for arg in &target.command.args {
        line.push(' ');
        line.push_str(&shell_quote(arg));
    }
    line
}

/// Open the resume command in a new terminal window. `Ok` means a window was
/// launched; `Err` lets `resume` perform its exec hand-off fallback.
#[cfg(target_os = "macos")]
fn spawn_new_window(target: &ResumeTarget) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;

    // A `*.command` file opened via `open` launches the user's default
    // terminal (Terminal.app, or iTerm if they've set it) in a new window.
    let safe: String = target
        .session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let path = std::env::temp_dir().join(format!("aum-resume-{safe}.command"));
    // ponytail: the script lingers in the temp dir until the OS clears it;
    // reopening the same session overwrites it, so at most one per session.
    std::fs::write(&path, format!("#!/bin/sh\n{}\n", resume_shell_line(target)))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;

    spawn_detached(
        std::process::Command::new("open")
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
}

/// One Linux terminal emulator plus the arguments needed before `sh -c`.
#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy)]
struct TerminalEmulator {
    program: &'static str,
    launch_args: &'static [&'static str],
}

/// Linux has no standard new-window mechanism, so try common emulators in
/// order. The first process that starts owns the resume shell command.
#[cfg(not(target_os = "macos"))]
const TERMINAL_EMULATORS: &[TerminalEmulator] = &[
    TerminalEmulator {
        program: "x-terminal-emulator",
        launch_args: &["-e"],
    },
    TerminalEmulator {
        program: "ghostty",
        launch_args: &["-e"],
    },
    TerminalEmulator {
        program: "kitty",
        launch_args: &["--"],
    },
    TerminalEmulator {
        program: "alacritty",
        launch_args: &["-e"],
    },
    TerminalEmulator {
        program: "wezterm",
        launch_args: &["start", "--"],
    },
    TerminalEmulator {
        program: "foot",
        launch_args: &[],
    },
    TerminalEmulator {
        program: "gnome-terminal",
        launch_args: &["--"],
    },
    TerminalEmulator {
        program: "konsole",
        launch_args: &["-e"],
    },
    TerminalEmulator {
        program: "xfce4-terminal",
        launch_args: &["-e"],
    },
    TerminalEmulator {
        program: "xterm",
        launch_args: &["-e"],
    },
];

#[cfg(not(target_os = "macos"))]
fn spawn_new_window(target: &ResumeTarget) -> std::io::Result<()> {
    use std::process::Stdio;

    let shell_line = resume_shell_line(target);
    let mut last_err = None;
    for terminal in TERMINAL_EMULATORS {
        let mut cmd = std::process::Command::new(terminal.program);
        cmd.args(terminal.launch_args)
            .arg("sh")
            .arg("-c")
            .arg(&shell_line)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match spawn_detached(&mut cmd) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no supported terminal emulator found",
        )
    }))
}

/// Restore the terminal and exec-replace `aum` with the agent CLI. This only
/// returns when exec itself fails.
fn exec_handoff(target: &ResumeTarget) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::os::unix::process::CommandExt;

    ratatui::restore();

    let mut cmd = std::process::Command::new(target.command.program);
    cmd.args(&target.command.args);
    if !target.cwd.is_empty() {
        cmd.current_dir(&target.cwd);
    }

    // `exec` only returns on failure. Surface it plainly on the now-restored
    // terminal so the user knows why nothing launched.
    let err = cmd.exec();
    eprintln!(
        "Failed to launch `{}` to resume session: {err}",
        target.command.program
    );
    Err(Box::new(err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Platform, intern};

    fn target(platform: Platform, session_id: &str, cwd: &str) -> ResumeTarget {
        ResumeTarget::from_selection(ResumeSelection {
            platform,
            session_id: intern(session_id),
            cwd: intern(cwd),
        })
    }

    #[test]
    fn target_combines_selection_with_platform_command() {
        let target = target(Platform::Codex, "abc", "/work/proj");
        assert_eq!(target.command.program, "codex");
        assert_eq!(target.command.args, vec!["resume", "abc"]);
        assert_eq!(target.cwd, "/work/proj");
        assert_eq!(target.session_id, "abc");
    }

    #[test]
    fn shell_quote_wraps_and_escapes() {
        assert_eq!(shell_quote("/a/b c"), "'/a/b c'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn resume_shell_line_includes_cd_only_when_cwd_known() {
        assert_eq!(
            resume_shell_line(&target(Platform::ClaudeCode, "abc", "/work/proj")),
            "cd '/work/proj' && exec 'claude' '--resume' 'abc'"
        );
        assert_eq!(
            resume_shell_line(&target(Platform::Cursor, "abc", "")),
            "exec 'cursor-agent' '--resume=abc'"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn terminal_emulators_are_tried_in_documented_order() {
        let programs: Vec<_> = TERMINAL_EMULATORS
            .iter()
            .map(|terminal| terminal.program)
            .collect();
        assert_eq!(
            programs,
            vec![
                "x-terminal-emulator",
                "ghostty",
                "kitty",
                "alacritty",
                "wezterm",
                "foot",
                "gnome-terminal",
                "konsole",
                "xfce4-terminal",
                "xterm",
            ]
        );
    }
}
