use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use font8x8::UnicodeFonts;
use spin::{Lazy, Mutex};
use x86_64::instructions::port::Port;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, Page, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};
use crate::drivers::wm::{Rect, WINDOW_TITLEBAR_HEIGHT};

pub const SCREEN_WIDTH: usize = 800;
pub const SCREEN_HEIGHT: usize = 600;
pub const FB_VIRT_ADDR: u64 = 0x5555_0000_0000;
pub const BB_VIRT_ADDR: u64 = 0x5555_1000_0000;
pub const BG_VIRT_ADDR: u64 = 0x5555_4000_0000;

pub const MENU_X: usize = 90;
pub const MENU_Y: usize = 30;
pub const MENU_W: usize = 200;
pub const MENU_ITEM_H: usize = 24;

pub static IS_GRAPHICS_MODE: AtomicBool = AtomicBool::new(false);
pub static IS_DESKTOP_VIEW: AtomicBool = AtomicBool::new(false);
pub static APPS_MENU_OPEN: AtomicBool = AtomicBool::new(false);

pub static PREV_CURSOR_X: AtomicUsize = AtomicUsize::new(400);
pub static PREV_CURSOR_Y: AtomicUsize = AtomicUsize::new(300);
pub static PENDING_DIRTY: Mutex<Option<Rect>> = Mutex::new(None);
static LAST_SEC: AtomicU64 = AtomicU64::new(0);
static CACHED_TIME_STR: Mutex<([u8; 32], usize)> = Mutex::new(([0u8; 32], 0));

#[derive(Clone)]
pub struct DesktopShortcut {
    pub name: String,
    pub x: usize,
    pub y: usize,
}

pub struct TrueTypeFont {
    pub data: Vec<u8>,
}

pub static SHORTCUTS: Lazy<Mutex<Vec<DesktopShortcut>>> = Lazy::new(|| Mutex::new(Vec::new()));
pub static SYSTEM_FONT: Lazy<Mutex<Option<TrueTypeFont>>> = Lazy::new(|| Mutex::new(None));

fn pci_find_bga_bar0() -> u32 {
    let mut config_addr = Port::<u32>::new(0xCF8);
    let mut config_data = Port::<u32>::new(0xCFC);

    for dev in 0..32 {
        let addr = 0x8000_0000 | (dev << 11);
        unsafe {
            config_addr.write(addr);
            let id = config_data.read();
            let vendor = (id & 0xFFFF) as u16;
            let device = (id >> 16) as u16;
            if vendor == 0x1234 && device == 0x1111 {
                config_addr.write(addr | 0x04);
                let cmd = config_data.read();
                config_data.write(cmd | 0x07);

                config_addr.write(addr | 0x10);
                return config_data.read() & 0xFFFF_FFF0;
            }
        }
    }
    0xFD00_0000
}

unsafe fn bga_write(idx: u16, val: u16) {
    let mut p_idx = Port::<u16>::new(0x01CE);
    let mut p_val = Port::<u16>::new(0x01CF);
    p_idx.write(idx);
    p_val.write(val);
}

pub fn init(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    let bar0 = pci_find_bga_bar0();
    let fb_size = SCREEN_WIDTH * SCREEN_HEIGHT * 4;
    let page_count = (fb_size + 4095) / 4096;

    for i in 0..page_count {
        let p = Page::containing_address(VirtAddr::new(FB_VIRT_ADDR + (i * 4096) as u64));
        let f = PhysFrame::containing_address(PhysAddr::new(bar0 as u64 + (i * 4096) as u64));
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe { let _ = mapper.map_to(p, f, flags, frame_allocator).map(|m| m.flush()); }
    }

    for i in 0..page_count {
        let p = Page::containing_address(VirtAddr::new(BB_VIRT_ADDR + (i * 4096) as u64));
        if let Ok(f) = frame_allocator.allocate_frame().ok_or(()) {
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
            unsafe { let _ = mapper.map_to(p, f, flags, frame_allocator).map(|m| m.flush()); }
        }
    }

    for i in 0..page_count {
        let p = Page::containing_address(VirtAddr::new(BG_VIRT_ADDR + (i * 4096) as u64));
        if let Ok(f) = frame_allocator.allocate_frame().ok_or(()) {
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
            unsafe { let _ = mapper.map_to(p, f, flags, frame_allocator).map(|m| m.flush()); }
        }
    }

    let t = crate::drivers::cmos::read();
    let mut time_str = [0u8; 32];
    let len = write_clock_str(&mut time_str, t.hour, t.minute, t.second, 0);
    *CACHED_TIME_STR.lock() = (time_str, len);

    set_video_mode(SCREEN_WIDTH as u16, SCREEN_HEIGHT as u16);
    IS_GRAPHICS_MODE.store(true, Ordering::Relaxed);
    prerender_desktop_bg();
}

fn prerender_desktop_bg() {
    let bg = BG_VIRT_ADDR as *mut u32;
    for y in 0..SCREEN_HEIGHT {
        let r = 10 + ((y * 15) / SCREEN_HEIGHT) as u32;
        let g = 8 + ((y * 10) / SCREEN_HEIGHT) as u32;
        let b = 15 + ((y * 25) / SCREEN_HEIGHT) as u32;
        let col = (r << 16) | (g << 8) | b;
        unsafe {
            let row = bg.add(y * SCREEN_WIDTH);
            for x in 0..SCREEN_WIDTH {
                *row.add(x) = col;
            }
        }
    }

    for y in 0..32 {
        unsafe {
            let row = bg.add(y * SCREEN_WIDTH);
            for x in 0..SCREEN_WIDTH {
                *row.add(x) = 0x000F172A;
            }
        }
    }

    draw_string_on_ptr(bg, 16, 12, "redKiteOS (UwU)", 0x00FB7185);
}

fn draw_string_on_ptr(ptr: *mut u32, x: usize, y: usize, text: &str, color: u32) {
    let mut cx = x;
    for c in text.chars() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(c) {
            for (yo, &b) in glyph.iter().enumerate() {
                for xo in 0..8 {
                    if (b & (1 << xo)) != 0 {
                        unsafe {
                            let p = ptr.add((y + yo) * SCREEN_WIDTH + (cx + xo));
                            *p = color;
                        }
                    }
                }
            }
        }
        cx += 8;
    }
}

pub fn set_video_mode(w: u16, h: u16) {
    unsafe {
        bga_write(4, 0x00);
        bga_write(1, w);
        bga_write(2, h);
        bga_write(3, 32);
        bga_write(4, 0x41);
    }
}

pub fn alpha_blend(fg: u32, bg: u32) -> u32 {
    let a = (fg >> 24) & 0xFF;
    if a == 0 {
        return bg;
    }
    if a == 255 {
        return fg & 0x00FFFFFF;
    }
    let inv_a = 255 - a;

    let fg_r = (fg >> 16) & 0xFF;
    let fg_g = (fg >> 8) & 0xFF;
    let fg_b = fg & 0xFF;

    let bg_r = (bg >> 16) & 0xFF;
    let bg_g = (bg >> 8) & 0xFF;
    let bg_b = bg & 0xFF;

    let r = (fg_r * a + bg_r * inv_a) >> 8;
    let g = (fg_g * a + bg_g * inv_a) >> 8;
    let b = (fg_b * a + bg_b * inv_a) >> 8;

    (r << 16) | (g << 8) | b
}

pub fn flush_rect(rect: Rect) {
    let bb = BB_VIRT_ADDR as *const u32;
    let fb = FB_VIRT_ADDR as *mut u32;

    for r in rect.y..(rect.y + rect.height).min(SCREEN_HEIGHT) {
        let off = r * SCREEN_WIDTH + rect.x;
        let count = rect.width.min(SCREEN_WIDTH.saturating_sub(rect.x));
        if count > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(bb.add(off), fb.add(off), count);
            }
        }
    }
}

pub fn draw_rect(x: usize, y: usize, w: usize, h: usize, color: u32) {
    let bb = BB_VIRT_ADDR as *mut u32;
    let alpha = (color >> 24) & 0xFF;

    for r in y..(y + h).min(SCREEN_HEIGHT) {
        let ptr = unsafe { bb.add(r * SCREEN_WIDTH) };
        for c in x..(x + w).min(SCREEN_WIDTH) {
            unsafe {
                if alpha == 255 || alpha == 0 {
                    *ptr.add(c) = color & 0x00FFFFFF;
                } else {
                    let bg = *ptr.add(c);
                    *ptr.add(c) = alpha_blend(color, bg);
                }
            }
        }
    }
}

pub fn draw_string(x: usize, y: usize, text: &str, color: u32) {
    let bb = BB_VIRT_ADDR as *mut u32;
    draw_string_on_ptr(bb, x, y, text, color);
}

pub fn restore_cursor_area(x: usize, y: usize) {
    unsafe {
        for r in 0..15 {
            let py = y + r;
            if py < SCREEN_HEIGHT {
                let off = py * SCREEN_WIDTH + x;
                let cnt = 15.min(SCREEN_WIDTH.saturating_sub(x));
                let mut src_ptr = (BB_VIRT_ADDR as *const u32).add(off);
                let mut dst_ptr = (FB_VIRT_ADDR as *mut u32).add(off);
                for _ in 0..cnt {
                    *dst_ptr = *src_ptr;
                    src_ptr = src_ptr.add(1);
                    dst_ptr = dst_ptr.add(1);
                }
            }
        }
    }
}

pub fn draw_cursor_direct(x: usize, y: usize) {
    for (r, row) in crate::drivers::mouse::CURSOR.iter().enumerate() {
        let py = y + r;
        if py < SCREEN_HEIGHT {
            for (c, ch) in row.chars().enumerate() {
                let px = x + c;
                if px < SCREEN_WIDTH {
                    match ch {
                        'X' => unsafe {
                            let p = (FB_VIRT_ADDR as *mut u32).add(py * SCREEN_WIDTH + px);
                            core::ptr::write_volatile(p, 0x00020617);
                        },
                        '.' => unsafe {
                            let p = (FB_VIRT_ADDR as *mut u32).add(py * SCREEN_WIDTH + px);
                            core::ptr::write_volatile(p, 0x00F8FAFC);
                        },
                        _ => {}
                    }
                }
            }
        }
    }
}

pub fn mark_dirty(rect: Rect) {
    let mut pending = PENDING_DIRTY.lock();
    *pending = match *pending {
        Some(prev) => Some(prev.union(&rect)),
        None => Some(rect),
    };
}

pub fn compositor_tick() {
    if !IS_DESKTOP_VIEW.load(Ordering::Relaxed) {
        return;
    }

    let uptime = crate::kernel::timer::ticks() / 18;
    if uptime != LAST_SEC.load(Ordering::Relaxed) {
        LAST_SEC.store(uptime, Ordering::Relaxed);
        let t = crate::drivers::cmos::read();
        let mut time_str = [0u8; 32];
        let len = write_clock_str(&mut time_str, t.hour, t.minute, t.second, uptime);
        *CACHED_TIME_STR.lock() = (time_str, len);
        mark_dirty(Rect::new(SCREEN_WIDTH - 220, 0, 220, 32));
    }

    crate::drivers::mouse::process_mouse_queue();

    let mx = crate::drivers::mouse::MOUSE_X.load(Ordering::Relaxed);
    let my = crate::drivers::mouse::MOUSE_Y.load(Ordering::Relaxed);
    let px = PREV_CURSOR_X.load(Ordering::Relaxed);
    let py = PREV_CURSOR_Y.load(Ordering::Relaxed);
    let is_left = crate::drivers::mouse::LEFT_BTN.load(Ordering::Relaxed);

    let mut dirty_from_drag = None;
    if is_left {
        let mut wm = crate::drivers::wm::WM.lock();
        if wm.dragging.is_some() {
            dirty_from_drag = wm.handle_drag(mx, my);
        }
    }

    let pending_dirty = PENDING_DIRTY.lock().take();

    let combined_dirty = match (dirty_from_drag, pending_dirty) {
        (Some(d1), Some(d2)) => Some(d1.union(&d2)),
        (Some(d1), None) => Some(d1),
        (None, Some(d2)) => Some(d2),
        (None, None) => None,
    };

    if let Some(dirty_rect) = combined_dirty {
        PREV_CURSOR_X.store(mx, Ordering::Relaxed);
        PREV_CURSOR_Y.store(my, Ordering::Relaxed);

        let cursor_old = Rect::new(px, py, 16, 16);
        let cursor_new = Rect::new(mx, my, 16, 16);
        let total_dirty = dirty_rect.union(&cursor_old).union(&cursor_new).clamp(
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );
        redraw_rect(total_dirty);
        draw_cursor_direct(mx, my);
    } else if mx != px || my != py {
        PREV_CURSOR_X.store(mx, Ordering::Relaxed);
        PREV_CURSOR_Y.store(my, Ordering::Relaxed);

        restore_cursor_area(px, py);
        draw_cursor_direct(mx, my);
    }
}

pub fn scan_desktop_shortcuts() {
    let mut list = SHORTCUTS.lock();
    list.clear();

    let fs = crate::sys::fs::VFS.lock();
    if let Ok(desk_ino) = fs.resolve_path(0, "/home/Desktop") {
        if let Ok(entries) = fs.read_dir(desk_ino) {
            let mut col = 0;
            let mut row = 0;
            for (_, name, _) in entries {
                if name != "." && name != ".." {
                    let icon_x = 24 + col * 84;
                    let icon_y = 44 + row * 84;

                    if icon_x + 48 < SCREEN_WIDTH && icon_y + 64 < SCREEN_HEIGHT {
                        list.push(DesktopShortcut {
                            name,
                            x: icon_x,
                            y: icon_y,
                        });
                    }

                    row += 1;
                    if 44 + row * 84 + 64 >= SCREEN_HEIGHT {
                        row = 0;
                        col += 1;
                    }
                }
            }
        }
    }
}

pub fn redraw_rect(rect: Rect) {
    let rect = rect.clamp(SCREEN_WIDTH, SCREEN_HEIGHT);
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let bg = BG_VIRT_ADDR as *const u32;
    let bb = BB_VIRT_ADDR as *mut u32;
    for r in rect.y..(rect.y + rect.height) {
        let off = r * SCREEN_WIDTH + rect.x;
        let count = rect.width;
        if count > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(bg.add(off), bb.add(off), count);
            }
        }
    }

    let top_bar_rect = Rect::new(0, 0, SCREEN_WIDTH, 32);
    if rect.intersects(&top_bar_rect) {
        draw_rect(90, 4, 85, 22, 0xFFE11D48);
        draw_string(102, 8, "[ Apps ]", 0x00FFFFFF);

        draw_rect(185, 4, 110, 22, 0xFF334155);
        draw_string(197, 8, ">_ Terminal", 0x00F1F5F9);

        let (cached_buf, len) = *CACHED_TIME_STR.lock();
        if len > 0 {
            if let Ok(s) = core::str::from_utf8(&cached_buf[..len]) {
                draw_string(SCREEN_WIDTH - 220, 8, s, 0x0038BDF8);
            }
        }
    }

    let shortcuts = SHORTCUTS.lock();
    for sc in shortcuts.iter() {
        let sc_rect = Rect::new(sc.x, sc.y, 48, 64);
        if rect.intersects(&sc_rect) {
            draw_rect(sc.x + 2, sc.y + 2, 48, 48, 0x88090D16);
            draw_rect(sc.x, sc.y, 48, 48, 0xFF1E293B);
            draw_rect(sc.x, sc.y, 48, 2, 0xFFE11D48);
            draw_rect(sc.x, sc.y + 46, 48, 2, 0xFFE11D48);
            draw_rect(sc.x, sc.y, 2, 48, 0xFFE11D48);
            draw_rect(sc.x + 46, sc.y, 2, 48, 0xFFE11D48);
            draw_string(sc.x + 16, sc.y + 18, "#", 0x0038BDF8);
            draw_string(sc.x + 4, sc.y + 54, &sc.name, 0x00F8FAFC);
        }
    }
    drop(shortcuts);

    crate::drivers::wm::WM.lock().render_rect(rect);

    if APPS_MENU_OPEN.load(Ordering::Relaxed) {
        let menu_rect = Rect::new(MENU_X, MENU_Y, MENU_W + 4, 244);
        if rect.intersects(&menu_rect) {
            draw_apps_menu();
        }
    }

    flush_rect(rect);
}

pub fn render_desktop_scene() {
    let count = SCREEN_WIDTH * SCREEN_HEIGHT;
    unsafe {
        core::ptr::copy_nonoverlapping(
            BG_VIRT_ADDR as *const u32,
            BB_VIRT_ADDR as *mut u32,
            count,
        );
    }

    draw_rect(90, 4, 85, 22, 0xFFE11D48);
    draw_string(102, 8, "[ Apps ]", 0x00FFFFFF);

    draw_rect(185, 4, 110, 22, 0xFF334155);
    draw_string(197, 8, ">_ Terminal", 0x00F1F5F9);

    let (cached_buf, len) = *CACHED_TIME_STR.lock();
    if len > 0 {
        if let Ok(s) = core::str::from_utf8(&cached_buf[..len]) {
            draw_string(SCREEN_WIDTH - 220, 8, s, 0x0038BDF8);
        }
    }

    let shortcuts = SHORTCUTS.lock();
    for sc in shortcuts.iter() {
        draw_rect(sc.x + 2, sc.y + 2, 48, 48, 0x88090D16);
        draw_rect(sc.x, sc.y, 48, 48, 0xFF1E293B);
        draw_rect(sc.x, sc.y, 48, 2, 0xFFE11D48);
        draw_rect(sc.x, sc.y + 46, 48, 2, 0xFFE11D48);
        draw_rect(sc.x, sc.y, 2, 48, 0xFFE11D48);
        draw_rect(sc.x + 46, sc.y, 2, 48, 0xFFE11D48);
        draw_string(sc.x + 16, sc.y + 18, "#", 0x0038BDF8);
        draw_string(sc.x + 4, sc.y + 54, &sc.name, 0x00F8FAFC);
    }
    drop(shortcuts);

    crate::drivers::wm::WM.lock().render();

    if APPS_MENU_OPEN.load(Ordering::Relaxed) {
        draw_apps_menu();
    }

    flush_rect(Rect::new(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT));

    let mx = crate::drivers::mouse::MOUSE_X.load(Ordering::Relaxed);
    let my = crate::drivers::mouse::MOUSE_Y.load(Ordering::Relaxed);
    PREV_CURSOR_X.store(mx, Ordering::Relaxed);
    PREV_CURSOR_Y.store(my, Ordering::Relaxed);
    draw_cursor_direct(mx, my);
}

fn draw_apps_menu() {
    let menu_x = MENU_X;
    let menu_y = MENU_Y;
    let menu_w = MENU_W;
    let menu_h = 240;

    draw_rect(menu_x + 4, menu_y + 4, menu_w, menu_h, 0x88020617);
    draw_rect(menu_x, menu_y, menu_w, menu_h, 0xFF0F172A);
    draw_rect(menu_x, menu_y, menu_w, WINDOW_TITLEBAR_HEIGHT, 0xFFBE123C);
    draw_string(menu_x + 10, menu_y + 6, "Installed Applications", 0x00FFFFFF);

    let fs = crate::sys::fs::VFS.lock();
    let mut has_entries = false;
    if let Ok(bin_ino) = fs.resolve_path(0, "/bin") {
        if let Ok(entries) = fs.read_dir(bin_ino) {
            let mut row = 0;
            for (_, name, _) in entries {
                if name != "." && name != ".." && row < 8 {
                    has_entries = true;
                    let item_y = menu_y + WINDOW_TITLEBAR_HEIGHT + 2 + row * MENU_ITEM_H;
                    draw_rect(menu_x + 6, item_y, menu_w - 12, 20, 0xFF1E293B);
                    draw_string(menu_x + 14, item_y + 4, &name, 0x00F1F5F9);
                    row += 1;
                }
            }
        }
    }

    if !has_entries {
        draw_string(menu_x + 10, menu_y + 40, "(No binaries in /bin)", 0x0064748B);
    }
}

fn write_clock_str(buf: &mut [u8; 32], h: u8, m: u8, s: u8, up: u64) -> usize {
    use core::fmt::Write;
    struct BufWriter<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }
    impl<'a> Write for BufWriter<'a> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for b in s.bytes() {
                if self.pos < self.buf.len() {
                    self.buf[self.pos] = b;
                    self.pos += 1;
                }
            }
            Ok(())
        }
    }
    let mut w = BufWriter { buf, pos: 0 };
    let _ = write!(w, "[{:02}:{:02}:{:02} | {}s]", h, m, s, up);
    w.pos
}

pub fn open_desktop() {
    scan_desktop_shortcuts();
    IS_DESKTOP_VIEW.store(true, Ordering::Relaxed);
    render_desktop_scene();
}