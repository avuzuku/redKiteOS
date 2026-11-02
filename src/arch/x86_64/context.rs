use crate::kernel::process::Context;

#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(prev: *mut Context, next: *const Context) {
    core::arch::naked_asm!(
        "mov [rdi + 0x00], r15",
        "mov [rdi + 0x08], r14",
        "mov [rdi + 0x10], r13",
        "mov [rdi + 0x18], r12",
        "mov [rdi + 0x20], rbx",
        "mov [rdi + 0x28], rbp",
        "mov [rdi + 0x30], rsp",
        "lea rax, [rip + 1f]",
        "mov [rdi + 0x38], rax",
        "mov r15, [rsi + 0x00]",
        "mov r14, [rsi + 0x08]",
        "mov r13, [rsi + 0x10]",
        "mov r12, [rsi + 0x18]",
        "mov rbx, [rsi + 0x20]",
        "mov rbp, [rsi + 0x28]",
        "mov rsp, [rsi + 0x30]",
        "jmp qword ptr [rsi + 0x38]",
        "1:",
        "ret"
    );
}