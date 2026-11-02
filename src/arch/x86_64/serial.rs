use crate::arch::x86_64::io::{inb, outb};

const PORT_COM1: u16 = 0x3F8;

pub fn init() {
    unsafe {
        outb(PORT_COM1 + 1, 0x00);
        outb(PORT_COM1 + 3, 0x80);
        outb(PORT_COM1 + 0, 0x01);
        outb(PORT_COM1 + 1, 0x00);
        outb(PORT_COM1 + 3, 0x03);
        outb(PORT_COM1 + 2, 0xC7);
        outb(PORT_COM1 + 4, 0x0B);
    }
}

pub fn read_byte() -> Option<u8> {
    unsafe {
        if (inb(PORT_COM1 + 5) & 1) != 0 {
            Some(inb(PORT_COM1))
        } else {
            None
        }
    }
}

pub fn write_byte(b: u8) {
    unsafe {
        while (inb(PORT_COM1 + 5) & 0x20) == 0 {}
        outb(PORT_COM1, b);
    }
}