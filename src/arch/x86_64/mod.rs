pub mod context;
pub mod gdt;
pub mod interrupts;
pub mod io;
pub mod paging;
pub mod serial;
pub mod simd;

pub use context::switch_context;

pub fn init() {
    gdt::init();
    interrupts::init_idt();
    simd::init();
    serial::init();
    unsafe {
        interrupts::PICS.lock().initialize();
        io::outb(0x21, 0xF8);
        io::outb(0xA1, 0xEF);
    }
}

pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

pub fn wait_for_interrupt() {
    unsafe {
        core::arch::asm!("sti", "hlt", "cli", options(nomem, nostack));
    }
}