use std::{
    fs,
    io::{stdout, Stdout, Write},
    ops::Range,
    process::exit,
    time::{Duration, SystemTime},
};

use ascii::{AsAsciiStr, AsciiChar, ToAsciiChar};
use crossterm::{
    cursor,
    event::{poll, read, Event, KeyCode},
    execute,
    terminal::{self, size},
};

const MEMORY_SIZE: usize = 30_000;
const MAX_POINTER: usize = MEMORY_SIZE - 1;
const WRAPPING: bool = false;
const DEBUG: bool = true;

pub type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(e) = main_inner() {
        cleanup_terminal();
        eprintln!("Error: {}", e);
        exit(1);
    }
}

fn main_inner() -> Result {
    let mut args = std::env::args().skip(1);
    let code_path = args.next().ok_or("no file specified")?;
    let input_path = args.next();

    let code = fs::read_to_string(code_path)?;
    let input = if let Some(input_path) = input_path {
        Some(fs::read_to_string(input_path)?)
    } else {
        None
    };

    let mut i = BrainfuckInterpreter::new(&code, input.as_ref())?;

    let debugger = if DEBUG {
        ctrlc::set_handler(|| {
            cleanup_terminal();
        })?;

        Some(Debugger::new()?)
    } else {
        None
    };

    i.run(debugger)?;

    Ok(())
}

fn setup_terminal() {
    let mut stdout = stdout();

    execute!(stdout, terminal::EnterAlternateScreen).unwrap();
    execute!(stdout, cursor::Hide).unwrap();

    // Needed for when ytop is run in a TTY since TTYs don't actually have an alternate screen.
    // Must be executed after attempting to enter the alternate screen so that it only clears the
    // 		primary screen if we are running in a TTY.
    // If not running in a TTY, then we just end up clearing the alternate screen which should have
    // 		no effect.
    execute!(stdout, terminal::Clear(terminal::ClearType::All)).unwrap();

    terminal::enable_raw_mode().unwrap();
}

fn cleanup_terminal() {
    let mut stdout = stdout();

    // Needed for when ytop is run in a TTY since TTYs don't actually have an alternate screen.
    // Must be executed before attempting to leave the alternate screen so that it only modifies the
    // 		primary screen if we are running in a TTY.
    // If not running in a TTY, then we just end up modifying the alternate screen which should have
    // 		no effect.
    execute!(stdout, cursor::MoveTo(0, 0)).unwrap();
    execute!(stdout, terminal::Clear(terminal::ClearType::All)).unwrap();

    execute!(stdout, terminal::LeaveAlternateScreen).unwrap();
    execute!(stdout, cursor::Show).unwrap();

    terminal::disable_raw_mode().unwrap();
}

struct BrainfuckInterpreter {
    pub memory: [u8; MEMORY_SIZE],
    pub pointer: usize,
    pub loop_stack: Vec<usize>,
    pub input: Vec<AsciiChar>,
    pub input_pos: usize,
    pub code: Vec<char>,
    pub code_pos: usize,

    pub output: String,
}

struct Debugger {
    stdout: Stdout,
    paused: bool,
    size: (u16, u16),

    op_counter: usize,
    last_op_reset: SystemTime,
    last_ops_per_second: usize,

    memory_range: Range<usize>,

    update_frequency: usize,
    update_counter: usize,
}

impl Drop for Debugger {
    fn drop(&mut self) {
        cleanup_terminal();
    }
}

enum DebugCommand {
    Quit,
    Step,
    // Continue,
    // Pause,
}

struct Bounds {
    pub start: usize,
    pub end: usize,
    pub rel: u16,
}

impl Debugger {
    pub fn new() -> Result<Self> {
        setup_terminal();

        let mut stdout = stdout();
        stdout.flush()?;

        let size = size()?;

        Ok(Self {
            stdout,
            paused: true,
            size,
            op_counter: 0,
            last_op_reset: SystemTime::now(),
            last_ops_per_second: 0,
            memory_range: 0..size.0 as usize / 4,
            update_frequency: 0,
            update_counter: 0,
        })
    }

    /// Calculates the region of the buffer which should be displayed.
    ///
    /// `width`: The width of the resulting rendered text (**in characters**)
    /// `buf_len`: The length of the buffer to be rendered
    /// `pos`: The position of the cursor in the buffer
    fn region_bounds(width: u16, buf_len: usize, pos: usize) -> Bounds {
        let width = width as usize;

        let buf_start = pos.saturating_sub(width / 2);
        let buf_end = (buf_start + width / 2).min(buf_len);

        let pos_rel = pos - buf_start;

        Bounds {
            start: buf_start,
            end: buf_end,
            rel: pos_rel as u16,
        }
    }

    // fn region_bounds(width: u16, buf_len: usize, pos: usize) -> Bounds {
    //     let width = width as usize;

    //     let buf_start = pos.saturating_sub(width / 2);

    //     let pos_rel = pos - buf_start;

    //     let missing_left = width / 2 - pos_rel;

    //     let buf_end = (buf_start + width / 2 + missing_left).min(buf_len);

    //     Bounds {
    //         start: buf_start,
    //         end: buf_end,
    //         rel: pos_rel as u16,
    //     }
    // }

    fn draw_region(
        &mut self,
        label: &str,
        (px, py): (u16, u16),
        width: u16,
        buf: impl AsRef<str>,
        pos: usize,
    ) -> Result {
        execute!(self.stdout, cursor::MoveTo(px, py))?;
        print!("{}:", label);

        let buf = buf.as_ref();

        let Bounds { start, end, rel } = Self::region_bounds(width, buf.len(), pos);

        execute!(self.stdout, cursor::MoveTo(px, py + 1))?;
        print!("{}", &buf[start..end]);

        execute!(self.stdout, cursor::MoveTo(px + rel as u16, py + 2))?;
        print!("^");

        Ok(())
    }

    fn draw_memory(
        &mut self,
        (px, py): (u16, u16),
        width: u16,
        memory: &[u8],
        pointer: usize,
    ) -> Result {
        let cell_count = width / 4;
        // let usable_width = width - width % 4;

        if pointer >= self.memory_range.end {
            self.memory_range.start += 1;
            self.memory_range.end = self.memory_range.start + cell_count as usize;
            self.memory_range.end = self.memory_range.end.min(MEMORY_SIZE);
            self.memory_range.start = self
                .memory_range
                .start
                .min(self.memory_range.end - cell_count as usize);
        } else if pointer < self.memory_range.start {
            self.memory_range.start -= 1;
            self.memory_range.start = self.memory_range.start.max(0);
            self.memory_range.end = self.memory_range.start + cell_count as usize;
            self.memory_range.end = self
                .memory_range
                .end
                .max(self.memory_range.start + cell_count as usize);
        }

        execute!(self.stdout, cursor::MoveTo(px, py))?;
        print!("Memory:");

        // let Bounds { start, end, rel } = Self::region_bounds(unit_width, MEMORY_SIZE, pointer);
        let rel = pointer - self.memory_range.start;

        execute!(self.stdout, cursor::MoveTo(px, py + 1))?;

        let region = &memory[self.memory_range.clone()];

        let region = region
            .iter()
            .map(|b| format!("{b:03}"))
            .collect::<Vec<_>>()
            .join(" ");

        print!("{}", region);

        execute!(self.stdout, cursor::MoveTo(px + rel as u16 * 4, py + 2))?;
        print!("^");

        Ok(())
    }

    pub fn draw(
        &mut self,
        interpreter: &BrainfuckInterpreter,
        force: bool,
    ) -> Result<DebugCommand> {
        // calculate op/s once every second
        let now = SystemTime::now();
        if now.duration_since(self.last_op_reset)? > Duration::from_secs(1) {
            self.last_ops_per_second = self.op_counter;
            self.op_counter = 0;
            self.last_op_reset = SystemTime::now();
        }

        self.op_counter += 1;

        if !force && self.update_counter < self.update_frequency {
            self.update_counter += 1;
            return Ok(DebugCommand::Step);
        }

        self.update_counter = 0;

        execute!(self.stdout, terminal::Clear(terminal::ClearType::All))?;

        self.draw_region(
            "Input",
            (0, 0),
            self.size.0,
            &interpreter.input.as_ascii_str().unwrap(),
            interpreter.input_pos,
        )?;

        execute!(self.stdout, cursor::MoveTo(0, 4))?;
        print!("Pos: {}", interpreter.code_pos);

        self.draw_memory(
            (0, 6),
            self.size.0,
            &interpreter.memory,
            interpreter.pointer,
        )?;

        execute!(self.stdout, cursor::MoveTo(0, 10))?;
        print!("Pointer: {}", interpreter.pointer);

        self.draw_region(
            "Output",
            (0, 12),
            self.size.0,
            &interpreter.output,
            interpreter.output.len(),
        )?;

        self.draw_region(
            "Code",
            (0, 16),
            self.size.0,
            &interpreter
                .code
                .iter()
                .map(|c| if *c == '\n' { ' ' } else { *c })
                .collect::<String>(),
            interpreter.code_pos,
        )?;

        execute!(self.stdout, cursor::MoveTo(0, self.size.1 - 2))?;
        print!(
            "Update frequency: 1/{} updates displayed",
            self.update_frequency + 1
        );

        execute!(self.stdout, cursor::MoveTo(0, self.size.1 - 1))?;
        print!("Ops/s: {:.2}", self.last_ops_per_second);

        self.stdout.flush()?;

        if self.paused {
            loop {
                match read()? {
                    Event::Key(key) => match key.code {
                        KeyCode::Char('q') => {
                            break Ok(DebugCommand::Quit);
                        }
                        KeyCode::Char('c') => {
                            self.paused = false;
                            // break Ok(DebugCommand::Continue);
                            break Ok(DebugCommand::Step);
                        }
                        KeyCode::Char(_)
                        | KeyCode::Left
                        | KeyCode::Right
                        | KeyCode::Up
                        | KeyCode::Down => {
                            break Ok(DebugCommand::Step);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        } else {
            if poll(Duration::from_micros(10))? {
                match read()? {
                    Event::Key(key) => match key.code {
                        KeyCode::Char('q') => {
                            return Ok(DebugCommand::Quit);
                        }
                        KeyCode::Char('p') => {
                            self.paused = true;
                        }
                        KeyCode::Up => {
                            if self.update_frequency == 0 {
                                self.update_frequency = 1;
                            } else {
                                self.update_frequency = self.update_frequency.saturating_mul(2);
                            }
                        }
                        KeyCode::Down => {
                            if self.update_frequency == 1 {
                                self.update_frequency = 0;
                            } else {
                                self.update_frequency = self.update_frequency.saturating_div(2);
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            Ok(DebugCommand::Step)
        }
    }
}

impl BrainfuckInterpreter {
    pub fn new(code: &str, input: Option<&String>) -> Result<Self> {
        let input = input.cloned().unwrap_or_else(|| String::new());
        let input_ascii = input.as_ascii_str()?;
        let input = input_ascii.chars().collect::<Vec<_>>();

        Ok(Self {
            memory: [0; MEMORY_SIZE],
            pointer: 0,
            loop_stack: Vec::new(),
            input,
            input_pos: 0,
            code: code.chars().collect::<Vec<_>>(),
            code_pos: 0,
            output: String::new(),
        })
    }

    pub fn run(&mut self, mut debugger: Option<Debugger>) -> Result {
        loop {
            if let Some(debugger) = &mut debugger {
                if matches!(debugger.draw(self, false)?, DebugCommand::Quit) {
                    break Ok(());
                }
            }

            let c = self.code[self.code_pos];

            let mut increment = true;

            match c {
                '>' => {
                    self.pointer += 1;
                    if WRAPPING {
                        self.pointer = self.pointer % MAX_POINTER;
                    } else {
                        self.pointer = self.pointer.min(MAX_POINTER);
                    }
                }
                '<' => {
                    if WRAPPING {
                        if self.pointer == 0 {
                            self.pointer = MAX_POINTER;
                        } else {
                            self.pointer -= 1;
                        }
                    } else if self.pointer > 0 {
                        self.pointer -= 1;
                    }
                }
                '+' => {
                    self.memory[self.pointer] = self.memory[self.pointer].wrapping_add(1);
                }
                '-' => {
                    self.memory[self.pointer] = self.memory[self.pointer].wrapping_sub(1);
                }
                '.' => {
                    self.output
                        .push(self.memory[self.pointer].to_ascii_char().unwrap().as_char());
                }
                ',' => {
                    if let Some(in_c) = self.input.get(self.input_pos) {
                        self.memory[self.pointer] = in_c.as_byte();
                        self.input_pos += 1;
                    }
                    // if there is no next char, do not clobber the current pointer
                }
                '[' => {
                    self.loop_stack.push(self.code_pos + 1);
                }
                ']' => {
                    if self.memory[self.pointer] != 0 {
                        self.code_pos = *self.loop_stack.last().ok_or("unmatched ]")?;
                        increment = false;
                    } else {
                        self.loop_stack.pop();
                    }
                }
                _ => {}
            }

            if increment {
                self.code_pos += 1;
            }

            if self.code_pos >= self.code.len() {
                if let Some(debugger) = &mut debugger {
                    debugger.paused = true;
                    debugger.draw(self, true)?;
                }
                break Ok(());
            }
        }
    }
}
