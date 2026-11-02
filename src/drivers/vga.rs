use core::fmt;
use spin::{Lazy, Mutex};
use volatile::Volatile;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    pub const fn new(fg: Color, bg: Color) -> Self {
        ColorCode((bg as u8) << 4 | (fg as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ScreenChar {
    pub ascii: u8,
    pub color: ColorCode,
}

const VGA_HEIGHT: usize = 25;
const VGA_WIDTH: usize = 80;

#[repr(transparent)]
struct VgaBuffer {
    chars: [[Volatile<ScreenChar>; VGA_WIDTH]; VGA_HEIGHT],
}

pub struct VgaWriter {
    pub col: usize,
    pub color: ColorCode,
    buffer: &'static mut VgaBuffer,
}

impl VgaWriter {
    pub fn write_byte(&mut self, b: u8) {
        match b {
            b'\n' => self.new_line(),
            0x08 => self.backspace(),
            byte => {
                if self.col >= VGA_WIDTH {
                    self.new_line();
                }
                let row = VGA_HEIGHT - 1;
                let col = self.col;
                self.buffer.chars[row][col].write(ScreenChar {
                    ascii: byte,
                    color: self.color,
                });
                self.col += 1;
            }
        }
    }

    pub fn write_str(&mut self, s: &str) {
        for b in s.bytes() {
            match b {
                0x20..=0x7E | b'\n' | 0x08 => self.write_byte(b),
                _ => self.write_byte(0xFE),
            }
        }
    }

    fn backspace(&mut self) {
        if self.col > 0 {
            self.col -= 1;
            let row = VGA_HEIGHT - 1;
            let col = self.col;
            self.buffer.chars[row][col].write(ScreenChar {
                ascii: b' ',
                color: self.color,
            });
        }
    }

    fn new_line(&mut self) {
        for r in 1..VGA_HEIGHT {
            for c in 0..VGA_WIDTH {
                let cell = self.buffer.chars[r][c].read();
                self.buffer.chars[r - 1][c].write(cell);
            }
        }
        self.clear_row(VGA_HEIGHT - 1);
        self.col = 0;
    }

    pub fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii: b' ',
            color: self.color,
        };
        for c in 0..VGA_WIDTH {
            self.buffer.chars[row][c].write(blank);
        }
    }

    pub fn clear(&mut self) {
        for r in 0..VGA_HEIGHT {
            self.clear_row(r);
        }
        self.col = 0;
    }
}

impl fmt::Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_str(s);
        Ok(())
    }
}

pub static WRITER: Lazy<Mutex<VgaWriter>> = Lazy::new(|| {
    Mutex::new(VgaWriter {
        col: 0,
        color: ColorCode::new(Color::LightRed, Color::Black),
        buffer: unsafe { &mut *(0xB8000 as *mut VgaBuffer) },
    })
});

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::drivers::vga::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(mut writer) = WRITER.try_lock() {
            let _ = writer.write_fmt(args);
        }
    });
}