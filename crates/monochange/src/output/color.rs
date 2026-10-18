use std::ffi::OsString;
use std::future::Future;
use std::io::IsTerminal;

tokio::task_local! {
	static COLOR: ColorChoice;
}

/// The `--color` choice: style output automatically, always, or never.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ColorChoice {
	#[default]
	Auto,
	Always,
	Never,
}

impl ColorChoice {
	pub(crate) fn parse(value: &str) -> Option<Self> {
		match value {
			"auto" => Some(Self::Auto),
			"always" => Some(Self::Always),
			"never" => Some(Self::Never),
			_ => None,
		}
	}

	/// Read `--color <WHEN>` or `--color=<WHEN>` from raw arguments; the last
	/// valid value wins, matching clap's override behaviour.
	pub(crate) fn from_args(args: &[OsString]) -> Self {
		let mut choice = Self::Auto;
		let mut arguments = args.iter().filter_map(|argument| argument.to_str());
		while let Some(argument) = arguments.next() {
			let value = if argument == "--color" {
				arguments.next()
			} else {
				argument.strip_prefix("--color=")
			};
			if let Some(value) = value.and_then(Self::parse) {
				choice = value;
			}
		}
		choice
	}

	/// Whether a stream gets ANSI styling.
	///
	/// An explicit `always` or `never` wins over the environment, as
	/// <https://no-color.org> asks of per-invocation flags. Under `auto`,
	/// `NO_COLOR` turns colour off, `CLICOLOR_FORCE` turns it on, and
	/// `CLICOLOR=0` or `TERM=dumb` turn it off; otherwise only a terminal is
	/// styled.
	pub(crate) fn resolve(self, environment: ColorEnvironment, stream_is_terminal: bool) -> bool {
		match self {
			Self::Always => true,
			Self::Never => false,
			Self::Auto if environment.no_color => false,
			Self::Auto if environment.clicolor_force => true,
			Self::Auto => {
				!environment.clicolor_disabled && !environment.term_is_dumb && stream_is_terminal
			}
		}
	}
}

/// The colour-related environment variables, read once per decision.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ColorEnvironment {
	pub(crate) no_color: bool,
	pub(crate) clicolor_force: bool,
	pub(crate) clicolor_disabled: bool,
	pub(crate) term_is_dumb: bool,
}

impl ColorEnvironment {
	pub(crate) fn detect() -> Self {
		Self::from_lookup(|name| std::env::var(name).ok())
	}

	/// `NO_COLOR` counts only when non-empty, per <https://no-color.org>;
	/// `CLICOLOR_FORCE` counts when set to anything but `0`, per
	/// <https://bixense.com/clicolors/>.
	pub(crate) fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
		Self {
			no_color: lookup("NO_COLOR").is_some_and(|value| !value.is_empty()),
			clicolor_force: lookup("CLICOLOR_FORCE").is_some_and(|value| value != "0"),
			clicolor_disabled: lookup("CLICOLOR").is_some_and(|value| value == "0"),
			term_is_dumb: lookup("TERM").is_some_and(|term| term == "dumb"),
		}
	}
}

/// Run `future` with the command's `--color` choice in scope.
///
/// A task-local keeps concurrent invocations (such as parallel tests) from
/// seeing each other's setting.
pub(crate) async fn with_color_choice<F: Future>(choice: ColorChoice, future: F) -> F::Output {
	COLOR.scope(choice, future).await
}

/// The running command's `--color` choice, `auto` outside a command.
pub(crate) fn color_choice() -> ColorChoice {
	COLOR.try_with(|choice| *choice).unwrap_or_default()
}

/// Whether text written to stdout is styled.
pub(crate) fn stdout_color() -> bool {
	stream_color(std::io::stdout().is_terminal())
}

/// Whether text written straight to stderr is styled.
pub(crate) fn stderr_color() -> bool {
	stream_color(std::io::stderr().is_terminal())
}

/// Unit tests only get colour when they ask for it explicitly, so a developer's
/// terminal or `CLICOLOR_FORCE` never leaks escapes into snapshots.
fn stream_color(stream_is_terminal: bool) -> bool {
	let choice = color_choice();
	if cfg!(test) && choice == ColorChoice::Auto {
		return false;
	}
	choice.resolve(ColorEnvironment::detect(), stream_is_terminal)
}

#[cfg(test)]
#[path = "../__tests__/output_color_tests.rs"]
mod tests;
