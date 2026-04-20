mod adopt;
mod cli;
mod doc;
mod embedded;
mod gitignore;
mod repo;
#[cfg(feature = "semantic-search")]
mod search_cmd;
mod serve;

/// Run the clayers CLI, parsing arguments from `std::env::args`.
pub fn cli_main() {
    cli::cli_main();
}

/// Run the clayers CLI with explicit arguments (first element is the program name).
pub fn cli_main_from<I, T>(args: I)
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    cli::cli_main_from(args);
}
