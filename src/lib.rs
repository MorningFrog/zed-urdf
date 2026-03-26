use zed::{Command, Result};
use zed_extension_api as zed;

/// The main extension type registered with Zed.
///
/// This type is intentionally stateless because all runtime intelligence
/// for URDF completions lives in the external language server binary.
struct UrdfExtension;

impl zed::Extension for UrdfExtension {
    /// Create a new extension instance.
    fn new() -> Self {
        Self
    }

    /// Return the command that launches the URDF language server.
    ///
    /// Zed will call this when it needs semantic language features such as
    /// completions. We look for `urdf-language-server` in the current
    /// worktree environment and forward the worktree shell environment to it.
    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Command> {
        let binary = worktree.which("urdf-language-server").ok_or_else(|| {
            "Could not find `urdf-language-server` in PATH. \
                 Build and install the server first with \
                 `cargo install --path ./urdf-language-server --force`."
                .to_string()
        })?;

        Ok(Command {
            command: binary,
            args: vec![],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(UrdfExtension);
