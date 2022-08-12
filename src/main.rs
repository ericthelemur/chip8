mod interpreter;

fn main() {
    let mut state = interpreter::VMState::new(700);

    let rom = std::fs::read("roms/uwcs simp.ch8").expect("ROM file doesn't exist");

    state.load(&rom);

    chip8_base::run(state);
}
 