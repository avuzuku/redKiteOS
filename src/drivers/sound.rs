use crate::arch::x86_64::io::{inb, outb};

pub fn play_freq(freq: u32) {
    if freq == 0 {
        stop();
        return;
    }
    let div = 1193180 / freq;
    unsafe {
        outb(0x43, 0xB6);
        outb(0x42, (div & 0xFF) as u8);
        outb(0x42, ((div >> 8) & 0xFF) as u8);
        let tmp = inb(0x61);
        if tmp != (tmp | 3) {
            outb(0x61, tmp | 3);
        }
    }
}

pub fn stop() {
    unsafe {
        let tmp = inb(0x61) & 0xFC;
        outb(0x61, tmp);
    }
}

pub fn beep() {
    play_freq(880);
    for _ in 0..1_500_000 {
        unsafe { core::arch::asm!("nop"); }
    }
    stop();
}

pub fn click() {
    play_freq(1200);
    for _ in 0..400_000 {
        unsafe { core::arch::asm!("nop"); }
    }
    stop();
}