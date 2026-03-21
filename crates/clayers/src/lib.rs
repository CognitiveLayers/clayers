mod adopt;
mod cli;
mod doc;
mod embedded;
mod repo;
mod serve;

fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Run the clayers CLI, parsing arguments from `std::env::args`.
pub fn cli_main() {
    install_crypto_provider();
    cli::cli_main();
}

/// Run the clayers CLI with explicit arguments (first element is the program name).
pub fn cli_main_from<I, T>(args: I)
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    install_crypto_provider();
    cli::cli_main_from(args);
}
