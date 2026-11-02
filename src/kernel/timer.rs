use core::sync::atomic::{AtomicU64, Ordering};

static TICKS: AtomicU64 = AtomicU64::new(0);

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack));
}

pub fn init() {
    unsafe {
        let divisor: u16 = 11932;
        outb(0x43, 0x36);
        outb(0x40, (divisor & 0xFF) as u8);
        outb(0x40, ((divisor >> 8) & 0xFF) as u8);
    }
}

pub fn tick() {
    let count = TICKS.fetch_add(1, Ordering::Relaxed);
    if count % 10 == 0 {
        crate::kernel::process::schedule();
    }
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

pub fn uptime_ms() -> u64 {
    (ticks() * 1000) / 100
}

pub fn sleep_ticks(count: u64) {
    let end = ticks() + count;
    let was_enabled = x86_64::instructions::interrupts::are_enabled();
    while ticks() < end {
        unsafe {
            core::arch::asm!("sti", "hlt", options(nomem, nostack));
        }
    }
    if !was_enabled {
        x86_64::instructions::interrupts::disable();
    }
}