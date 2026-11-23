//! erebor-agent [--data-dir DIR]
//! Same as `erebor agent start --foreground`, for service managers and development.

fn main() {
    let mut args = std::env::args().skip(1);
    let mut dir = std::env::var_os("EREBOR_DATA_DIR")
        .map(Into::into)
        .unwrap_or_else(erebor_core::default_data_dir);
    while let Some(a) = args.next() {
        match (a.as_str(), args.next()) {
            ("--data-dir", Some(d)) => dir = d.into(),
            ("--version" | "-V", _) => {
                return println!("erebor-agent {}", env!("CARGO_PKG_VERSION"))
            }
            _ => {
                eprintln!("usage: erebor-agent [--data-dir DIR]");
                std::process::exit(2);
            }
        }
    }
    if let Err(e) = erebor_agent::run(dir) {
        eprintln!("erebor-agent: {e}");
        std::process::exit(1);
    }
}
