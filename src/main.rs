#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::alloc::Layout;
use core::panic::PanicInfo;
use redkite_os::arch::x86_64::paging::{self, BootInfoFrameAllocator};
use redkite_os::kernel::allocator;
use redkite_os::sys::fs::VFS;
use redkite_os::{println, usr};
use x86_64::VirtAddr;

entry_point!(kernel_main);

#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    println!();
    println!("==================================================");
    println!("  [!] BAKA! WHERES MY RAM?! (OOM ERROR)           ");
    println!("==================================================");
    println!("The allocator starved! Failed to get {} bytes (align {}).", layout.size(), layout.align());
    println!("System suspended in despair. UwU");
    println!("==================================================");
    redkite_os::arch::x86_64::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let s = alloc::format!("KERNEL PANIC: {}\n", info);
    for b in s.bytes() {
        unsafe {
            redkite_os::arch::x86_64::io::outb(0x3F8, b);
        }
    }

    println!("\n==================================================");
    println!("  [!] KERNEL PANIC: EXISTENTIAL CRISIS! (x_x)");
    println!("==================================================");
    println!("{}", info);
    println!("I-It's not like I wanted to crash or anything, b-baka!");
    println!("==================================================");
    redkite_os::arch::x86_64::hlt_loop();
}

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    let phys_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { paging::init(phys_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("Heap init failed");
    redkite_os::drivers::gfx::init(&mut mapper, &mut frame_allocator);

    *redkite_os::kernel::MAPPER.lock() = Some(mapper);
    *redkite_os::kernel::FRAME_ALLOCATOR.lock() = Some(frame_allocator);

    redkite_os::init();

    if let Err(e) = VFS.lock().init() {
        println!("[!] VFS init error: {}", e.as_str());
    }

    let _ = redkite_os::kernel::loader::deploy_binaries();

    let _ = VFS.lock().create_dir(3, "Desktop", 0, 0);

    redkite_os::drivers::net::init_pci_network();

    redkite_os::drivers::gfx::open_desktop();

    usr::bin::sh::main();
}