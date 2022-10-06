use std::time::Duration;
use chip8_base::{Interpreter, Pixel};

const FONT: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

// Position font at 0x50
const FONT_INDEX: usize = 0x50;

static TIME_60HZ: Duration = Duration::from_nanos(16666667);

pub struct VMState {
    memory: [u8; 4096],
    registers: [u8; 16],
    program_counter: u16,
    stack_pointer: u8,
    stack: [u16; 16],
    index: u16,
    display: chip8_base::Display,

    speed: Duration,
    decrement_timer: Duration,
    delay_timer: u8,
    sound_timer: u8,
}

impl VMState {
    pub fn new(freq: u32) -> VMState {
        let s = 1_f64 / freq as f64;    // s per clock

        let mut m = [0; 4096];
        for (fi, mi) in (FONT_INDEX..FONT_INDEX+50).enumerate() {
            m[mi] = FONT[fi]; 
        }
        VMState { 
            memory: m.clone(), 
            registers: [0; 16], 
            program_counter: 0x200, 
            stack_pointer: 0,
            stack: [0; 16],
            index: 0,
            display: [[Pixel::default(); 64]; 32],
            speed: Duration::from_secs_f64(s),
            decrement_timer: TIME_60HZ,
            delay_timer: 0,
            sound_timer: 0,
        }
    }

    pub fn load(&mut self, rom: &Vec<u8>) {
        for (i, b) in rom.iter().enumerate() {
            self.memory[0x200 + i] = *b;
        }
    }

    fn fetch(&mut self) -> (u8, u8) {
        let ind = self.program_counter as usize;
        (self.memory[ind], self.memory[ind+1])
    }

    fn extract_nibbles(i: (u8, u8)) -> (u8, u8, u8, u8) {
        return ((i.0 >> 4) & 0xf, i.0 & 0xf, (i.1 >> 4) & 0xf, i.1 & 0xf);
    }

    fn extract_12_bits(n0: u8, n1: u8, n2: u8) -> u16 {
        let p0 = (n0 as u16) << 8;
        let p1 = (n1 as u16) << 4;
        let p2 = n2 as u16;
        return p0 | p1 | p2;
    }

    fn execute(&mut self, i: (u8, u8), keys: &chip8_base::Keys) -> Option<chip8_base::Display> {
        let (n0, n1, n2, n3) = VMState::extract_nibbles(i);
        match (n0, n1, n2, n3) {
            (0x0, 0x0, 0x0, 0x0) => {},
            // 00E0 CLS: Clears the display
            (0x0, 0x0, 0xE, 0x0) => {
                self.display = [[Pixel::default(); 64]; 32];
                return Some(self.display);
            },
            // 00EE RET: Return from subroutine
            (0x0, 0x0, 0xE, 0xE) => {
                self.program_counter = self.stack[self.stack_pointer as usize];
                self.stack_pointer -= 1;
            },
            // 1nnn JP addr: Jump to location nnn
            (0x1, a0, a1, a2) => self.program_counter = VMState::extract_12_bits(a0, a1, a2),
            // 2nnn CALL addr: Call subroutine at nnn
            (0x2, a0, a1, a2) => {
                self.stack_pointer += 1;
                self.stack[self.stack_pointer as usize] = self.program_counter;
                self.program_counter = VMState::extract_12_bits(a0, a1, a2);
            },
            // 3xkk SE Vx, byte: Skip next instruction if Vx = kk.
            (0x3, x, _, _) => {
                if self.registers[x as usize] == i.1 {
                    self.program_counter += 2;
                }
            },
            // 4xkk SNE Vx, byte: Skip next instruction if Vx != kk.
            (0x4, x, _, _) => {
                if self.registers[x as usize] != i.1 {
                    self.program_counter += 2;
                }
            },
            // 5xy0 SE Vx, Vy: Skip next instruction if Vx = Vy.
            (0x5, x, y, 0) => {
                if self.registers[x as usize] == self.registers[y as usize] {
                    self.program_counter += 2;
                }
            },
            // 6xkk LD Vx, byte: Set Vx = kk.
            (0x6, x, _, _) => self.registers[x as usize] = i.1,
            // 7xkk ADD Vx, byte: Set Vx = Vx + kk.
            (0x7, x, _, _) => self.registers[x as usize] = self.registers[x as usize].wrapping_add(i.1),
            // 8xy0 LD Vx, Vy: Set Vx = Vy.
            (0x8, x, y, 0x0) => self.registers[x as usize] = self.registers[y as usize],
            // 8xy1 OR Vx, Vy: Set Vx = Vx OR Vy.
            (0x8, x, y, 0x1) => {
                let vx = self.registers[x as usize];
                let vy = self.registers[y as usize];
                self.registers[x as usize] = vx | vy;
            },
            // 8xy2 AND Vx, Vy: Set Vx = Vx AND Vy.
            (0x8, x, y, 0x2) => {
                let vx = self.registers[x as usize];
                let vy = self.registers[y as usize];
                self.registers[x as usize] = vx & vy;
            },
            // 8xy3 XOR Vx, Vy: Set Vx = Vx XOR Vy.
            (0x8, x, y, 0x3) => {
                let vx = self.registers[x as usize];
                let vy = self.registers[y as usize];
                self.registers[x as usize] = vx ^ vy;
            },
            // 8xy4 ADD Vx, Vy: Set Vx = Vx + Vy, set VF = carry.
            (0x8, x, y, 0x4) => {
                let vx = self.registers[x as usize];
                let vy = self.registers[y as usize];
                let (r, carry) = vx.overflowing_add(vy);
                self.registers[x as usize] = r;
                self.registers[0xF as usize] = if carry { 1 } else { 0 };
            },
            // 8xy5 SUB Vx, Vy: Set Vx = Vx - Vy, set VF = NOT borrow.
            (0x8, x, y, 0x5) => {
                let vx = self.registers[x as usize];
                let vy = self.registers[y as usize];
                let (r, borrow) = vx.overflowing_sub(vy);
                self.registers[x as usize] = r;
                self.registers[0xF as usize] = if borrow { 0 } else { 1 };
            },
            // 8xy6 SHR Vx {, Vy}: Set Vx = Vx SHR 1.
            (0x8, x, _, 0x6) => {
                let vx = self.registers[x as usize];
                self.registers[0xF as usize] = vx & 0x1;   // LSB
                self.registers[x as usize] = vx >> 1;
            },
            // 8xy7 SUBN Vx, Vy: Set Vx = Vy - Vx, set VF = NOT borrow.
            (0x8, x, y, 0x7) => {
                let vx = self.registers[x as usize];
                let vy = self.registers[y as usize];
                let (r, borrow) = vy.overflowing_sub(vx);
                self.registers[x as usize] = r;
                self.registers[0xF as usize] = if borrow { 0 } else { 1 };
            },
            // 8xyE SHL Vx {, Vy}: Set Vx = Vx SHL 1.
            (0x8, x, _, 0xE) => {
                let vx = self.registers[x as usize];
                self.registers[0xF as usize] = vx & 0x80;   // MSB
                self.registers[x as usize] = vx << 1;
            },
            // 9xy0 SNE Vx, Vy: Skip next instruction if Vx != Vy.
            (0x9, x, y, 0x0) => {
                if self.registers[x as usize] != self.registers[y as usize] {
                    self.program_counter += 2;
                }
            },
            // Annn LD I, addr: Set I = nnn.
            (0xA, n0, n1, n2) => self.index = VMState::extract_12_bits(n0, n1, n2),
            // Bnnn JP V0, addr: Jump to location nnn + V0.
            (0xB, n0, n1, n2) => {
                self.program_counter = VMState::extract_12_bits(n0, n1, n2) + (self.registers[0] as u16);
            },
            // Cxkk RND Vx, byte: Set Vx = random byte AND kk.
            (0xC, x, _, _) => {
                self.registers[x as usize] = rand::random::<u8>() & i.1;
            },
            // Dxyn DRW Vx, Vy, nibble: Display n-byte sprite starting at memory location I at (Vx, Vy), set VF = collision.
            (0xD, x, y, n) => {
                let tlx = self.registers[x as usize] % 64;
                let tly = self.registers[y as usize] % 32;
                self.registers[0xF] = 0;
                let ind = self.index as usize;
                let sprite = &self.memory[ind..(ind + n as usize)];

                for (i, row) in sprite.iter().enumerate() {
                    let pxy = tly + i as u8;
                    if pxy > 31 {
                        break;
                    }
                    
                    for j in 0..8 {
                        let pxx = tlx + j;
                        if pxx > 63 {
                            break;
                        }
                        let old_px = &mut self.display[pxy as usize][pxx as usize];
                        let mask = 2_u8.pow(7 - j as u32);
                        let new_u8 = (row & mask) >> (7 - j);
                        let new_px: Pixel = new_u8.try_into().unwrap();
                        if (new_px & *old_px).into() { // if collision
                            self.registers[0xF] = 1 
                        }
                        *old_px ^= new_px;
                    }
                }
                return Some(self.display)
            },
            // Ex9E SKP Vx: Skip next instruction if key with the value of Vx is pressed.
            (0xE, x, 0x9, 0xE) => {
                let vx = self.registers[x as usize];
                if keys[vx as usize] {
                    self.program_counter += 2;
                }
            },
            // ExA1 SKNP Vx: Skip next instruction if key with the value of Vx is not pressed.
            (0xE, x, 0xA, 0x1) => {
                let vx = self.registers[x as usize];
                if !keys[vx as usize] {
                    self.program_counter += 2;
                }
            },
            // Fx07 LD Vx, DT: Set Vx = delay timer value.
            (0xF, x, 0x0, 0x7) => {
                self.registers[x as usize] = self.delay_timer;
            },
            // Fx0A LD Vx, K Wait for a key press, store the value of the key in Vx.
            (0xF, x, 0x0, 0xA) => {
                if !keys.iter().any(|x| *x) {
                    self.program_counter -= 2;      // Reverse inc of PC so stays at same instr
                } else {
                    for (i, k) in keys.iter().enumerate() {
                        if *k {
                            self.registers[x as usize] = i as u8;
                            break;
                        }
                    }
                }
            },
            // Fx15 LD DT, Vx: Set delay timer = Vx.
            (0xF, x, 0x1, 0x5) => {
                self.delay_timer = self.registers[x as usize];
            },
            // Fx18 LD ST, Vx: Set sound timer = Vx.
            (0xF, x, 0x1, 0x8) => {
                self.sound_timer = self.registers[x as usize];
            },
            // Fx1E ADD I, Vx: Set I = I + Vx.
            (0xF, x, 0x1, 0xE) => {
                let vx = self.registers[x as usize];
                let (r, carry) = self.index.overflowing_add(vx.into());
                self.index = r;
                self.registers[0xF] = if carry || self.index > 0x0FFF { 1 } else { 0 };     // set flag if overflow
            },
            // Fx29 LD F, Vx: Set I = location of sprite for digit Vx.
            (0xF, x, 0x2, 0x9) => {
                let vx = self.registers[x as usize] as usize;
                self.index = FONT_INDEX as u16 + 5 * vx as u16;
            },
            // Fx33 LD B, Vx: Store BCD representation of Vx in memory locations I, I+1, and I+2.
            (0xF, x, 0x3, 0x3) => {
                let vx = self.registers[x as usize];
                
                let ind = self.index as usize;
                self.memory[ind] = (vx / 100) % 10;
                self.memory[ind + 1] = (vx / 10) % 10;
                self.memory[ind + 2] = (vx / 1) % 10;
            },
            // Fx55 LD [I], Vx: Store registers V0 through Vx in memory starting at location I.
            (0xF, x, 0x5, 0x5) => {
                let ind = self.index as usize;
                let end = x as usize;
                for i in 0..=end {
                    self.memory[ind + i] = self.registers[i];
                }
            },
            // Fx65 LD Vx, [I]: Read registers V0 through Vx from memory starting at location I.
            (0xF, x, 0x6, 0x5) => {
                let ind = self.index as usize;
                let end = x as usize;
                for i in 0..=end {
                    self.registers[i] = self.memory[ind + i];
                }
            },
            _ => println!("Not implemented {} {} {} {}", n0, n1, n2, n3),
        }
        None
    }
}

impl Interpreter for VMState {
    fn step(&mut self, keys: &chip8_base::Keys) -> Option<chip8_base::Display> {
        
        // Timers
        self.decrement_timer = self.decrement_timer.saturating_sub(self.speed);
        if self.decrement_timer == Duration::ZERO {
            if self.delay_timer > 0 { self.delay_timer -= 1 }
            if self.sound_timer > 0 { self.sound_timer -= 1 }
            self.decrement_timer = TIME_60HZ;
        }

        let instr = self.fetch();
        self.program_counter += 2;
        self.program_counter %= 4096;
        self.execute(instr, keys)
    }

    fn speed(&self) -> std::time::Duration {
        self.speed
    }

    fn buzzer_active(&self) -> bool {
        self.sound_timer > 0
        // true
    }
}

