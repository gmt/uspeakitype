mod audio;
mod backend;
mod ui;

use clap::Parser;

#[derive(Parser)]
#[command(name = "barbara")]
#[command(about = "Streaming speech-to-text with live revision")]
struct Args {
    #[arg(long)]
    headless: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    println!("barbara v{}", env!("CARGO_PKG_VERSION"));

    if args.headless {
        println!("Headless mode not yet implemented");
    } else {
        println!("UI mode not yet implemented");
    }

    Ok(())
}
