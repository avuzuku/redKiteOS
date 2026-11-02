use alloc::boxed::Box;
use alloc::collections::VecDeque;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use spin::{Lazy, Mutex};
use x86_64::instructions::interrupts;

static RAW_SCANCODE_QUEUE: Lazy<Mutex<VecDeque<u8>>> = Lazy::new(|| Mutex::new(VecDeque::with_capacity(256)));
static DECODED_KEY_QUEUE: Lazy<Mutex<VecDeque<DecodedKey>>> = Lazy::new(|| Mutex::new(VecDeque::with_capacity(128)));
static TASK_QUEUE: Lazy<Mutex<VecDeque<Box<dyn FnOnce() + Send + 'static>>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(32)));

static CTRL_PRESSED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

static KEYBOARD: Lazy<Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>>> = Lazy::new(|| {
    Mutex::new(Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::MapLettersToUnicode,
    ))
});

pub fn spawn<F: FnOnce() + Send + 'static>(f: F) {
    let mut q = TASK_QUEUE.lock();
    q.push_back(Box::new(f));
}

pub fn poll_tasks() {
    let task = {
        let mut q = TASK_QUEUE.lock();
        q.pop_front()
    };
    if let Some(task) = task {
        task();
    }
}

pub fn add_scancode(scancode: u8) {
    match scancode {
        0x1D => CTRL_PRESSED.store(true, core::sync::atomic::Ordering::Relaxed),
        0x9D => CTRL_PRESSED.store(false, core::sync::atomic::Ordering::Relaxed),
        _ => {}
    }

    interrupts::without_interrupts(|| {
        let mut raw_q = RAW_SCANCODE_QUEUE.lock();
        if raw_q.len() < 256 {
            raw_q.push_back(scancode);
        }

        let mut kb = KEYBOARD.lock();
        if let Ok(Some(key_event)) = kb.add_byte(scancode) {
            if let Some(key) = kb.process_keyevent(key_event) {
                let mut dec_q = DECODED_KEY_QUEUE.lock();
                let ctrl = CTRL_PRESSED.load(core::sync::atomic::Ordering::Relaxed);

                let final_key = if ctrl {
                    match key {
                        DecodedKey::Unicode('c') | DecodedKey::Unicode('C') | DecodedKey::Unicode('\x03') => {
                            DecodedKey::Unicode('\x03')
                        }
                        DecodedKey::Unicode('o') | DecodedKey::Unicode('O') | DecodedKey::Unicode('\x0F') => {
                            DecodedKey::Unicode('\x0F')
                        }
                        DecodedKey::Unicode('x') | DecodedKey::Unicode('X') | DecodedKey::Unicode('\x18') => {
                            DecodedKey::Unicode('\x18')
                        }
                        other => other,
                    }
                } else {
                    key
                };

                if dec_q.len() < 128 {
                    dec_q.push_back(final_key);
                }
            }
        }
    });
}

pub fn pop_key() -> Option<DecodedKey> {
    interrupts::without_interrupts(|| DECODED_KEY_QUEUE.lock().pop_front())
}

pub fn clear_key_queue() {
    interrupts::without_interrupts(|| {
        DECODED_KEY_QUEUE.lock().clear();
        RAW_SCANCODE_QUEUE.lock().clear();
    });
}

pub fn wait_for_key() -> DecodedKey {
    loop {
        if let Some(key) = pop_key() {
            return key;
        }
        x86_64::instructions::hlt();
    }
}