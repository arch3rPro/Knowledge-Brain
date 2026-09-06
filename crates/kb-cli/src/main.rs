use clap::Parser;

#[derive(Parser)]
#[command(
    name = "kb",
    version,
    about = "Knowledge-Brain portable knowledge vault"
)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
