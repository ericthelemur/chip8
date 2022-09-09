mod interpreter;

use clap::Parser;

fn main() {
    let cli = Cli::parse();

    let filename: &str = &cli.rom;
    let mut state = interpreter::VMState::new(700);

    let rom = std::fs::read(filename).expect("ROM file doesn't exist");

    state.load(&rom);

    chip8_base::run(state);
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// A CHIP-8 ROM to load into the interpreter
    #[clap(validator = rom_exists)]
    rom: String,
}

fn rom_exists(f: &str) -> Result<(), &'static str> {
    let p = std::path::Path::new(f);
    if !p.is_file() {
        Err("File does not exist.")
    } else {
        Ok(())
    }
}
