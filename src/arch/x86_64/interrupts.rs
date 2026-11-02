use pic8259::ChainedPics;
use spin::{Lazy, Mutex};
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.divide_error.set_handler_fn(divide_error_handler);
    idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
    idt.general_protection_fault.set_handler_fn(gpf_handler);
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt.stack_segment_fault.set_handler_fn(stack_segment_fault_handler);
    idt.segment_not_present.set_handler_fn(segment_not_present_handler);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(crate::arch::x86_64::gdt::DOUBLE_FAULT_IST_INDEX);
    }
    idt[PIC_1_OFFSET as usize].set_handler_fn(timer_handler);
    idt[(PIC_1_OFFSET + 1) as usize].set_handler_fn(keyboard_handler);
    idt[(PIC_1_OFFSET + 12) as usize].set_handler_fn(mouse_handler);
    idt
});

pub fn init_idt() {
    IDT.load();
}

extern "x86-interrupt" fn divide_error_handler(sf: InterruptStackFrame) {
    crate::println!();
    crate::println!("==================================================");
    crate::println!("  [!] BAKA! MATH COLLAPSE: DIVIDE BY ZERO (#DE)   ");
    crate::println!("==================================================");
    crate::println!("Bro really divided by zero! Calculators are crying, baka!");
    crate::println!("Faulting IP: {:?}", sf.instruction_pointer);
    crate::println!("==================================================");
    crate::arch::x86_64::hlt_loop();
}

extern "x86-interrupt" fn invalid_opcode_handler(sf: InterruptStackFrame) {
    crate::println!();
    crate::println!("==================================================");
    crate::println!("  [!] N-NANI?! INVALID OPCODE DETECTED (#UD)      ");
    crate::println!("==================================================");
    crate::println!("The CPU doesn't know this instruction, Senpai!");
    crate::println!("What weird alien payload did you feed me? uwu");
    crate::println!("Faulting IP: {:?}", sf.instruction_pointer);
    crate::println!("==================================================");
    crate::arch::x86_64::hlt_loop();
}

extern "x86-interrupt" fn gpf_handler(sf: InterruptStackFrame, code: u64) {
    crate::println!();
    crate::println!("==================================================");
    crate::println!("  [!] YAMETE KUDASAI! GENERAL PROTECTION FAULT    ");
    crate::println!("==================================================");
    crate::println!("Hardware privilege violated! You touched a forbidden register!");
    crate::println!("Tsundere CPU refuses your illegal access request!");
    crate::println!("Error Code: {:#x} | IP: {:?}", code, sf.instruction_pointer);
    crate::println!("==================================================");
    crate::arch::x86_64::hlt_loop();
}

extern "x86-interrupt" fn page_fault_handler(sf: InterruptStackFrame, code: PageFaultErrorCode) {
    use x86_64::registers::control::Cr2;
    crate::println!();
    crate::println!("==================================================");
    crate::println!("  [!] SHINIGAMI MEMORY TRESPASS: PAGE FAULT (#PF) ");
    crate::println!("==================================================");
    crate::println!("Non-canonical address accessed: {:?}", Cr2::read());
    crate::println!("Flags: {:?} | Faulting IP: {:?}", code, sf.instruction_pointer);
    crate::println!("You fell right into my Domain Expansion: Virtual Void!");
    crate::println!("==================================================");
    crate::arch::x86_64::hlt_loop();
}

extern "x86-interrupt" fn stack_segment_fault_handler(sf: InterruptStackFrame, code: u64) {
    crate::println!();
    crate::println!("==================================================");
    crate::println!("  [!] OMAE WA MOU SHINDEIRU: STACK SEGMENT FAULT  ");
    crate::println!("==================================================");
    crate::println!("Stack limits exceeded! Senpai broke the stack frame!");
    crate::println!("Code: {:#x} | IP: {:?}", code, sf.instruction_pointer);
    crate::println!("==================================================");
    crate::arch::x86_64::hlt_loop();
}

extern "x86-interrupt" fn segment_not_present_handler(sf: InterruptStackFrame, code: u64) {
    crate::println!();
    crate::println!("==================================================");
    crate::println!("  [!] GOMENASAI! SEGMENT NOT PRESENT (#NP)        ");
    crate::println!("==================================================");
    crate::println!("The requested descriptor vanished into another dimension!");
    crate::println!("Selector index: {:#x} | IP: {:?}", code, sf.instruction_pointer);
    crate::println!("==================================================");
    crate::arch::x86_64::hlt_loop();
}

extern "x86-interrupt" fn double_fault_handler(sf: InterruptStackFrame, code: u64) -> ! {
    crate::println!();
    crate::println!("==================================================");
    crate::println!("  [!] AAAAAAAAA! DOUBLE FAULT BRAIN MELTDOWN      ");
    crate::println!("==================================================");
    crate::println!("Fatal cascade! Even Magical Girls can't fix this mess!");
    crate::println!("Code: {:#x} | IP: {:?}", code, sf.instruction_pointer);
    crate::println!("==================================================");
    crate::arch::x86_64::hlt_loop();
}

extern "x86-interrupt" fn timer_handler(_sf: InterruptStackFrame) {
    crate::kernel::timer::tick();
    unsafe { PICS.lock().notify_end_of_interrupt(PIC_1_OFFSET); }
}

extern "x86-interrupt" fn keyboard_handler(_sf: InterruptStackFrame) {
    let sc = unsafe { crate::arch::x86_64::io::inb(0x60) };
    crate::kernel::task::add_scancode(sc);
    unsafe { PICS.lock().notify_end_of_interrupt(PIC_1_OFFSET + 1); }
}

extern "x86-interrupt" fn mouse_handler(_sf: InterruptStackFrame) {
    let b = unsafe { crate::arch::x86_64::io::inb(0x60) };
    crate::drivers::mouse::MOUSE_QUEUE.push(b);
    unsafe { PICS.lock().notify_end_of_interrupt(PIC_1_OFFSET + 12); }
}