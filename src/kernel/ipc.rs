use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, Page, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::VirtAddr;

pub const USER_SHMEM_BASE: u64 = 0x0000_0000_1000_0000;
pub const WINDOW_SHMEM_STRIDE: u64 = 0x0040_0000;
pub const MAX_WINDOWS: usize = 16;
pub const PAGE_SIZE: u64 = 4096;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    None = 0,
    KeyDown = 1,
    KeyUp = 2,
    MouseDown = 3,
    MouseUp = 4,
    MouseMove = 5,
    WindowClose = 6,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowEvent {
    pub event_type: u32,
    pub win_id: u32,
    pub code: u32,
    pub x: i32,
    pub y: i32,
}

pub struct WindowBuffer {
    pub id: usize,
    pub pid: usize,
    pub width: usize,
    pub height: usize,
    pub title: String,
    pub user_virt_addr: u64,
    pub pages_count: usize,
    pub frames: Vec<PhysFrame<Size4KiB>>,
    pub event_queue: VecDeque<WindowEvent>,
}

pub static WINDOW_TABLE: spin::Lazy<Mutex<Vec<Option<WindowBuffer>>>> =
    spin::Lazy::new(|| Mutex::new(Vec::new()));

pub fn sys_create_window(width: usize, height: usize, title: &str) -> Result<usize, &'static str> {
    if width == 0 || height == 0 || width > 1920 || height > 1080 {
        return Err("Invalid window dimensions");
    }

    let buffer_size = width.checked_mul(height).and_then(|px| px.checked_mul(4)).ok_or("Buffer size overflow")?;
    let pages_count = ((buffer_size as u64) + PAGE_SIZE - 1) / PAGE_SIZE;

    let mut alloc_guard = crate::kernel::FRAME_ALLOCATOR.lock();
    let allocator = match alloc_guard.as_mut() {
        Some(a) => a,
        None => return Err("MMU allocator unavailable"),
    };

    let mut frames = Vec::with_capacity(pages_count as usize);
    for _ in 0..pages_count {
        match allocator.allocate_frame() {
            Some(f) => frames.push(f),
            None => return Err("Out of physical frames for window buffer"),
        }
    }
    drop(alloc_guard);

    let mut table = WINDOW_TABLE.lock();
    let win_id = match table.iter().position(|slot| slot.is_none()) {
        Some(idx) => idx,
        None => {
            if table.len() >= MAX_WINDOWS {
                return Err("Window table limit reached");
            }
            let idx = table.len();
            table.push(None);
            idx
        }
    };

    let user_virt_addr = USER_SHMEM_BASE + (win_id as u64) * WINDOW_SHMEM_STRIDE;
    let pid = crate::kernel::process::current_pid();

    table[win_id] = Some(WindowBuffer {
        id: win_id,
        pid,
        width,
        height,
        title: String::from(title),
        user_virt_addr,
        pages_count: pages_count as usize,
        frames,
        event_queue: VecDeque::with_capacity(32),
    });
    drop(table);

    let _ = crate::drivers::wm::create_window(title, width, height);

    Ok(win_id)
}

pub fn sys_map_window_buffer(win_id: usize) -> Result<u64, &'static str> {
    let table = WINDOW_TABLE.lock();
    let win = table.get(win_id).and_then(|s| s.as_ref()).ok_or("Invalid window id")?;
    let user_virt_addr = win.user_virt_addr;
    let pages_count = win.pages_count;
    let frames = win.frames.clone();
    let total_bytes = win.width * win.height * 4;
    drop(table);

    let mut map_guard = crate::kernel::MAPPER.lock();
    let mut alloc_guard = crate::kernel::FRAME_ALLOCATOR.lock();
    let (mapper, allocator) = match (map_guard.as_mut(), alloc_guard.as_mut()) {
        (Some(m), Some(a)) => (m, a),
        _ => return Err("MMU subsystem unavailable"),
    };

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    for i in 0..pages_count {
        let page_addr = VirtAddr::new(user_virt_addr + (i as u64) * PAGE_SIZE);
        let page: Page<Size4KiB> = Page::containing_address(page_addr);
        if let Ok((_, flush)) = mapper.unmap(page) {
            flush.flush();
        }

        unsafe {
            mapper.map_to(page, frames[i], flags, allocator)
                .map_err(|_| "Failed to map window buffer frame")?
                .flush();
        }
    }

    let dest = user_virt_addr as *mut u8;
    unsafe {
        core::ptr::write_bytes(dest, 0, total_bytes);
    }

    Ok(user_virt_addr)
}

pub fn sys_poll_event(win_id: usize, event_ptr: *mut WindowEvent) -> Result<usize, &'static str> {
    crate::drivers::gfx::compositor_tick();

    if event_ptr.is_null() {
        return Err("EFAULT: Null event pointer");
    }

    let mut table = WINDOW_TABLE.lock();
    let win = table.get_mut(win_id).and_then(|s| s.as_mut()).ok_or("EBADF: Unknown window")?;

    if let Some(ev) = win.event_queue.pop_front() {
        unsafe {
            core::ptr::write(event_ptr, ev);
        }
        return Ok(1);
    }

    if let Some(key) = crate::kernel::task::pop_key() {
        let code = match key {
            pc_keyboard::DecodedKey::Unicode(c) => c as u32,
            _ => 0,
        };
        let ev = WindowEvent {
            event_type: EventType::KeyDown as u32,
            win_id: win_id as u32,
            code,
            x: 0,
            y: 0,
        };
        unsafe {
            core::ptr::write(event_ptr, ev);
        }
        return Ok(1);
    }

    Ok(0)
}

pub fn push_window_event(win_id: usize, ev: WindowEvent) {
    let mut table = WINDOW_TABLE.lock();
    if let Some(Some(win)) = table.get_mut(win_id) {
        if win.event_queue.len() < 32 {
            win.event_queue.push_back(ev);
        }
    }
}

pub fn get_window_pixels(win_id: usize) -> Option<*const u32> {
    let table = WINDOW_TABLE.lock();
    if let Some(Some(win)) = table.get(win_id) {
        return Some(win.user_virt_addr as *const u32);
    }
    for slot in table.iter().flatten() {
        if slot.id == win_id || table.iter().flatten().count() == 1 {
            return Some(slot.user_virt_addr as *const u32);
        }
    }
    None
}

pub fn destroy_window(win_id: usize) -> Result<(), &'static str> {
    let mut table = WINDOW_TABLE.lock();
    let win = match table.get_mut(win_id) {
        Some(slot) => slot.take().ok_or("Window not found")?,
        None => return Err("Invalid window id"),
    };

    let mut map_guard = crate::kernel::MAPPER.lock();
    if let Some(mapper) = map_guard.as_mut() {
        for i in 0..win.pages_count {
            let page_addr = VirtAddr::new(win.user_virt_addr + (i as u64) * PAGE_SIZE);
            let page: Page<Size4KiB> = Page::containing_address(page_addr);
            if let Ok((_, flush)) = mapper.unmap(page) {
                flush.flush();
            }
        }
    }

    Ok(())
}