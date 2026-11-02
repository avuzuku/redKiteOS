use crate::arch::x86_64::io::{inb, outb};

#[derive(Debug, Clone, Copy)]
pub struct RtcTime {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
}

fn read_register(reg: u8) -> u8 {
    unsafe {
        outb(0x70, reg);
        inb(0x71)
    }
}

fn is_updating() -> bool {
    (read_register(0x0A) & 0x80) != 0
}

fn bcd_to_bin(val: u8) -> u8 {
    (val & 0x0F) + ((val / 16) * 10)
}

pub fn read() -> RtcTime {
    while is_updating() {}
    let mut second = read_register(0x00);
    let mut minute = read_register(0x02);
    let mut hour = read_register(0x04);
    let mut day = read_register(0x07);
    let mut month = read_register(0x08);
    let mut year = read_register(0x09) as u16;

    let b = read_register(0x0B);
    if (b & 0x04) == 0 {
        second = bcd_to_bin(second);
        minute = bcd_to_bin(minute);
        hour = ((hour & 0x7F) / 16) * 10 + (hour & 0x0F);
        day = bcd_to_bin(day);
        month = bcd_to_bin(month);
        year = bcd_to_bin(year as u8) as u16;
    }
    year += 2000;

    RtcTime { second, minute, hour, day, month, year }
}