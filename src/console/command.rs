use std::collections::BTreeMap;

use super::context::DeveloperExecutionContext;
use super::parser::ParsedCommand;

/// Structured output from executing a developer console command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    /// Command completed successfully with output text.
    Success(String),
    /// Command execution encountered an error with an actionable diagnostic message.
    Error(String),
    /// Console requested output clearing (Amendment 10).
    Clear,
}

impl CommandResult {
    #[inline]
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    #[inline]
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    #[inline]
    pub fn is_clear(&self) -> bool {
        matches!(self, Self::Clear)
    }

    pub fn output_text(&self) -> Option<&str> {
        match self {
            Self::Success(msg) | Self::Error(msg) => Some(msg),
            Self::Clear => None,
        }
    }
}

/// Trait implemented by all extensible developer console commands.
///
/// HELP GENERATION (Amendment 14):
/// Help text is generated directly from `name`, `description`, `usage`, and `detailed_help`
/// to eliminate documentation drift.
pub trait ConsoleCommand: Send + Sync {
    /// Canonical command name (case-insensitive in parser).
    fn name(&self) -> &'static str;

    /// Short one-line summary displayed in `help`.
    fn description(&self) -> &'static str;

    /// Usage syntax string (e.g. `time set <day_fraction>`).
    fn usage(&self) -> &'static str;

    /// Optional extended documentation, subcommands, and examples.
    fn detailed_help(&self) -> Option<&'static str> {
        None
    }

    /// Executes the command given string arguments and developer context.
    fn execute(&self, args: &[String], ctx: &mut DeveloperExecutionContext) -> CommandResult;
}

/// Central registry storing and dispatching developer console commands.
pub struct CommandRegistry {
    commands: BTreeMap<String, Box<dyn ConsoleCommand>>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: BTreeMap::new(),
        }
    }

    /// Registers a command into the registry. Returns Err if a command with the same name already exists.
    pub fn register(&mut self, cmd: Box<dyn ConsoleCommand>) -> Result<(), String> {
        let name = cmd.name().to_lowercase();
        if self.commands.contains_key(&name) {
            return Err(format!("command '{}' is already registered", name));
        }
        self.commands.insert(name, cmd);
        Ok(())
    }

    /// Dispatches a parsed command line to its registered handler.
    pub fn dispatch(
        &self,
        parsed: &ParsedCommand,
        ctx: &mut DeveloperExecutionContext,
    ) -> CommandResult {
        let cmd_name = parsed.command.to_lowercase();

        // 1. Built-in help interception (help <command> or <command> help)
        if cmd_name == "help" {
            let target = parsed.args.first().map(|s| s.as_str());
            return self.generate_help(target);
        }

        if parsed.args.first().map(|s| s.as_str()) == Some("help") {
            return self.generate_help(Some(&cmd_name));
        }

        // 2. Command lookup
        match self.commands.get(&cmd_name) {
            Some(cmd) => cmd.execute(&parsed.args, ctx),
            None => CommandResult::Error(format!(
                "unknown command \"{}\". Type \"help\" for a list of available commands.",
                cmd_name
            )),
        }
    }

    /// Generates structured help output from registered command definitions (Amendment 14).
    pub fn generate_help(&self, target_command: Option<&str>) -> CommandResult {
        if let Some(target) = target_command {
            let target_lower = target.to_lowercase();
            match self.commands.get(&target_lower) {
                Some(cmd) => {
                    let mut out = format!("Usage: {}\n\n{}", cmd.usage(), cmd.description());
                    if let Some(detail) = cmd.detailed_help() {
                        out.push_str("\n\n");
                        out.push_str(detail);
                    }
                    CommandResult::Success(out)
                }
                None => CommandResult::Error(format!(
                    "unknown command \"{}\". Type \"help\" to list available commands.",
                    target
                )),
            }
        } else {
            let mut out = String::from("Available developer commands:\n");
            for (name, cmd) in &self.commands {
                out.push_str(&format!("  {:<10} {}\n", name, cmd.description()));
            }
            out.push_str("\nType \"help <command>\" or \"<command> help\" for detailed usage.");
            CommandResult::Success(out)
        }
    }

    /// Number of registered commands.
    #[inline]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether the registry has any registered commands.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Checks if a command is registered.
    #[inline]
    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(&name.to_lowercase())
    }
}
