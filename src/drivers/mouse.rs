use alloc::collections::VecDeque;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use spin::Mutex;
use crate::arch::x86_64::io::inb;
use crate::drivers::wm::{Rect, WINDOW_TITLEBAR_HEIGHT};

pub struct MouseQueue {
    inner: Mutex<VecDeque<u8>>,
}

impl MouseQueue {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push(&self, b: u8) {
        let mut q = self.inner.lock();
        if q.len() < 512 {
            q.push_back(b);
        }
    }

    pub fn pop(&self) -> Option<u8> {
        self.inner.lock().pop_front()
    }
}

impl core::ops::Deref for MouseQueue {
    type Target = Mutex<VecDeque<u8>>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub static MOUSE_QUEUE: MouseQueue = MouseQueue::new();

pub static MOUSE_X: AtomicUsize = AtomicUsize::new(400);
pub static MOUSE_Y: AtomicUsize = AtomicUsize::new(300);
pub static LEFT_BTN: AtomicBool = AtomicBool::new(false);
static OLD_LEFT: AtomicBool = AtomicBool::new(false);

static PACKET_INDEX: Mutex<usize> = Mutex::new(0);
static PACKET_DATA: Mutex<[u8; 3]> = Mutex::new([0; 3]);

pub static PENDING_APP_LAUNCH: Mutex<Option<String>> = Mutex::new(None);

pub const CURSOR: [&str; 15] = [
    "X              ",
    "XX             ",
    "X.X            ",
    "X..X           ",
    "X...X          ",
    "X....X         ",
    "X.....X        ",
    "X......X       ",
    "X.......X      ",
    "X.....XXXX     ",
    "X..X..X        ",
    "X.X X..X       ",
    "XX   X..X      ",
    "X     X..X     ",
    "       XX      ",
];

unsafe fn wait_write() {
    for _ in 0..10_000 {
        if (inb(0x64) & 2) == 0 { return; }
    }
}

unsafe fn wait_read() {
    for _ in 0..10_000 {
        if (inb(0x64) & 1) != 0 { return; }
    }
}

pub fn init() {
    use crate::arch::x86_64::io::outb;
    unsafe {
        wait_write();
        outb(0x64, 0xA8);
        wait_write();
        outb(0x64, 0x20);
        wait_read();
        let status = inb(0x60) | 2;
        wait_write();
        outb(0x64, 0x60);
        wait_write();
        outb(0x60, status);
        wait_write();
        outb(0x64, 0xD4);
        wait_write();
        outb(0x60, 0xF4);
        wait_read();
        let _ = inb(0x60);
    }
}

pub fn handle_byte(b: u8) {
    MOUSE_QUEUE.push(b);
}

pub fn process_mouse_queue() {
    while let Some(b) = MOUSE_QUEUE.pop() {
        process_single_byte(b);
    }
}

fn process_single_byte(b: u8) {
    let mut idx = PACKET_INDEX.lock();
    let mut data = PACKET_DATA.lock();

    if *idx == 0 && (b & 0x08) == 0 {
        return;
    }
    data[*idx] = b;
    *idx += 1;

    if *idx == 3 {
        *idx = 0;
        let flags = data[0];
        let mut dx = data[1] as i32;
        let mut dy = data[2] as i32;
        if (flags & 0x10) != 0 { dx |= !0xFF; }
        if (flags & 0x20) != 0 { dy |= !0xFF; }
        dy = -dy;

        let left = (flags & 0x01) != 0;
        drop(data);
        drop(idx);

        let just_pressed = left && !OLD_LEFT.load(Ordering::Relaxed);
        let just_released = !left && OLD_LEFT.load(Ordering::Relaxed);
        OLD_LEFT.store(left, Ordering::Relaxed);
        LEFT_BTN.store(left, Ordering::Relaxed);

        let cur_x = MOUSE_X.load(Ordering::Relaxed) as i32 + dx;
        let cur_y = MOUSE_Y.load(Ordering::Relaxed) as i32 + dy;
        let nx = cur_x.clamp(0, (crate::drivers::gfx::SCREEN_WIDTH - 16) as i32) as usize;
        let ny = cur_y.clamp(0, (crate::drivers::gfx::SCREEN_HEIGHT - 16) as i32) as usize;

        MOUSE_X.store(nx, Ordering::Relaxed);
        MOUSE_Y.store(ny, Ordering::Relaxed);

        if just_released {
            let mut wm = crate::drivers::wm::WM.lock();
            wm.handle_up();
        }

        if crate::drivers::gfx::IS_DESKTOP_VIEW.load(Ordering::Relaxed) {
            if just_pressed {
                if nx >= 90 && nx <= 175 && ny >= 4 && ny <= 26 {
                    crate::drivers::sound::click();
                    let cur = crate::drivers::gfx::APPS_MENU_OPEN.load(Ordering::Relaxed);
                    crate::drivers::gfx::APPS_MENU_OPEN.store(!cur, Ordering::Relaxed);
                    let menu_rect = Rect::new(crate::drivers::gfx::MENU_X, crate::drivers::gfx::MENU_Y, crate::drivers::gfx::MENU_W + 4, 244);
                    crate::drivers::gfx::mark_dirty(menu_rect);
                    crate::drivers::gfx::mark_dirty(Rect::new(90, 4, 85, 22));
                    return;
                }

                if nx >= 185 && nx <= 295 && ny >= 4 && ny <= 26 {
                    crate::drivers::sound::click();
                    crate::drivers::wm::get_terminal_descriptor();
                    return;
                }

                if crate::drivers::gfx::APPS_MENU_OPEN.load(Ordering::Relaxed) {
                    let mx = crate::drivers::gfx::MENU_X;
                    let my = crate::drivers::gfx::MENU_Y + WINDOW_TITLEBAR_HEIGHT;
                    let mw = crate::drivers::gfx::MENU_W;
                    let mh = crate::drivers::gfx::MENU_ITEM_H;

                    if nx >= mx && nx <= mx + mw && ny >= my && ny <= my + 8 * mh {
                        let row = (ny - my) / mh;
                        let mut target_app = None;
                        {
                            let fs = crate::sys::fs::VFS.lock();
                            if let Ok(bin_ino) = fs.resolve_path(0, "/bin") {
                                if let Ok(entries) = fs.read_dir(bin_ino) {
                                    let mut real_entries = entries.into_iter().filter(|(_, name, _)| name != "." && name != "..");
                                    if let Some((_, name, _)) = real_entries.nth(row) {
                                        target_app = Some(alloc::format!("/bin/{}", name));
                                    }
                                }
                            }
                        }

                        if let Some(app_path) = target_app {
                            crate::drivers::sound::click();
                            crate::drivers::gfx::APPS_MENU_OPEN.store(false, Ordering::Relaxed);
                            let menu_rect = Rect::new(crate::drivers::gfx::MENU_X, crate::drivers::gfx::MENU_Y, crate::drivers::gfx::MENU_W + 4, 244);
                            crate::drivers::gfx::mark_dirty(menu_rect);
                            crate::usr::bin::execute_binary(0, &app_path, &[]);
                            return;
                        }
                    } else {
                        crate::drivers::gfx::APPS_MENU_OPEN.store(false, Ordering::Relaxed);
                        let menu_rect = Rect::new(crate::drivers::gfx::MENU_X, crate::drivers::gfx::MENU_Y, crate::drivers::gfx::MENU_W + 4, 244);
                        crate::drivers::gfx::mark_dirty(menu_rect);
                    }
                }

                let mut launch_target = None;
                {
                    let shortcuts = crate::drivers::gfx::SHORTCUTS.lock();
                    for sc in shortcuts.iter() {
                        if nx >= sc.x && nx <= sc.x + 48 && ny >= sc.y && ny <= sc.y + 48 {
                            launch_target = Some(alloc::format!("/home/Desktop/{}", sc.name));
                            break;
                        }
                    }
                }

                if let Some(app_path) = launch_target {
                    crate::drivers::sound::click();
                    crate::usr::bin::execute_binary(0, &app_path, &[]);
                    return;
                }

                let mut wm = crate::drivers::wm::WM.lock();
                wm.handle_down(nx, ny);
            }
        }
    }
}