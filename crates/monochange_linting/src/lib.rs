#![doc(
	html_logo_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-512.png",
	html_favicon_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/favicon.ico"
)]
#![forbid(clippy::indexing_slicing)]
#![doc = include_str!("crate_docs.md")]

pub use monochange_core::lint::LintCategory;
pub use monochange_core::lint::LintMaturity;
pub use monochange_core::lint::LintOptionDefinition;
pub use monochange_core::lint::LintOptionKind;
pub use monochange_core::lint::LintRule;

/// Construct a [`LintRule`] with less boilerplate.
#[macro_export]
macro_rules! declare_lint_rule {
    (
        $vis:vis $name:ident,
        id: $id:expr,
        name: $title:expr,
        description: $description:expr,
        category: $category:expr,
        maturity: $maturity:expr,
        autofixable: $autofixable:expr $(,
        options: $options:expr)? $(,)?
    ) => {
        #[derive(Debug)]
        $vis struct $name {
            rule: $crate::LintRule,
        }

        impl $name {
            #[must_use]
            $vis fn new() -> Self {
                let rule = $crate::LintRule::new(
                    $id,
                    $title,
                    $description,
                    $category,
                    $maturity,
                    $autofixable,
                ) $(.with_options($options))?;
                Self { rule }
            }
        }
    };
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
