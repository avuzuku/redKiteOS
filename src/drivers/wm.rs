use font8x8::UnicodeFonts;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::{Lazy, Mutex};
use crate::drivers::gfx::alpha_blend;

pub const WINDOW_TITLEBAR_HEIGHT: usize = 28;
pub const WINDOW_SHADOW_OFFSET: usize = 6;
pub const TOP_PANEL_HEIGHT: usize = 32;
pub const SNAP_THRESHOLD: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self { x, y, width, height }
    }

    pub fn union(&self, other: &Rect) -> Rect {
        if self.width == 0 || self.height == 0 {
            return *other;
        }
        if other.width == 0 || other.height == 0 {
            return *self;
        }
        let min_x = self.x.min(other.x);
        let min_y = self.y.min(other.y);
        let max_x = (self.x + self.width).max(other.x + other.width);
        let max_y = (self.y + self.height).max(other.y + other.height);
        Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }

    pub fn clamp(&self, max_w: usize, max_h: usize) -> Rect {
        let x = self.x.min(max_w);
        let y = self.y.min(max_h);
        let width = self.width.min(max_w.saturating_sub(x));
        let height = self.height.min(max_h.saturating_sub(y));
        Rect { x, y, width, height }
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

pub struct Window {
    pub id: usize,
    pub title: String,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub visible: bool,
    pub buffer: Vec<u32>,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub input_queue: VecDeque<u8>,
    pub fg_color: u32,
    pub escape_state: u8,
    pub escape_val: u32,
}

impl Window {
    pub fn outer_rect(&self) -> Rect {
        Rect::new(
            self.x,
            self.y,
            self.width + WINDOW_SHADOW_OFFSET + 4,
            self.height + WINDOW_SHADOW_OFFSET + 1,
        )
    }

    pub fn client_rect(&self) -> Rect {
        Rect::new(
            self.x,
            self.y + WINDOW_TITLEBAR_HEIGHT,
            self.width,
            self.height.saturating_sub(WINDOW_TITLEBAR_HEIGHT),
        )
    }

    pub fn handle_key(&mut self, key: u8) {
        self.input_queue.push_back(key);
    }

    pub fn write_byte(&mut self, b: u8) -> bool {
        let client_h = self.height.saturating_sub(WINDOW_TITLEBAR_HEIGHT);
        let max_cols = self.width.saturating_sub(16) / 8;
        let max_rows = client_h.saturating_sub(16) / 10;
        if max_cols == 0 || max_rows == 0 {
            return false;
        }

        if self.escape_state == 1 {
            if b == b'[' {
                self.escape_state = 2;
                self.escape_val = 0;
            } else {
                self.escape_state = 0;
            }
            return false;
        } else if self.escape_state == 2 {
            if b >= b'0' && b <= b'9' {
                self.escape_val = self.escape_val * 10 + (b - b'0') as u32;
            } else if b == b';' {
                self.escape_val = 0;
            } else if b == b'm' {
                match self.escape_val {
                    0 => self.fg_color = 0x00F8FAFC,
                    30 => self.fg_color = 0x00000000,
                    31 => self.fg_color = 0x00E11D48,
                    32 => self.fg_color = 0x004ADE80,
                    33 => self.fg_color = 0x00FACC15,
                    34 => self.fg_color = 0x003B82F6,
                    35 => self.fg_color = 0x00D946EF,
                    36 => self.fg_color = 0x0038BDF8,
                    37 => self.fg_color = 0x00F8FAFC,
                    90 => self.fg_color = 0x0094A3B8,
                    _ => {}
                }
                self.escape_state = 0;
            } else {
                self.escape_state = 0;
            }
            return false;
        }

        if b == 0x1B {
            self.escape_state = 1;
            return false;
        }

        let mut scrolled = false;

        match b {
            0x08 => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                    let x = 8 + self.cursor_col * 8;
                    let y = 8 + self.cursor_row * 10;
                    for r in 0..10 {
                        let py = y + r;
                        if py < client_h {
                            for c in 0..8 {
                                let px = x + c;
                                if px < self.width {
                                    self.buffer[py * self.width + px] = 0x00141414;
                                }
                            }
                        }
                    }
                }
            }
            b'\n' => {
                self.cursor_col = 0;
                if self.cursor_row + 1 < max_rows {
                    self.cursor_row += 1;
                } else {
                    self.scroll();
                    scrolled = true;
                }
            }
            ch if (0x20..=0x7E).contains(&ch) => {
                let x = 8 + self.cursor_col * 8;
                let y = 8 + self.cursor_row * 10;
                if let Some(glyph) = font8x8::BASIC_FONTS.get(ch as char) {
                    for (yo, &b_byte) in glyph.iter().enumerate() {
                        let py = y + yo;
                        if py < client_h {
                            for xo in 0..8 {
                                let px = x + xo;
                                if px < self.width && (b_byte & (1 << xo)) != 0 {
                                    self.buffer[py * self.width + px] = self.fg_color;
                                }
                            }
                        }
                    }
                }
                self.cursor_col += 1;
                if self.cursor_col >= max_cols {
                    self.cursor_col = 0;
                    if self.cursor_row + 1 < max_rows {
                        self.cursor_row += 1;
                    } else {
                        self.scroll();
                        scrolled = true;
                    }
                }
            }
            _ => {}
        }

        scrolled
    }

    pub fn write_str(&mut self, s: &str) -> bool {
        let mut any_scrolled = false;
        for b in s.bytes() {
            if self.write_byte(b) {
                any_scrolled = true;
            }
        }
        any_scrolled
    }

    fn scroll(&mut self) {
        let client_h = self.height.saturating_sub(WINDOW_TITLEBAR_HEIGHT);
        let line_h = 10;
        let stride = self.width;
        let max_rows = client_h.saturating_sub(16) / line_h;
        if max_rows <= 1 {
            return;
        }

        let start_y = 8;
        let copy_pixels = (max_rows - 1) * line_h * stride;

        unsafe {
            let dst = self.buffer.as_mut_ptr().add(start_y * stride);
            let src = self.buffer.as_ptr().add((start_y + line_h) * stride);
            core::ptr::copy(src, dst, copy_pixels);
        }

        let last_line_y = start_y + (max_rows - 1) * line_h;
        for y in last_line_y..(last_line_y + line_h).min(client_h) {
            let row_start = y * stride;
            for x in 0..stride {
                self.buffer[row_start + x] = 0x00141414;
            }
        }
    }
}

pub struct WindowManager {
    pub windows: Vec<Window>,
    pub focused_idx: Option<usize>,
    pub dragging: Option<usize>,
    pub drag_x: usize,
    pub drag_y: usize,
}

impl WindowManager {
    pub fn new() -> Self {
        let mut wm = WindowManager {
            windows: Vec::new(),
            focused_idx: None,
            dragging: None,
            drag_x: 0,
            drag_y: 0,
        };

        let win_w = 400;
        let win_h = 300;
        let win_x = (crate::drivers::gfx::SCREEN_WIDTH.saturating_sub(win_w + WINDOW_SHADOW_OFFSET + 4)) / 2;
        let win_y = TOP_PANEL_HEIGHT + 16;
        let id = wm.create_window("Welcome to redKiteOS", win_x, win_y, win_w, win_h);

        let client_h = win_h.saturating_sub(WINDOW_TITLEBAR_HEIGHT);
        if let Some(w) = wm.windows.iter_mut().find(|w| w.id == id) {
            for r in 0..client_h {
                let shade = 18 + ((r * 14) / client_h) as u32;
                let bg_col = (shade << 16) | (shade << 8) | (shade + 8);
                for c in 0..win_w {
                    w.buffer[r * win_w + c] = bg_col;
                }
            }

            draw_text_to_buffer(&mut w.buffer, win_w, 24, 30, "Welcome to redKiteOS!", 0x00F43F5E);
            draw_text_to_buffer(&mut w.buffer, win_w, 24, 60, "Async 60 FPS Compositor Ready~", 0x0038BDF8);
            draw_text_to_buffer(&mut w.buffer, win_w, 24, 90, "Try dragging this window by header!", 0x00CBD5E1);
            draw_text_to_buffer(&mut w.buffer, win_w, 24, 115, "Click [X] button to close.", 0x0094A3B8);
            draw_text_to_buffer(&mut w.buffer, win_w, 24, 150, "Architecture: x86_64 POSIX-like", 0x00FACC15);
            draw_text_to_buffer(&mut w.buffer, win_w, 24, 175, "Theme: Dark Zinc & Crimson", 0x00E11D48);
            draw_text_to_buffer(&mut w.buffer, win_w, 24, 210, "Status: Active Window Focused Nya~", 0x004ADE80);

            w.visible = true;
        }
        wm.focused_idx = Some(0);

        wm
    }

    pub fn create_window(&mut self, title: &str, x: usize, y: usize, width: usize, height: usize) -> usize {
        let id = self.windows.len();
        let client_h = height.saturating_sub(WINDOW_TITLEBAR_HEIGHT);
        let buffer = vec![0x00141414; width * client_h];

        self.windows.push(Window {
            id,
            title: String::from(title),
            x,
            y,
            width,
            height,
            visible: false,
            buffer,
            cursor_col: 0,
            cursor_row: 0,
            input_queue: VecDeque::new(),
            fg_color: 0x00F8FAFC,
            escape_state: 0,
            escape_val: 0,
        });
        id
    }

    pub fn set_visible(&mut self, id: usize, vis: bool) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            w.visible = vis;
            if vis {
                self.focus_window(id);
            }
        }
    }

    pub fn focus_window(&mut self, id: usize) -> Option<Rect> {
        let old_top_rect = self.windows.last().map(|w| w.outer_rect());
        if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
            let win = self.windows.remove(pos);
            let new_top_rect = win.outer_rect();
            self.windows.push(win);
            self.focused_idx = Some(self.windows.len() - 1);

            let dirty = match old_top_rect {
                Some(r) => r.union(&new_top_rect),
                None => new_top_rect,
            };
            return Some(dirty);
        }
        None
    }

    pub fn handle_key(&mut self, key: u8) {
        if let Some(idx) = self.focused_idx {
            if let Some(win) = self.windows.get_mut(idx) {
                if win.visible {
                    win.handle_key(key);
                }
            }
        }
    }

    pub fn render(&self) {
        self.render_rect(Rect::new(0, 0, crate::drivers::gfx::SCREEN_WIDTH, crate::drivers::gfx::SCREEN_HEIGHT));
    }

    pub fn render_rect(&self, clip: Rect) {
        for (i, win) in self.windows.iter().enumerate() {
            if !win.visible { continue; }
            let outer = win.outer_rect();
            if !outer.intersects(&clip) { continue; }

            let is_focused = self.focused_idx == Some(i);
            let header = if is_focused { 0xFFDC2626 } else { 0xFF450A0A };
            let border_color = if is_focused { 0xFFE11D48 } else { 0xFF334155 };

            draw_rect_clipped(win.x + win.width, win.y + 3, WINDOW_SHADOW_OFFSET, win.height, 0x40000000, clip);
            draw_rect_clipped(win.x + 3, win.y + win.height, win.width + WINDOW_SHADOW_OFFSET, WINDOW_SHADOW_OFFSET, 0x40000000, clip);

            draw_rect_clipped(win.x, win.y, win.width, win.height, 0xFF141414, clip);
            draw_rect_clipped(win.x, win.y, win.width, WINDOW_TITLEBAR_HEIGHT, header, clip);

            let title_rect = Rect::new(win.x + 10, win.y + 7, win.title.len() * 8, 8);
            if title_rect.intersects(&clip) {
                crate::drivers::gfx::draw_string(win.x + 10, win.y + 7, &win.title, 0x00FFFFFF);
            }

            let btn_rect = Rect::new(win.x + win.width - 22, win.y + 5, 18, 18);
            if btn_rect.intersects(&clip) {
                draw_rect_clipped(win.x + win.width - 22, win.y + 5, 18, 18, 0xFF7F1D1D, clip);
                crate::drivers::gfx::draw_string(win.x + win.width - 17, win.y + 6, "X", 0x00FFFFFF);
            }

            let client_h = win.height.saturating_sub(WINDOW_TITLEBAR_HEIGHT);
            let bb = crate::drivers::gfx::BB_VIRT_ADDR as *mut u32;
            let shmem_pixels = crate::kernel::ipc::get_window_pixels(win.id);

            for r in 0..client_h {
                let dy = win.y + WINDOW_TITLEBAR_HEIGHT + r;
                if dy < crate::drivers::gfx::SCREEN_HEIGHT && dy >= clip.y && dy < clip.y + clip.height {
                    let start_x = win.x.max(clip.x);
                    let end_x = (win.x + win.width).min(clip.x + clip.width).min(crate::drivers::gfx::SCREEN_WIDTH);
                    if start_x < end_x {
                        let dst = unsafe { bb.add(dy * crate::drivers::gfx::SCREEN_WIDTH + start_x) };
                        let offset = r * win.width + (start_x - win.x);
                        let len = end_x - start_x;
                        unsafe {
                            if let Some(ptr) = shmem_pixels {
                                core::ptr::copy_nonoverlapping((ptr as *const u32).add(offset), dst, len);
                            } else {
                                core::ptr::copy_nonoverlapping(win.buffer.as_ptr().add(offset), dst, len);
                            }
                        }
                    }
                }
            }

            draw_rect_clipped(win.x, win.y, win.width, 1, border_color, clip);
            draw_rect_clipped(win.x, win.y + win.height.saturating_sub(1), win.width, 1, border_color, clip);
            draw_rect_clipped(win.x, win.y, 1, win.height, border_color, clip);
            draw_rect_clipped(win.x + win.width.saturating_sub(1), win.y, 1, win.height, border_color, clip);
            draw_rect_clipped(win.x, win.y + WINDOW_TITLEBAR_HEIGHT, win.width, 1, border_color, clip);
        }
    }

    pub fn handle_down(&mut self, x: usize, y: usize) -> bool {
        for i in (0..self.windows.len()).rev() {
            let win = &self.windows[i];
            if !win.visible { continue; }

            if x >= win.x && x < win.x + win.width && y >= win.y && y < win.y + win.height {
                let win_id = win.id;
                if let Some(dirty) = self.focus_window(win_id) {
                    crate::drivers::gfx::mark_dirty(dirty);
                }
                let top = match self.windows.last() {
                    Some(w) => w,
                    None => return false,
                };

                if x >= top.x + top.width - 22 && y <= top.y + 23 {
                    let dirty = top.outer_rect();
                    self.set_visible(win_id, false);
                    crate::drivers::gfx::mark_dirty(dirty);
                    crate::drivers::sound::click();
                    return true;
                }

                if y < top.y + WINDOW_TITLEBAR_HEIGHT {
                    self.dragging = Some(win_id);
                    self.drag_x = x - top.x;
                    self.drag_y = y - top.y;
                    return true;
                }

                self.handle_client_click(win_id, x.saturating_sub(top.x), y.saturating_sub(top.y + WINDOW_TITLEBAR_HEIGHT));
                return true;
            }
        }
        false
    }

    pub fn handle_client_click(&mut self, _window_id: usize, _rel_x: usize, _rel_y: usize) {
        crate::drivers::sound::click();
    }

    pub fn handle_drag(&mut self, x: usize, y: usize) -> Option<Rect> {
        if let Some(id) = self.dragging {
            let other_windows: Vec<(usize, usize, usize, usize)> = self.windows.iter()
                .filter(|w| w.id != id && w.visible)
                .map(|w| (w.x, w.y, w.width, w.height))
                .collect();

            if let Some(win) = self.windows.iter_mut().find(|w| w.id == id) {
                let old_rect = win.outer_rect();
                let max_x = crate::drivers::gfx::SCREEN_WIDTH.saturating_sub(win.width + WINDOW_SHADOW_OFFSET + 4);
                let max_y = crate::drivers::gfx::SCREEN_HEIGHT.saturating_sub(win.height + WINDOW_SHADOW_OFFSET + 1);

                let mut new_x = x.saturating_sub(self.drag_x).clamp(0, max_x);
                let mut new_y = y.saturating_sub(self.drag_y).clamp(TOP_PANEL_HEIGHT, max_y);

                if new_y >= TOP_PANEL_HEIGHT && new_y <= TOP_PANEL_HEIGHT + SNAP_THRESHOLD {
                    new_y = TOP_PANEL_HEIGHT;
                }

                if new_x <= SNAP_THRESHOLD {
                    new_x = 8;
                } else if new_x + win.width + WINDOW_SHADOW_OFFSET + 4 + SNAP_THRESHOLD >= crate::drivers::gfx::SCREEN_WIDTH {
                    new_x = max_x.saturating_sub(8);
                }

                for &(_ox, oy, _ow, _oh) in &other_windows {
                    let diff_y = (new_y as isize - oy as isize).abs();
                    if diff_y <= SNAP_THRESHOLD as isize {
                        new_y = oy;
                        break;
                    }
                }

                for &(ox, _oy, ow, _oh) in &other_windows {
                    let diff_x_left = (new_x as isize - ox as isize).abs();
                    if diff_x_left <= SNAP_THRESHOLD as isize {
                        new_x = ox;
                        break;
                    }
                    let diff_x_right = (new_x as isize - (ox + ow + 8) as isize).abs();
                    if diff_x_right <= SNAP_THRESHOLD as isize && ox + ow + 8 <= max_x {
                        new_x = ox + ow + 8;
                        break;
                    }
                }

                if win.x != new_x || win.y != new_y {
                    win.x = new_x;
                    win.y = new_y;
                    let new_rect = win.outer_rect();
                    return Some(old_rect.union(&new_rect).clamp(crate::drivers::gfx::SCREEN_WIDTH, crate::drivers::gfx::SCREEN_HEIGHT));
                }
            }
        }
        None
    }

    pub fn process_mouse_drag(&mut self, window_id: usize, dx: isize, dy: isize) -> Option<Rect> {
        let window = self.windows.iter_mut().find(|w| w.id == window_id)?;
        let old_rect = window.outer_rect();

        let max_x = crate::drivers::gfx::SCREEN_WIDTH.saturating_sub(window.width + WINDOW_SHADOW_OFFSET + 4) as isize;
        let max_y = crate::drivers::gfx::SCREEN_HEIGHT.saturating_sub(window.height + WINDOW_SHADOW_OFFSET + 1) as isize;

        let mut new_x = (window.x as isize + dx).clamp(0, max_x) as usize;
        let mut new_y = (window.y as isize + dy).clamp(TOP_PANEL_HEIGHT as isize, max_y) as usize;

        if new_y >= TOP_PANEL_HEIGHT && new_y <= TOP_PANEL_HEIGHT + SNAP_THRESHOLD {
            new_y = TOP_PANEL_HEIGHT;
        }
        if new_x <= SNAP_THRESHOLD {
            new_x = 8;
        }

        window.x = new_x;
        window.y = new_y;

        let new_rect = window.outer_rect();
        Some(old_rect.union(&new_rect).clamp(crate::drivers::gfx::SCREEN_WIDTH, crate::drivers::gfx::SCREEN_HEIGHT))
    }

    pub fn handle_up(&mut self) -> Option<Rect> {
        let dirty = if let Some(id) = self.dragging {
            self.windows.iter().find(|w| w.id == id).map(|w| w.outer_rect())
        } else {
            None
        };
        self.dragging = None;
        dirty
    }
}

pub static WM: Lazy<Mutex<WindowManager>> = Lazy::new(|| Mutex::new(WindowManager::new()));

pub fn create_window(title: &str, width: usize, height: usize) -> Option<usize> {
    let mut wm = WM.lock();
    let x = (crate::drivers::gfx::SCREEN_WIDTH.saturating_sub(width + WINDOW_SHADOW_OFFSET + 4)) / 2;
    let y = TOP_PANEL_HEIGHT + 8;
    let id = wm.create_window(title, x, y, width, height);
    if let Some(w) = wm.windows.iter_mut().find(|w| w.id == id) {
        w.visible = true;
    }
    if let Some(dirty) = wm.focus_window(id) {
        crate::drivers::gfx::mark_dirty(dirty);
    }
    Some(id)
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalDescriptor {
    pub window_id: usize,
}

impl TerminalDescriptor {
    pub fn new(window_id: usize) -> Self {
        Self { window_id }
    }

    pub fn write_str(&self, s: &str) {
        let mut wm = WM.lock();
        if let Some(win) = wm.windows.iter_mut().find(|w| w.id == self.window_id) {
            let prev_row = win.cursor_row;
            let scrolled = win.write_str(s);
            if scrolled {
                crate::drivers::gfx::mark_dirty(win.client_rect());
            } else {
                let line_h = 10;
                let min_r = prev_row.min(win.cursor_row);
                let max_r = prev_row.max(win.cursor_row);
                let y = win.y + WINDOW_TITLEBAR_HEIGHT + 8 + min_r * line_h;
                let h = (max_r - min_r + 1) * line_h;
                let dirty = Rect::new(win.x + 8, y, win.width.saturating_sub(16), h);
                crate::drivers::gfx::mark_dirty(dirty);
            }
        }
    }

    pub fn read_line(&self) -> String {
        let mut line = String::new();
        loop {
            crate::drivers::gfx::compositor_tick();

            if let Some(key) = crate::kernel::task::pop_key() {
                match key {
                    pc_keyboard::DecodedKey::Unicode(c) => match c {
                        '\n' | '\r' => {
                            self.write_str("\n");
                            break;
                        }
                        '\x08' => {
                            if !line.is_empty() {
                                line.pop();
                                self.write_str("\x08");
                            }
                        }
                        ch if ch.is_ascii() && !ch.is_ascii_control() => {
                            line.push(ch);
                            let mut tmp = [0u8; 4];
                            let s = ch.encode_utf8(&mut tmp);
                            self.write_str(s);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            } else {
                x86_64::instructions::hlt();
            }
        }
        line
    }

    pub fn poll_char(&self) -> Option<char> {
        if let Some(key) = crate::kernel::task::pop_key() {
            match key {
                pc_keyboard::DecodedKey::Unicode(c) => Some(c),
                _ => None,
            }
        } else {
            None
        }
    }
}

impl core::fmt::Write for TerminalDescriptor {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        (*self).write_str(s);
        Ok(())
    }
}

pub fn get_terminal_descriptor() -> TerminalDescriptor {
    let mut wm = WM.lock();
    let existing_id = wm.windows.iter().find(|w| w.title.contains("Terminal")).map(|w| w.id);
    if let Some(id) = existing_id {
        if let Some(w) = wm.windows.iter_mut().find(|w| w.id == id) {
            w.visible = true;
        }
        if let Some(dirty) = wm.focus_window(id) {
            crate::drivers::gfx::mark_dirty(dirty);
        }
        return TerminalDescriptor::new(id);
    }
    let id = wm.create_window("Terminal [zsh/sh]", 80, 50, 640, 420);
    if let Some(w) = wm.windows.iter_mut().find(|w| w.id == id) {
        w.visible = true;
    }
    if let Some(dirty) = wm.focus_window(id) {
        crate::drivers::gfx::mark_dirty(dirty);
    }
    TerminalDescriptor::new(id)
}

fn draw_rect_clipped(x: usize, y: usize, w: usize, h: usize, color: u32, clip: Rect) {
    let r_start = y.max(clip.y);
    let r_end = (y + h).min(clip.y + clip.height).min(crate::drivers::gfx::SCREEN_HEIGHT);
    let c_start = x.max(clip.x);
    let c_end = (x + w).min(clip.x + clip.width).min(crate::drivers::gfx::SCREEN_WIDTH);

    if r_start >= r_end || c_start >= c_end {
        return;
    }

    let bb = crate::drivers::gfx::BB_VIRT_ADDR as *mut u32;
    for r in r_start..r_end {
        let ptr = unsafe { bb.add(r * crate::drivers::gfx::SCREEN_WIDTH) };
        for c in c_start..c_end {
            unsafe {
                let alpha = (color >> 24) & 0xFF;
                if alpha == 255 {
                    *ptr.add(c) = color & 0x00FFFFFF;
                } else if alpha > 0 {
                    let bg = *ptr.add(c);
                    *ptr.add(c) = alpha_blend(color, bg);
                }
            }
        }
    }
}

fn draw_text_to_buffer(buffer: &mut [u32], stride: usize, x: usize, y: usize, text: &str, color: u32) {
    let mut cx = x;
    let total_h = buffer.len() / stride;
    for c in text.chars() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(c) {
            for (yo, &glyph_byte) in glyph.iter().enumerate() {
                let py = y + yo;
                if py < total_h {
                    for xo in 0..8 {
                        let px = cx + xo;
                        if px < stride && (glyph_byte & (1 << xo)) != 0 {
                            buffer[py * stride + px] = color;
                        }
                    }
                }
            }
        }
        cx += 8;
    }
}