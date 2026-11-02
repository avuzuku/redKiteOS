use spin::Lazy;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

#[repr(align(16))]
struct Stack<const N: usize>([u8; N]);

static mut DOUBLE_FAULT_STACK: Stack<{ 4096 * 5 }> = Stack([0; 4096 * 5]);
static mut KERNEL_STACK: Stack<{ 4096 * 8 }> = Stack([0; 4096 * 8]);

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        let stack_start = VirtAddr::from_ptr(&raw const DOUBLE_FAULT_STACK);
        stack_start + (4096usize * 5)
    };
    tss.privilege_stack_table[0] = {
        let stack_start = VirtAddr::from_ptr(&raw const KERNEL_STACK);
        stack_start + (4096usize * 8)
    };
    tss
});

static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let kernel_code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
    let kernel_data_selector = gdt.add_entry(Descriptor::kernel_data_segment());
    let user_data_selector = gdt.add_entry(Descriptor::user_data_segment());
    let user_code_selector = gdt.add_entry(Descriptor::user_code_segment());
    let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));
    (
        gdt,
        Selectors {
            kernel_code_selector,
            kernel_data_selector,
            user_data_selector,
            user_code_selector,
            tss_selector,
        },
    )
});

pub struct Selectors {
    pub kernel_code_selector: SegmentSelector,
    pub kernel_data_selector: SegmentSelector,
    pub user_data_selector: SegmentSelector,
    pub user_code_selector: SegmentSelector,
    pub tss_selector: SegmentSelector,
}

pub fn selectors() -> &'static Selectors {
    &GDT.1
}

pub fn kernel_stack_top() -> VirtAddr {
    let stack_start = VirtAddr::from_ptr(&raw const KERNEL_STACK);
    stack_start + (4096usize * 8)
}

pub fn set_interrupt_kernel_stack(stack_top: VirtAddr) {
    unsafe {
        let tss_ptr = &raw const *TSS as *mut TaskStateSegment;
        (*tss_ptr).privilege_stack_table[0] = stack_top;
    }
}

pub fn init() {
    use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.kernel_code_selector);
        DS::set_reg(GDT.1.kernel_data_selector);
        ES::set_reg(GDT.1.kernel_data_selector);
        SS::set_reg(GDT.1.kernel_data_selector);
        load_tss(GDT.1.tss_selector);
    }
}