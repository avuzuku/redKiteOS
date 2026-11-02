pub mod allocator;
pub mod error;
pub mod ipc;
pub mod loader;
pub mod process;
pub mod syscall;
pub mod task;
pub mod timer;

use spin::Mutex;
use x86_64::structures::paging::OffsetPageTable;

pub static MAPPER: Mutex<Option<OffsetPageTable<'static>>> = Mutex::new(None);
pub static FRAME_ALLOCATOR: Mutex<Option<crate::arch::x86_64::paging::BootInfoFrameAllocator>> = Mutex::new(None);