#![no_std]
#![feature(abi_x86_interrupt)]

extern crate alloc;

pub mod arch;
pub mod drivers;
pub mod kernel;
pub mod sys;
pub mod usr;

pub fn init() {
    arch::x86_64::init();
    drivers::mouse::init();
    kernel::syscall::init();
    x86_64::instructions::interrupts::enable();
}