use clap::Parser;
use dev_cleaner::cli::Cli;

fn main() {
    let cli = Cli::parse();

    if let Err(err) = cli.run() {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}
